//! Storage backend trait for local and proxy storage

use async_trait::async_trait;
use uuid::Uuid;

use super::error::StorageError;

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// Trait for storage backends (local filesystem or S3 proxy)
#[async_trait]
pub trait StorageBackend: Send + Sync {
  /// Initialize the storage backend
  async fn init(&self) -> StorageResult<()>;

  /// Initialize a bucket's storage
  async fn init_bucket(&self, bucket: &str) -> StorageResult<()>;

  /// Delete a bucket's storage
  async fn delete_bucket(&self, bucket: &str) -> StorageResult<()>;

  /// Write an object and return (storage_path, etag, size)
  async fn write_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    data: &[u8],
  ) -> StorageResult<(String, String, i64)>;

  /// Read an object's data
  async fn read_object(&self, path: &str) -> StorageResult<Vec<u8>>;

  /// Read an object's data with range
  async fn read_object_range(
    &self,
    path: &str,
    start: u64,
    end: Option<u64>,
  ) -> StorageResult<Vec<u8>>;

  /// Delete an object's data
  async fn delete_object(&self, path: &str) -> StorageResult<()>;

  /// Write a multipart part and return (storage_path, etag, size)
  async fn write_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
    data: &[u8],
  ) -> StorageResult<(String, String, i64)>;

  /// Read a multipart part's data
  async fn read_part(&self, path: &str) -> StorageResult<Vec<u8>>;

  /// Assemble parts into final object and return (storage_path, etag, size)
  async fn assemble_parts(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    part_paths: &[String],
  ) -> StorageResult<(String, String, i64)>;

  /// Clean up a multipart upload's temporary files
  async fn cleanup_multipart(&self, upload_id: Uuid) -> StorageResult<()>;

  /// Copy an object and return (storage_path, etag, size)
  async fn copy_object(
    &self,
    src_path: &str,
    dst_bucket: &str,
    dst_key: &str,
    dst_version_id: Uuid,
  ) -> StorageResult<(String, String, i64)>;

  /// Test connection to the backend (for proxy mode)
  async fn test_connection(&self) -> StorageResult<()>;

  /// Get a human-readable name for this backend
  fn name(&self) -> &'static str;
}
