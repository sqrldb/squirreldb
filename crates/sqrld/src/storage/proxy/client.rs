//! S3 proxy client for connecting to external S3 providers

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{Builder as S3ConfigBuilder, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::storage::backend::StorageBackend;
use crate::storage::config::ProxyConfig;
use crate::storage::error::StorageError;
use crate::storage::filesystem::LocalFileStorage;

type PartCache = HashMap<Uuid, HashMap<i32, Vec<u8>>>;

/// S3 proxy client that connects to external S3 providers
pub struct S3ProxyClient {
  client: S3Client,
  config: ProxyConfig,
  /// Cache for part data during multipart uploads (upload_id -> part_number -> data)
  part_cache: Arc<RwLock<PartCache>>,
}

impl S3ProxyClient {
  /// Create a new S3 proxy client from configuration
  pub async fn new(config: ProxyConfig) -> Result<Self, StorageError> {
    let credentials = Credentials::new(
      &config.access_key_id,
      &config.secret_access_key,
      None,
      None,
      "sqrld-proxy",
    );

    let mut s3_config = S3ConfigBuilder::new()
      .behavior_version(BehaviorVersion::latest())
      .region(Region::new(config.region.clone()))
      .credentials_provider(credentials);

    // Set custom endpoint if provided
    if !config.endpoint.is_empty() {
      s3_config = s3_config.endpoint_url(&config.endpoint);
    }

    // Force path style for MinIO and self-hosted S3
    if config.force_path_style {
      s3_config = s3_config.force_path_style(true);
    }

    let client = S3Client::from_conf(s3_config.build());

    Ok(Self {
      client,
      config,
      part_cache: Arc::new(RwLock::new(HashMap::new())),
    })
  }

  /// Get the effective bucket name (with optional prefix)
  fn effective_bucket(&self, bucket: &str) -> String {
    match &self.config.bucket_prefix {
      Some(prefix) if !prefix.is_empty() => format!("{}{}", prefix, bucket),
      _ => bucket.to_string(),
    }
  }

  /// Generate a storage path for proxy objects (virtual path, not filesystem)
  fn storage_path(&self, bucket: &str, key: &str, version_id: Uuid) -> String {
    format!(
      "s3://{}/{}/{}",
      self.effective_bucket(bucket),
      key,
      version_id
    )
  }

  /// Parse a storage path back to bucket and key
  fn parse_storage_path(path: &str) -> Option<(String, String)> {
    if !path.starts_with("s3://") {
      return None;
    }
    let rest = &path[5..];
    let parts: Vec<&str> = rest.splitn(3, '/').collect();
    if parts.len() >= 2 {
      // Remove version_id from key if present
      let key = if parts.len() == 3 {
        // key/version_id format - extract just the key
        parts[1].to_string()
      } else {
        parts[1].to_string()
      };
      Some((parts[0].to_string(), key))
    } else {
      None
    }
  }
}

#[async_trait]
impl StorageBackend for S3ProxyClient {
  async fn init(&self) -> Result<(), StorageError> {
    // Verify connection by listing buckets
    self
      .client
      .list_buckets()
      .send()
      .await
      .map_err(|e| StorageError::internal_error(format!("Failed to connect to S3: {}", e)))?;
    Ok(())
  }

  async fn init_bucket(&self, bucket: &str) -> Result<(), StorageError> {
    let effective_bucket = self.effective_bucket(bucket);

    // Check if bucket exists first
    let exists = self
      .client
      .head_bucket()
      .bucket(&effective_bucket)
      .send()
      .await
      .is_ok();

    if !exists {
      // Create the bucket
      self
        .client
        .create_bucket()
        .bucket(&effective_bucket)
        .send()
        .await
        .map_err(|e| StorageError::internal_error(format!("Failed to create bucket: {}", e)))?;
    }

    Ok(())
  }

  async fn delete_bucket(&self, bucket: &str) -> Result<(), StorageError> {
    let effective_bucket = self.effective_bucket(bucket);

    self
      .client
      .delete_bucket()
      .bucket(&effective_bucket)
      .send()
      .await
      .map_err(|e| StorageError::internal_error(format!("Failed to delete bucket: {}", e)))?;

    Ok(())
  }

  async fn write_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    data: &[u8],
  ) -> Result<(String, String, i64), StorageError> {
    let effective_bucket = self.effective_bucket(bucket);

    let result = self
      .client
      .put_object()
      .bucket(&effective_bucket)
      .key(key)
      .body(ByteStream::from(data.to_vec()))
      .send()
      .await
      .map_err(|e| StorageError::internal_error(format!("Failed to write object: {}", e)))?;

    let etag = result
      .e_tag()
      .map(|s| s.trim_matches('"').to_string())
      .unwrap_or_else(|| LocalFileStorage::calculate_etag(data));

    let storage_path = self.storage_path(bucket, key, version_id);
    let size = data.len() as i64;

    Ok((storage_path, etag, size))
  }

  async fn read_object(&self, path: &str) -> Result<Vec<u8>, StorageError> {
    let (bucket, key) = Self::parse_storage_path(path)
      .ok_or_else(|| StorageError::no_such_key(format!("Invalid storage path: {}", path)))?;

    let result = self
      .client
      .get_object()
      .bucket(&bucket)
      .key(&key)
      .send()
      .await
      .map_err(|e| StorageError::no_such_key(format!("Failed to read object: {}", e)))?;

    let data = result
      .body
      .collect()
      .await
      .map_err(|e| StorageError::internal_error(format!("Failed to read body: {}", e)))?
      .into_bytes()
      .to_vec();

    Ok(data)
  }

  async fn read_object_range(
    &self,
    path: &str,
    start: u64,
    end: Option<u64>,
  ) -> Result<Vec<u8>, StorageError> {
    let (bucket, key) = Self::parse_storage_path(path)
      .ok_or_else(|| StorageError::no_such_key(format!("Invalid storage path: {}", path)))?;

    let range = match end {
      Some(e) => format!("bytes={}-{}", start, e),
      None => format!("bytes={}-", start),
    };

    let result = self
      .client
      .get_object()
      .bucket(&bucket)
      .key(&key)
      .range(range)
      .send()
      .await
      .map_err(|e| StorageError::no_such_key(format!("Failed to read object range: {}", e)))?;

    let data = result
      .body
      .collect()
      .await
      .map_err(|e| StorageError::internal_error(format!("Failed to read body: {}", e)))?
      .into_bytes()
      .to_vec();

    Ok(data)
  }

  async fn delete_object(&self, path: &str) -> Result<(), StorageError> {
    let (bucket, key) = Self::parse_storage_path(path)
      .ok_or_else(|| StorageError::no_such_key(format!("Invalid storage path: {}", path)))?;

    self
      .client
      .delete_object()
      .bucket(&bucket)
      .key(&key)
      .send()
      .await
      .map_err(|e| StorageError::internal_error(format!("Failed to delete object: {}", e)))?;

    Ok(())
  }

  async fn write_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
    data: &[u8],
  ) -> Result<(String, String, i64), StorageError> {
    // Store part data in cache for later assembly
    let mut cache = self.part_cache.write().await;
    let parts = cache.entry(upload_id).or_insert_with(HashMap::new);
    parts.insert(part_number, data.to_vec());

    let etag = LocalFileStorage::calculate_etag(data);
    let storage_path = format!("part://{}:{}", upload_id, part_number);
    let size = data.len() as i64;

    Ok((storage_path, etag, size))
  }

  async fn read_part(&self, path: &str) -> Result<Vec<u8>, StorageError> {
    // Parse part path: part://upload_id:part_number
    if !path.starts_with("part://") {
      return Err(StorageError::internal_error(format!(
        "Invalid part path: {}",
        path
      )));
    }

    let rest = &path[7..];
    let parts: Vec<&str> = rest.split(':').collect();
    if parts.len() != 2 {
      return Err(StorageError::internal_error(format!(
        "Invalid part path format: {}",
        path
      )));
    }

    let upload_id = Uuid::parse_str(parts[0])
      .map_err(|_| StorageError::internal_error("Invalid upload ID in part path"))?;
    let part_number: i32 = parts[1]
      .parse()
      .map_err(|_| StorageError::internal_error("Invalid part number in part path"))?;

    let cache = self.part_cache.read().await;
    cache
      .get(&upload_id)
      .and_then(|parts| parts.get(&part_number))
      .cloned()
      .ok_or_else(|| StorageError::internal_error(format!("Part not found: {}", path)))
  }

  async fn assemble_parts(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    part_paths: &[String],
  ) -> Result<(String, String, i64), StorageError> {
    // Read all parts and concatenate
    let mut final_data = Vec::new();
    let mut part_etags = Vec::new();

    for path in part_paths {
      let data = self.read_part(path).await?;
      part_etags.push(LocalFileStorage::calculate_etag(&data));
      final_data.extend_from_slice(&data);
    }

    // Upload assembled data to S3
    let (storage_path, _, _) = self
      .write_object(bucket, key, version_id, &final_data)
      .await?;

    // Calculate multipart ETag
    let etag = LocalFileStorage::calculate_multipart_etag(&part_etags);
    let size = final_data.len() as i64;

    Ok((storage_path, etag, size))
  }

  async fn cleanup_multipart(&self, upload_id: Uuid) -> Result<(), StorageError> {
    // Remove from cache
    let mut cache = self.part_cache.write().await;
    cache.remove(&upload_id);
    Ok(())
  }

  async fn copy_object(
    &self,
    src_path: &str,
    dst_bucket: &str,
    dst_key: &str,
    dst_version_id: Uuid,
  ) -> Result<(String, String, i64), StorageError> {
    let (src_bucket, src_key) = Self::parse_storage_path(src_path)
      .ok_or_else(|| StorageError::no_such_key(format!("Invalid storage path: {}", src_path)))?;

    let effective_src_bucket = src_bucket;
    let effective_dst_bucket = self.effective_bucket(dst_bucket);

    let copy_source = format!("{}/{}", effective_src_bucket, src_key);

    let result = self
      .client
      .copy_object()
      .bucket(&effective_dst_bucket)
      .key(dst_key)
      .copy_source(&copy_source)
      .send()
      .await
      .map_err(|e| StorageError::internal_error(format!("Failed to copy object: {}", e)))?;

    let etag = result
      .copy_object_result()
      .and_then(|r| r.e_tag())
      .map(|s| s.trim_matches('"').to_string())
      .unwrap_or_default();

    // Get size from head object
    let head = self
      .client
      .head_object()
      .bucket(&effective_dst_bucket)
      .key(dst_key)
      .send()
      .await
      .map_err(|e| StorageError::internal_error(format!("Failed to get object size: {}", e)))?;

    let size = head.content_length().unwrap_or(0);
    let storage_path = self.storage_path(dst_bucket, dst_key, dst_version_id);

    Ok((storage_path, etag, size))
  }

  async fn test_connection(&self) -> Result<(), StorageError> {
    self
      .client
      .list_buckets()
      .send()
      .await
      .map_err(|e| StorageError::internal_error(format!("Connection test failed: {}", e)))?;
    Ok(())
  }

  fn name(&self) -> &'static str {
    "s3-proxy"
  }
}
