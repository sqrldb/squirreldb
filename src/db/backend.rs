use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::storage::{ObjectAcl, StorageBucket, MultipartUpload, StorageObject, MultipartPart};
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
#[allow(clippy::too_many_arguments)]
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
    offset: Option<usize>,
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

  // =========================================================================
  // Object Storage Methods
  // =========================================================================

  // Storage Access Key methods
  /// Get storage access key and owner ID for authentication
  async fn get_storage_access_key(
    &self,
    access_key_id: &str,
  ) -> Result<Option<(String, Option<Uuid>)>, anyhow::Error>;

  /// Create a new storage access key
  async fn create_storage_access_key(
    &self,
    access_key_id: &str,
    secret_key: &str,
    owner_id: Option<Uuid>,
    name: &str,
  ) -> Result<(), anyhow::Error>;

  /// Delete a storage access key
  async fn delete_storage_access_key(&self, access_key_id: &str) -> Result<bool, anyhow::Error>;

  /// List all storage access keys
  async fn list_storage_access_keys(&self) -> Result<Vec<StorageAccessKeyInfo>, anyhow::Error>;

  // Storage Bucket methods
  /// Get a bucket by name
  async fn get_storage_bucket(&self, name: &str) -> Result<Option<StorageBucket>, anyhow::Error>;

  /// Create a new bucket
  async fn create_storage_bucket(&self, name: &str, owner_id: Option<Uuid>)
    -> Result<(), anyhow::Error>;

  /// Delete a bucket
  async fn delete_storage_bucket(&self, name: &str) -> Result<(), anyhow::Error>;

  /// List all buckets
  async fn list_storage_buckets(&self) -> Result<Vec<StorageBucket>, anyhow::Error>;

  /// Update bucket stats (size and object count)
  async fn update_storage_bucket_stats(
    &self,
    bucket: &str,
    size_delta: i64,
    count_delta: i64,
  ) -> Result<(), anyhow::Error>;

  // Storage Object methods
  /// Get an object by bucket, key, and optional version
  async fn get_storage_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Option<Uuid>,
  ) -> Result<Option<StorageObject>, anyhow::Error>;

  /// Create a new object
  async fn create_storage_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    etag: &str,
    size: i64,
    content_type: &str,
    storage_path: &str,
    metadata: serde_json::Value,
  ) -> Result<(), anyhow::Error>;

  /// Delete an object (specific version or all if version_id is None)
  async fn delete_storage_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Option<Uuid>,
  ) -> Result<(), anyhow::Error>;

  /// Create a delete marker (for versioned buckets)
  async fn create_storage_delete_marker(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
  ) -> Result<(), anyhow::Error>;

  /// Mark all versions of an object as not latest
  async fn unset_storage_object_latest(&self, bucket: &str, key: &str) -> Result<(), anyhow::Error>;

  /// Update object ACL
  async fn update_storage_object_acl(
    &self,
    bucket: &str,
    key: &str,
    acl: ObjectAcl,
  ) -> Result<(), anyhow::Error>;

  /// List objects in a bucket
  async fn list_storage_objects(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    delimiter: Option<&str>,
    max_keys: i32,
    continuation_token: Option<&str>,
  ) -> Result<(Vec<StorageObject>, bool, Option<String>), anyhow::Error>;

  /// List common prefixes (for delimiter-based listing)
  async fn list_storage_common_prefixes(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    delimiter: Option<&str>,
  ) -> Result<Vec<String>, anyhow::Error>;

  /// List object versions
  async fn list_storage_object_versions(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    max_keys: i32,
  ) -> Result<(Vec<StorageObject>, bool, Option<String>), anyhow::Error>;

  // Multipart Upload methods
  /// Get a multipart upload by ID
  async fn get_multipart_upload(
    &self,
    upload_id: Uuid,
  ) -> Result<Option<MultipartUpload>, anyhow::Error>;

  /// Create a new multipart upload
  async fn create_multipart_upload(
    &self,
    upload_id: Uuid,
    bucket: &str,
    key: &str,
    content_type: Option<&str>,
    metadata: serde_json::Value,
  ) -> Result<(), anyhow::Error>;

  /// Delete a multipart upload and its parts
  async fn delete_multipart_upload(&self, upload_id: Uuid) -> Result<(), anyhow::Error>;

  /// List multipart uploads for a bucket
  async fn list_multipart_uploads(
    &self,
    bucket: &str,
    max_uploads: i32,
  ) -> Result<(Vec<MultipartUpload>, bool), anyhow::Error>;

  /// Get a multipart part
  async fn get_multipart_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
  ) -> Result<Option<MultipartPart>, anyhow::Error>;

  /// Create or update a multipart part
  async fn upsert_multipart_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
    etag: &str,
    size: i64,
    storage_path: &str,
  ) -> Result<(), anyhow::Error>;

  /// List parts for a multipart upload
  async fn list_multipart_parts(
    &self,
    upload_id: Uuid,
    max_parts: i32,
  ) -> Result<(Vec<MultipartPart>, bool), anyhow::Error>;

  // =========================================================================
  // Feature Settings Methods
  // =========================================================================

  /// Get feature settings from database
  async fn get_feature_settings(
    &self,
    name: &str,
  ) -> Result<Option<(bool, serde_json::Value)>, anyhow::Error>;

  /// Update feature settings in database
  async fn update_feature_settings(
    &self,
    name: &str,
    enabled: bool,
    settings: serde_json::Value,
  ) -> Result<(), anyhow::Error>;

  // =========================================================================
  // Storage Atomic Operations (reduces round-trips)
  // =========================================================================

  /// Atomic object creation with bucket stats update (saves 1 round-trip)
  async fn create_storage_object_with_stats(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    etag: &str,
    size: i64,
    content_type: &str,
    storage_path: &str,
    metadata: serde_json::Value,
  ) -> Result<(), anyhow::Error>;

  /// Atomic object deletion with stats update (saves 2 round-trips)
  /// Returns (storage_path, size) if object was found and deleted, None otherwise
  async fn delete_storage_object_with_stats(
    &self,
    bucket: &str,
    key: &str,
    version_id: Option<Uuid>,
  ) -> Result<Option<(String, i64)>, anyhow::Error>;

  /// Atomic object replacement for non-versioned buckets
  /// Returns old storage_path for file cleanup, None if new object
  async fn replace_storage_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    etag: &str,
    size: i64,
    content_type: &str,
    storage_path: &str,
    metadata: serde_json::Value,
  ) -> Result<Option<String>, anyhow::Error>;

  /// Combined objects and prefixes listing (saves 1 round-trip)
  async fn list_storage_objects_with_prefixes(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    delimiter: Option<&str>,
    max_keys: i32,
    continuation_token: Option<&str>,
  ) -> Result<(Vec<StorageObject>, Vec<String>, bool, Option<String>), anyhow::Error>;
}

/// Storage access key metadata (without the actual secret)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAccessKeyInfo {
  pub access_key_id: String,
  pub owner_id: Option<Uuid>,
  pub name: String,
  pub created_at: DateTime<Utc>,
}
