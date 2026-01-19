use parking_lot::RwLock;
use rquickjs::{Context, Runtime};
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::types::{Change, ChangeEvent, ChangeOperation, Document, QuerySpec, ServerMessage};

#[derive(Clone)]
struct Subscription {
  id: String,
  query: QuerySpec,
}

/// Manages subscriptions with O(1) lookup by collection.
/// Uses a collection index to eliminate O(N×M) iteration when processing changes.
pub struct SubscriptionManager {
  /// Client ID -> (Subscription ID -> Subscription)
  subs: RwLock<HashMap<Uuid, HashMap<String, Subscription>>>,
  /// Collection name -> Vec<(Client ID, Subscription ID)>
  /// This index enables O(S) lookup where S = subscriptions for that collection
  collection_index: RwLock<HashMap<String, Vec<(Uuid, String)>>>,
  out_tx: broadcast::Sender<(Uuid, ServerMessage)>,
  runtime: Runtime,
}

impl SubscriptionManager {
  pub fn new() -> Self {
    let (out_tx, _) = broadcast::channel(4096);
    let runtime = Runtime::new().expect("JS runtime");
    runtime.set_memory_limit(10 * 1024 * 1024);
    Self {
      subs: RwLock::new(HashMap::new()),
      collection_index: RwLock::new(HashMap::new()),
      out_tx,
      runtime,
    }
  }

  pub fn subscribe_to_outgoing(&self) -> broadcast::Receiver<(Uuid, ServerMessage)> {
    self.out_tx.subscribe()
  }

  pub fn add_subscription(&self, client: Uuid, id: String, query: QuerySpec) {
    let collection = query.table.clone();

    // Add to main subscriptions map
    self.subs.write().entry(client).or_default().insert(
      id.clone(),
      Subscription {
        id: id.clone(),
        query,
      },
    );

    // Add to collection index for O(1) lookup
    self
      .collection_index
      .write()
      .entry(collection)
      .or_default()
      .push((client, id));
  }

  pub fn remove_subscription(&self, client: Uuid, id: &str) {
    let mut subs = self.subs.write();
    if let Some(client_subs) = subs.get_mut(&client) {
      // Get the collection name before removing
      if let Some(sub) = client_subs.get(id) {
        let collection = sub.query.table.clone();
        client_subs.remove(id);

        // Remove from collection index
        let mut index = self.collection_index.write();
        if let Some(entries) = index.get_mut(&collection) {
          entries.retain(|(c, s)| !(*c == client && s == id));
          if entries.is_empty() {
            index.remove(&collection);
          }
        }
      }
    }
  }

  pub fn remove_client(&self, client: Uuid) {
    let mut subs = self.subs.write();
    if let Some(client_subs) = subs.remove(&client) {
      // Remove all subscriptions from collection index
      let mut index = self.collection_index.write();
      for (sub_id, sub) in client_subs {
        if let Some(entries) = index.get_mut(&sub.query.table) {
          entries.retain(|(c, s)| !(*c == client && s == &sub_id));
          if entries.is_empty() {
            index.remove(&sub.query.table);
          }
        }
      }
    }
  }

  pub async fn process_changes(&self, mut rx: broadcast::Receiver<Change>) {
    while let Ok(change) = rx.recv().await {
      // Use the collection index for O(S) lookup instead of O(N×M) iteration
      let index = self.collection_index.read();
      let Some(subscriptions) = index.get(&change.collection) else {
        continue;
      };

      // Only check subscriptions for this collection
      let subs = self.subs.read();
      for (client_id, sub_id) in subscriptions {
        if let Some(client_subs) = subs.get(client_id) {
          if let Some(sub) = client_subs.get(sub_id) {
            if self.matches(&sub.query, &change) {
              if let Some(evt) = self.to_event(&sub.query, &change) {
                let _ = self
                  .out_tx
                  .send((*client_id, ServerMessage::change(&sub.id, evt)));
              }
            }
          }
        }
      }
    }
  }

  fn matches(&self, query: &QuerySpec, change: &Change) -> bool {
    let Some(filter) = &query.filter else {
      return true;
    };
    if filter.compiled_sql.is_some() {
      return true;
    }
    let data = match change.operation {
      ChangeOperation::Delete => change.old_data.as_ref(),
      _ => change.new_data.as_ref(),
    };
    let Some(data) = data else { return false };
    let json_str = match serde_json::to_string(data) {
      Ok(s) => s,
      Err(_) => return false,
    };
    Context::full(&self.runtime)
      .ok()
      .map(|ctx| {
        ctx.with(|ctx| {
          ctx
            .eval::<bool, _>(format!("(({})({}));", filter.js_code, json_str))
            .unwrap_or(false)
        })
      })
      .unwrap_or(false)
  }

  fn to_event(&self, query: &QuerySpec, change: &Change) -> Option<ChangeEvent> {
    let map_data = |d: &serde_json::Value| -> serde_json::Value {
      query
        .map
        .as_ref()
        .and_then(|m| {
          Context::full(&self.runtime)
            .ok()
            .and_then(|ctx| {
              ctx.with(|ctx| {
                ctx
                  .eval::<String, _>(format!(
                    "JSON.stringify(({})({}));",
                    m,
                    serde_json::to_string(d).ok()?
                  ))
                  .ok()
              })
            })
            .and_then(|s| serde_json::from_str(&s).ok())
        })
        .unwrap_or_else(|| d.clone())
    };

    match change.operation {
      ChangeOperation::Insert => {
        let data = map_data(change.new_data.as_ref()?);
        Some(ChangeEvent::Insert {
          new: Document {
            id: change.document_id,
            collection: change.collection.clone(),
            data,
            created_at: change.changed_at,
            updated_at: change.changed_at,
          },
        })
      }
      ChangeOperation::Update => {
        let old = map_data(change.old_data.as_ref()?);
        let new = map_data(change.new_data.as_ref()?);
        Some(ChangeEvent::Update {
          old,
          new: Document {
            id: change.document_id,
            collection: change.collection.clone(),
            data: new,
            created_at: change.changed_at,
            updated_at: change.changed_at,
          },
        })
      }
      ChangeOperation::Delete => {
        let data = map_data(change.old_data.as_ref()?);
        Some(ChangeEvent::Delete {
          old: Document {
            id: change.document_id,
            collection: change.collection.clone(),
            data,
            created_at: change.changed_at,
            updated_at: change.changed_at,
          },
        })
      }
    }
  }
}

impl Default for SubscriptionManager {
  fn default() -> Self {
    Self::new()
  }
}
