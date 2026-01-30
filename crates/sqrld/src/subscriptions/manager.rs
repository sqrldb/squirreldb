use parking_lot::RwLock;
use rquickjs::{Context, Runtime};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::db::DatabaseBackend;
use crate::types::{Change, ChangeEvent, ChangeOperation, Document, QuerySpec, ServerMessage};

#[derive(Clone)]
struct Subscription {
  id: String,
  query: QuerySpec,
}

/// Manages subscriptions with O(1) lookup by collection.
/// Uses a collection index to eliminate O(N×M) iteration when processing changes.
/// Also registers compiled SQL filters in PostgreSQL for server-side filtering.
pub struct SubscriptionManager {
  /// Client ID -> (Subscription ID -> Subscription)
  subs: RwLock<HashMap<Uuid, HashMap<String, Subscription>>>,
  /// Collection name -> Vec<(Client ID, Subscription ID)>
  /// This index enables O(S) lookup where S = subscriptions for that collection
  collection_index: RwLock<HashMap<String, Vec<(Uuid, String)>>>,
  out_tx: broadcast::Sender<(Uuid, ServerMessage)>,
  runtime: Runtime,
  /// Optional database backend for registering subscription filters in PostgreSQL
  backend: Option<Arc<dyn DatabaseBackend>>,
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
      backend: None,
    }
  }

  /// Create a SubscriptionManager with a database backend for PostgreSQL-side filtering
  pub fn with_backend(backend: Arc<dyn DatabaseBackend>) -> Self {
    let (out_tx, _) = broadcast::channel(4096);
    let runtime = Runtime::new().expect("JS runtime");
    runtime.set_memory_limit(10 * 1024 * 1024);
    Self {
      subs: RwLock::new(HashMap::new()),
      collection_index: RwLock::new(HashMap::new()),
      out_tx,
      runtime,
      backend: Some(backend),
    }
  }

  pub fn subscribe_to_outgoing(&self) -> broadcast::Receiver<(Uuid, ServerMessage)> {
    self.out_tx.subscribe()
  }

  /// Add a subscription and optionally register its SQL filter in PostgreSQL
  pub async fn add_subscription(&self, client: Uuid, id: String, query: QuerySpec) {
    let collection = query.table.clone();

    // Extract compiled SQL filter if available (for PostgreSQL-side filtering)
    let compiled_sql = query
      .filter
      .as_ref()
      .and_then(|f| f.compiled_sql.as_ref())
      .map(|s| s.as_str());

    // Register filter in PostgreSQL for server-side filtering (if backend available)
    if let Some(ref backend) = self.backend {
      if let Err(e) = backend
        .add_subscription_filter(client, &id, &collection, compiled_sql)
        .await
      {
        tracing::warn!("Failed to register subscription filter in DB: {}", e);
      }
    }

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

  /// Remove a subscription and unregister its filter from PostgreSQL
  pub async fn remove_subscription(&self, client: Uuid, id: &str) {
    // Remove filter from PostgreSQL
    if let Some(ref backend) = self.backend {
      if let Err(e) = backend.remove_subscription_filter(client, id).await {
        tracing::warn!("Failed to remove subscription filter from DB: {}", e);
      }
    }

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

  /// Remove all subscriptions for a client and unregister their filters from PostgreSQL
  pub async fn remove_client(&self, client: Uuid) {
    // Remove all filters for this client from PostgreSQL
    if let Some(ref backend) = self.backend {
      if let Err(e) = backend.remove_client_filters(client).await {
        tracing::warn!("Failed to remove client filters from DB: {}", e);
      }
    }

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
            project_id: change.project_id,
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
            project_id: change.project_id,
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
            project_id: change.project_id,
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
