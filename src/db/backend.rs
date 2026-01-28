use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::s3::{ObjectAcl, S3Bucket, S3MultipartUpload, S3Object, S3Part};
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
  // S3 Storage Methods
  // =========================================================================

  // S3 Access Key methods
  /// Get S3 access key and owner ID for authentication
  async fn get_s3_access_key(
    &self,
    access_key_id: &str,
  ) -> Result<Option<(String, Option<Uuid>)>, anyhow::Error>;

  /// Create a new S3 access key
  async fn create_s3_access_key(
    &self,
    access_key_id: &str,
    secret_key: &str,
    owner_id: Option<Uuid>,
    name: &str,
  ) -> Result<(), anyhow::Error>;

  /// Delete an S3 access key
  async fn delete_s3_access_key(&self, access_key_id: &str) -> Result<bool, anyhow::Error>;

  /// List all S3 access keys
  async fn list_s3_access_keys(&self) -> Result<Vec<S3AccessKeyInfo>, anyhow::Error>;

  // S3 Bucket methods
  /// Get a bucket by name
  async fn get_s3_bucket(&self, name: &str) -> Result<Option<S3Bucket>, anyhow::Error>;

  /// Create a new bucket
  async fn create_s3_bucket(&self, name: &str, owner_id: Option<Uuid>)
    -> Result<(), anyhow::Error>;

  /// Delete a bucket
  async fn delete_s3_bucket(&self, name: &str) -> Result<(), anyhow::Error>;

  /// List all buckets
  async fn list_s3_buckets(&self) -> Result<Vec<S3Bucket>, anyhow::Error>;

  /// Update bucket stats (size and object count)
  async fn update_s3_bucket_stats(
    &self,
    bucket: &str,
    size_delta: i64,
    count_delta: i64,
  ) -> Result<(), anyhow::Error>;

  // S3 Object methods
  /// Get an object by bucket, key, and optional version
  async fn get_s3_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Option<Uuid>,
  ) -> Result<Option<S3Object>, anyhow::Error>;

  /// Create a new object
  async fn create_s3_object(
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
  async fn delete_s3_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Option<Uuid>,
  ) -> Result<(), anyhow::Error>;

  /// Create a delete marker (for versioned buckets)
  async fn create_s3_delete_marker(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
  ) -> Result<(), anyhow::Error>;

  /// Mark all versions of an object as not latest
  async fn unset_s3_object_latest(&self, bucket: &str, key: &str) -> Result<(), anyhow::Error>;

  /// Update object ACL
  async fn update_s3_object_acl(
    &self,
    bucket: &str,
    key: &str,
    acl: ObjectAcl,
  ) -> Result<(), anyhow::Error>;

  /// List objects in a bucket
  async fn list_s3_objects(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    delimiter: Option<&str>,
    max_keys: i32,
    continuation_token: Option<&str>,
  ) -> Result<(Vec<S3Object>, bool, Option<String>), anyhow::Error>;

  /// List common prefixes (for delimiter-based listing)
  async fn list_s3_common_prefixes(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    delimiter: Option<&str>,
  ) -> Result<Vec<String>, anyhow::Error>;

  /// List object versions
  async fn list_s3_object_versions(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    max_keys: i32,
  ) -> Result<(Vec<S3Object>, bool, Option<String>), anyhow::Error>;

  // S3 Multipart Upload methods
  /// Get a multipart upload by ID
  async fn get_s3_multipart_upload(
    &self,
    upload_id: Uuid,
  ) -> Result<Option<S3MultipartUpload>, anyhow::Error>;

  /// Create a new multipart upload
  async fn create_s3_multipart_upload(
    &self,
    upload_id: Uuid,
    bucket: &str,
    key: &str,
    content_type: Option<&str>,
    metadata: serde_json::Value,
  ) -> Result<(), anyhow::Error>;

  /// Delete a multipart upload and its parts
  async fn delete_s3_multipart_upload(&self, upload_id: Uuid) -> Result<(), anyhow::Error>;

  /// List multipart uploads for a bucket
  async fn list_s3_multipart_uploads(
    &self,
    bucket: &str,
    max_uploads: i32,
  ) -> Result<(Vec<S3MultipartUpload>, bool), anyhow::Error>;

  /// Get a multipart part
  async fn get_s3_multipart_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
  ) -> Result<Option<S3Part>, anyhow::Error>;

  /// Create or update a multipart part
  async fn upsert_s3_multipart_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
    etag: &str,
    size: i64,
    storage_path: &str,
  ) -> Result<(), anyhow::Error>;

  /// List parts for a multipart upload
  async fn list_s3_multipart_parts(
    &self,
    upload_id: Uuid,
    max_parts: i32,
  ) -> Result<(Vec<S3Part>, bool), anyhow::Error>;

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
}

/// S3 access key metadata (without the actual secret)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3AccessKeyInfo {
  pub access_key_id: String,
  pub owner_id: Option<Uuid>,
  pub name: String,
  pub created_at: DateTime<Utc>,
}
