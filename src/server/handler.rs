use std::sync::Arc;
use uuid::Uuid;

use crate::db::DatabaseBackend;
use crate::query::QueryEnginePool;
use crate::subscriptions::SubscriptionManager;
use crate::types::{ClientMessage, ServerMessage};

pub struct MessageHandler {
  backend: Arc<dyn DatabaseBackend>,
  subs: Arc<SubscriptionManager>,
  engine_pool: Arc<QueryEnginePool>,
}

impl MessageHandler {
  pub fn new(
    backend: Arc<dyn DatabaseBackend>,
    subs: Arc<SubscriptionManager>,
    engine_pool: Arc<QueryEnginePool>,
  ) -> Self {
    Self {
      backend,
      subs,
      engine_pool,
    }
  }

  pub async fn handle(&self, client_id: Uuid, msg: ClientMessage) -> ServerMessage {
    match msg {
      ClientMessage::Query { id, query } => {
        match self
          .engine_pool
          .execute(&query, self.backend.as_ref())
          .await
        {
          Ok(data) => ServerMessage::result(id, data),
          Err(e) => ServerMessage::error(id, e.to_string()),
        }
      }
      ClientMessage::Subscribe { id, query } => match self.engine_pool.parse_query(&query) {
        Ok(spec) => {
          self
            .subs
            .add_subscription(client_id, id.clone(), spec)
            .await;
          ServerMessage::subscribed(id)
        }
        Err(e) => ServerMessage::error(id, e.to_string()),
      },
      ClientMessage::Unsubscribe { id } => {
        self.subs.remove_subscription(client_id, &id).await;
        ServerMessage::Unsubscribed { id }
      }
      ClientMessage::Insert {
        id,
        collection,
        data,
      } => match self.backend.insert(&collection, data).await {
        Ok(doc) => {
          // Invalidate cache for this table after write
          self.engine_pool.invalidate_table(&collection);
          match serde_json::to_value(doc) {
            Ok(v) => ServerMessage::result(id, v),
            Err(e) => ServerMessage::error(id, format!("Serialization error: {}", e)),
          }
        }
        Err(e) => ServerMessage::error(id, e.to_string()),
      },
      ClientMessage::Update {
        id,
        collection,
        document_id,
        data,
      } => match self.backend.update(&collection, document_id, data).await {
        Ok(Some(doc)) => {
          // Invalidate cache for this table after write
          self.engine_pool.invalidate_table(&collection);
          match serde_json::to_value(doc) {
            Ok(v) => ServerMessage::result(id, v),
            Err(e) => ServerMessage::error(id, format!("Serialization error: {}", e)),
          }
        }
        Ok(None) => ServerMessage::error(
          id,
          format!(
            "Document {} not found in collection '{}'",
            document_id, collection
          ),
        ),
        Err(e) => ServerMessage::error(id, e.to_string()),
      },
      ClientMessage::Delete {
        id,
        collection,
        document_id,
      } => match self.backend.delete(&collection, document_id).await {
        Ok(Some(doc)) => {
          // Invalidate cache for this table after write
          self.engine_pool.invalidate_table(&collection);
          match serde_json::to_value(doc) {
            Ok(v) => ServerMessage::result(id, v),
            Err(e) => ServerMessage::error(id, format!("Serialization error: {}", e)),
          }
        }
        Ok(None) => ServerMessage::error(
          id,
          format!(
            "Document {} not found in collection '{}'",
            document_id, collection
          ),
        ),
        Err(e) => ServerMessage::error(id, e.to_string()),
      },
      ClientMessage::ListCollections { id } => match self.backend.list_collections().await {
        Ok(cols) => match serde_json::to_value(cols) {
          Ok(v) => ServerMessage::result(id, v),
          Err(e) => ServerMessage::error(id, format!("Serialization error: {}", e)),
        },
        Err(e) => ServerMessage::error(id, e.to_string()),
      },
      ClientMessage::Ping { id } => ServerMessage::pong(id),
    }
  }
}
