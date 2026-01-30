use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use uuid::Uuid;

use super::backend::StorageBackend;
use super::error::StorageError;

/// Local filesystem storage for S3 objects
pub struct LocalFileStorage {
  base_path: PathBuf,
}

impl LocalFileStorage {
  pub fn new(base_path: impl AsRef<Path>) -> Self {
    Self {
      base_path: base_path.as_ref().to_path_buf(),
    }
  }

  /// Initialize storage directory structure
  pub async fn init(&self) -> Result<(), StorageError> {
    fs::create_dir_all(&self.base_path).await?;
    fs::create_dir_all(self.base_path.join("buckets")).await?;
    fs::create_dir_all(self.base_path.join("multipart")).await?;
    Ok(())
  }

  /// Get storage path for an object
  fn object_path(&self, bucket: &str, key: &str, version_id: Uuid) -> PathBuf {
    let key_hash = Self::hash_key(key);
    self
      .base_path
      .join("buckets")
      .join(bucket)
      .join("objects")
      .join(&key_hash[0..2])
      .join(&key_hash[2..4])
      .join(format!("{}.data", version_id))
  }

  /// Get storage path for a multipart part
  fn part_path(&self, upload_id: Uuid, part_number: i32) -> PathBuf {
    self
      .base_path
      .join("multipart")
      .join(upload_id.to_string())
      .join(format!("part_{}", part_number))
  }

  /// Hash key for path sharding
  fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
  }

  /// Write object data and return (storage_path, etag, size)
  pub async fn write_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    data: &[u8],
  ) -> Result<(String, String, i64), StorageError> {
    let path = self.object_path(bucket, key, version_id);

    // Create parent directories
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).await?;
    }

    // Write data
    let mut file = File::create(&path).await?;
    file.write_all(data).await?;
    file.flush().await?;

    // Calculate ETag (MD5 hash)
    let etag = Self::calculate_etag(data);
    let size = data.len() as i64;

    Ok((path.to_string_lossy().into_owned(), etag, size))
  }

  /// Read object data
  pub async fn read_object(&self, storage_path: &str) -> Result<Vec<u8>, StorageError> {
    let path = Path::new(storage_path);
    if !path.exists() {
      return Err(StorageError::no_such_key(storage_path));
    }
    let data = fs::read(path).await?;
    Ok(data)
  }

  /// Read object data with range
  pub async fn read_object_range(
    &self,
    storage_path: &str,
    start: u64,
    end: Option<u64>,
  ) -> Result<Vec<u8>, StorageError> {
    let path = Path::new(storage_path);
    if !path.exists() {
      return Err(StorageError::no_such_key(storage_path));
    }

    let mut file = File::open(path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len();

    let actual_end = end.unwrap_or(file_size - 1).min(file_size - 1);
    let length = (actual_end - start + 1) as usize;

    file.seek(SeekFrom::Start(start)).await?;

    let mut buffer = vec![0u8; length];
    file.read_exact(&mut buffer).await?;

    Ok(buffer)
  }

  /// Delete object data
  pub async fn delete_object(&self, storage_path: &str) -> Result<(), StorageError> {
    let path = Path::new(storage_path);
    if path.exists() {
      fs::remove_file(path).await?;
    }
    Ok(())
  }

  /// Write multipart part
  pub async fn write_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
    data: &[u8],
  ) -> Result<(String, String, i64), StorageError> {
    let path = self.part_path(upload_id, part_number);

    // Create parent directories
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).await?;
    }

    // Write data
    let mut file = File::create(&path).await?;
    file.write_all(data).await?;
    file.flush().await?;

    let etag = Self::calculate_etag(data);
    let size = data.len() as i64;

    Ok((path.to_string_lossy().into_owned(), etag, size))
  }

  /// Read multipart part
  pub async fn read_part(&self, storage_path: &str) -> Result<Vec<u8>, StorageError> {
    let path = Path::new(storage_path);
    if !path.exists() {
      return Err(StorageError::internal_error(format!(
        "Part not found: {}",
        storage_path
      )));
    }
    let data = fs::read(path).await?;
    Ok(data)
  }

  /// Assemble multipart parts into final object
  pub async fn assemble_parts(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    part_paths: &[String],
  ) -> Result<(String, String, i64), StorageError> {
    let final_path = self.object_path(bucket, key, version_id);

    // Create parent directories
    if let Some(parent) = final_path.parent() {
      fs::create_dir_all(parent).await?;
    }

    // Read all parts and concatenate
    let mut final_data = Vec::new();
    let mut part_etags = Vec::new();

    for path in part_paths {
      let data = self.read_part(path).await?;
      part_etags.push(Self::calculate_etag(&data));
      final_data.extend_from_slice(&data);
    }

    // Write final object
    let mut file = File::create(&final_path).await?;
    file.write_all(&final_data).await?;
    file.flush().await?;

    // Calculate multipart ETag
    let etag = Self::calculate_multipart_etag(&part_etags);
    let size = final_data.len() as i64;

    Ok((final_path.to_string_lossy().into_owned(), etag, size))
  }

  /// Clean up multipart upload directory
  pub async fn cleanup_multipart(&self, upload_id: Uuid) -> Result<(), StorageError> {
    let path = self.base_path.join("multipart").join(upload_id.to_string());
    if path.exists() {
      fs::remove_dir_all(path).await?;
    }
    Ok(())
  }

  /// Initialize bucket storage directory
  pub async fn init_bucket(&self, bucket: &str) -> Result<(), StorageError> {
    let bucket_path = self.base_path.join("buckets").join(bucket);
    fs::create_dir_all(bucket_path.join("objects")).await?;
    Ok(())
  }

  /// Delete bucket storage directory
  pub async fn delete_bucket(&self, bucket: &str) -> Result<(), StorageError> {
    let bucket_path = self.base_path.join("buckets").join(bucket);
    if bucket_path.exists() {
      fs::remove_dir_all(bucket_path).await?;
    }
    Ok(())
  }

  /// Calculate MD5 ETag for data
  pub fn calculate_etag(data: &[u8]) -> String {
    let digest = md5::compute(data);
    format!("{:x}", digest)
  }

  /// Calculate multipart ETag (MD5 of MD5s + part count)
  pub fn calculate_multipart_etag(part_etags: &[String]) -> String {
    let mut combined = Vec::new();
    for etag in part_etags {
      if let Ok(bytes) = hex::decode(etag) {
        combined.extend_from_slice(&bytes);
      }
    }
    let digest = md5::compute(&combined);
    format!("{:x}-{}", digest, part_etags.len())
  }

  /// Copy object from one location to another
  pub async fn copy_object(
    &self,
    src_path: &str,
    dst_bucket: &str,
    dst_key: &str,
    dst_version_id: Uuid,
  ) -> Result<(String, String, i64), StorageError> {
    let data = self.read_object(src_path).await?;
    self
      .write_object(dst_bucket, dst_key, dst_version_id, &data)
      .await
  }
}

#[async_trait]
impl StorageBackend for LocalFileStorage {
  async fn init(&self) -> Result<(), StorageError> {
    LocalFileStorage::init(self).await
  }

  async fn init_bucket(&self, bucket: &str) -> Result<(), StorageError> {
    LocalFileStorage::init_bucket(self, bucket).await
  }

  async fn delete_bucket(&self, bucket: &str) -> Result<(), StorageError> {
    LocalFileStorage::delete_bucket(self, bucket).await
  }

  async fn write_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    data: &[u8],
  ) -> Result<(String, String, i64), StorageError> {
    LocalFileStorage::write_object(self, bucket, key, version_id, data).await
  }

  async fn read_object(&self, path: &str) -> Result<Vec<u8>, StorageError> {
    LocalFileStorage::read_object(self, path).await
  }

  async fn read_object_range(
    &self,
    path: &str,
    start: u64,
    end: Option<u64>,
  ) -> Result<Vec<u8>, StorageError> {
    LocalFileStorage::read_object_range(self, path, start, end).await
  }

  async fn delete_object(&self, path: &str) -> Result<(), StorageError> {
    LocalFileStorage::delete_object(self, path).await
  }

  async fn write_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
    data: &[u8],
  ) -> Result<(String, String, i64), StorageError> {
    LocalFileStorage::write_part(self, upload_id, part_number, data).await
  }

  async fn read_part(&self, path: &str) -> Result<Vec<u8>, StorageError> {
    LocalFileStorage::read_part(self, path).await
  }

  async fn assemble_parts(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    part_paths: &[String],
  ) -> Result<(String, String, i64), StorageError> {
    LocalFileStorage::assemble_parts(self, bucket, key, version_id, part_paths).await
  }

  async fn cleanup_multipart(&self, upload_id: Uuid) -> Result<(), StorageError> {
    LocalFileStorage::cleanup_multipart(self, upload_id).await
  }

  async fn copy_object(
    &self,
    src_path: &str,
    dst_bucket: &str,
    dst_key: &str,
    dst_version_id: Uuid,
  ) -> Result<(String, String, i64), StorageError> {
    LocalFileStorage::copy_object(self, src_path, dst_bucket, dst_key, dst_version_id).await
  }

  async fn test_connection(&self) -> Result<(), StorageError> {
    // Local storage is always available if the directory exists
    if self.base_path.exists() {
      Ok(())
    } else {
      Err(StorageError::internal_error("Storage directory does not exist"))
    }
  }

  fn name(&self) -> &'static str {
    "local"
  }
}
