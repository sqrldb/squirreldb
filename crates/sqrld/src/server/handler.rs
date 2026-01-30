use std::sync::Arc;
use uuid::Uuid;

use crate::db::DatabaseBackend;
use crate::query::QueryEnginePool;
use crate::subscriptions::SubscriptionManager;
use crate::types::{ClientMessage, QueryInput, ServerMessage, DEFAULT_PROJECT_ID};

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

  /// Execute a query, routing to structured or JS execution based on input type
  async fn execute_query(&self, query: &QueryInput) -> Result<serde_json::Value, anyhow::Error> {
    match query {
      QueryInput::Structured(q) => {
        self
          .engine_pool
          .execute_structured(q, self.backend.as_ref())
          .await
      }
      QueryInput::Script(script) => {
        self
          .engine_pool
          .execute(script, self.backend.as_ref())
          .await
      }
    }
  }

  /// Parse a query into a QuerySpec, routing based on input type
  fn parse_query(&self, query: &QueryInput) -> Result<crate::types::QuerySpec, anyhow::Error> {
    match query {
      QueryInput::Structured(q) => self.engine_pool.parse_structured(q),
      QueryInput::Script(script) => self.engine_pool.parse_query(script),
    }
  }

  pub async fn handle(&self, client_id: Uuid, msg: ClientMessage) -> ServerMessage {
    match msg {
      ClientMessage::Query { id, query } => match self.execute_query(&query).await {
        Ok(data) => ServerMessage::result(id, data),
        Err(e) => ServerMessage::error(id, e.to_string()),
      },
      ClientMessage::Subscribe { id, query } => match self.parse_query(&query) {
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
      ClientMessage::SelectProject { id, project_id } => {
        // TODO: Store project context for this client
        ServerMessage::ProjectSelected { id, project_id }
      }
      ClientMessage::Insert {
        id,
        collection,
        data,
      } => match self.backend.insert(DEFAULT_PROJECT_ID, &collection, data).await {
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
      } => match self
        .backend
        .update(DEFAULT_PROJECT_ID, &collection, document_id, data)
        .await
      {
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
      } => match self
        .backend
        .delete(DEFAULT_PROJECT_ID, &collection, document_id)
        .await
      {
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
      ClientMessage::ListCollections { id } => {
        match self.backend.list_collections(DEFAULT_PROJECT_ID).await {
          Ok(cols) => match serde_json::to_value(cols) {
            Ok(v) => ServerMessage::result(id, v),
            Err(e) => ServerMessage::error(id, format!("Serialization error: {}", e)),
          },
          Err(e) => ServerMessage::error(id, e.to_string()),
        }
      }
      ClientMessage::ListProjects { id } => match self.backend.list_projects().await {
        Ok(projects) => match serde_json::to_value(projects) {
          Ok(v) => ServerMessage::result(id, v),
          Err(e) => ServerMessage::error(id, format!("Serialization error: {}", e)),
        },
        Err(e) => ServerMessage::error(id, e.to_string()),
      },
      ClientMessage::Ping { id } => ServerMessage::pong(id),
    }
  }
}
