use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::types::{Change, Document, OrderBySpec};

/// API token metadata (without the actual secret)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTokenInfo {
  pub id: Uuid,
  pub name: String,
  pub created_at: DateTime<Utc>,
}

/// SQL dialect for query compilation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
  Postgres,
  Sqlite,
}

impl SqlDialect {
  /// Generate SQL for accessing a JSON string field
  pub fn json_text(&self, field: &str) -> String {
    // Handle nested fields by converting dots to proper JSON path
    let path = self.field_to_path(field);
    match self {
      Self::Postgres => format!("data{}", path),
      Self::Sqlite => format!("json_extract(data, '$.{}')", field),
    }
  }

  /// Generate SQL for accessing a JSON numeric field
  pub fn json_numeric(&self, field: &str) -> String {
    // Handle nested fields by converting dots to proper JSON path
    let path = self.field_to_path(field);
    match self {
      Self::Postgres => format!("(data{})::numeric", path.replace("->>", "->")),
      Self::Sqlite => format!("CAST(json_extract(data, '$.{}') AS REAL)", field),
    }
  }

  /// Generate SQL for accessing a JSON boolean field
  pub fn json_bool(&self, field: &str) -> String {
    let path = self.field_to_path(field);
    match self {
      Self::Postgres => format!("(data{})::boolean", path.replace("->>", "->")),
      Self::Sqlite => format!("json_extract(data, '$.{}')", field),
    }
  }

  /// Generate SQL for ordering by a JSON field
  pub fn json_order(&self, field: &str) -> String {
    self.json_text(field)
  }

  /// Convert a dotted field path to SQL JSON path syntax
  fn field_to_path(&self, field: &str) -> String {
    match self {
      Self::Postgres => {
        let parts: Vec<&str> = field.split('.').collect();
        if parts.len() == 1 {
          format!("->>'{}'", parts[0])
        } else {
          // For nested: data->'address'->>'city'
          let mut result = String::new();
          for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
              result.push_str(&format!("->>'{}'", part));
            } else {
              result.push_str(&format!("->'{}'", part));
            }
          }
          result
        }
      }
      Self::Sqlite => field.to_string(), // SQLite uses $.field.nested directly
    }
  }
}

/// Abstract database backend
#[async_trait]
pub trait DatabaseBackend: Send + Sync {
  fn dialect(&self) -> SqlDialect;

  async fn init_schema(&self) -> Result<(), anyhow::Error>;
  async fn drop_schema(&self) -> Result<(), anyhow::Error>;

  async fn insert(
    &self,
    collection: &str,
    data: serde_json::Value,
  ) -> Result<Document, anyhow::Error>;
  async fn get(&self, collection: &str, id: Uuid) -> Result<Option<Document>, anyhow::Error>;
  async fn update(
    &self,
    collection: &str,
    id: Uuid,
    data: serde_json::Value,
  ) -> Result<Option<Document>, anyhow::Error>;
  async fn delete(&self, collection: &str, id: Uuid) -> Result<Option<Document>, anyhow::Error>;
  async fn list(
    &self,
    collection: &str,
    filter: Option<&str>,
    order: Option<&OrderBySpec>,
    limit: Option<usize>,
  ) -> Result<Vec<Document>, anyhow::Error>;
  async fn list_collections(&self) -> Result<Vec<String>, anyhow::Error>;

  fn subscribe_changes(&self) -> broadcast::Receiver<Change>;
  async fn start_change_listener(&self) -> Result<(), anyhow::Error>;

  // Token management methods
  async fn create_token(&self, name: &str, token_hash: &str)
    -> Result<ApiTokenInfo, anyhow::Error>;
  async fn delete_token(&self, id: Uuid) -> Result<bool, anyhow::Error>;
  async fn list_tokens(&self) -> Result<Vec<ApiTokenInfo>, anyhow::Error>;
  async fn validate_token(&self, token_hash: &str) -> Result<bool, anyhow::Error>;

  // Subscription filter methods for PostgreSQL-side filtering
  /// Register a subscription filter in the database for efficient server-side filtering
  async fn add_subscription_filter(
    &self,
    client_id: Uuid,
    subscription_id: &str,
    collection: &str,
    compiled_sql: Option<&str>,
  ) -> Result<(), anyhow::Error>;

  /// Remove a subscription filter from the database
  async fn remove_subscription_filter(
    &self,
    client_id: Uuid,
    subscription_id: &str,
  ) -> Result<(), anyhow::Error>;

  /// Remove all subscription filters for a client
  async fn remove_client_filters(&self, client_id: Uuid) -> Result<u64, anyhow::Error>;

  // Rate limiting methods (for distributed rate limiting)
  /// Check if a request is allowed under rate limiting (atomic check and consume)
  async fn rate_limit_check(
    &self,
    ip: std::net::IpAddr,
    rate: u32,
    capacity: u32,
  ) -> Result<bool, anyhow::Error>;

  /// Acquire a connection slot for an IP address
  async fn connection_acquire(
    &self,
    ip: std::net::IpAddr,
    max_connections: u32,
  ) -> Result<bool, anyhow::Error>;

  /// Release a connection slot for an IP address
  async fn connection_release(&self, ip: std::net::IpAddr) -> Result<(), anyhow::Error>;
}
