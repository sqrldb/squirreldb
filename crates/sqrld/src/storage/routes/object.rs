use axum::{
  body::{Body, Bytes},
  extract::{Path, Query, State},
  http::{HeaderMap, StatusCode},
  response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::security::validate_object_key;
use crate::storage::error::StorageError;
use crate::storage::server::StorageState;
use crate::storage::types::*;
use crate::storage::xml;

/// PUT /{bucket}/{key} - Put object or special operation
pub async fn put_object_or_operation(
  State(state): State<Arc<StorageState>>,
  Path((bucket, key)): Path<(String, String)>,
  Query(params): Query<HashMap<String, String>>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<Response, StorageError> {
  // Validate object key to prevent path traversal attacks
  validate_object_key(&key).map_err(|e| {
    StorageError::new(
      crate::storage::error::StorageErrorCode::InvalidArgument,
      format!("Invalid object key: {}", e),
    )
  })?;

  // Check for special operations
  if params.contains_key("acl") {
    return put_object_acl(state, &bucket, &key, body).await;
  }

  // Check for copy operation
  if let Some(copy_source) = headers.get("x-amz-copy-source") {
    if let Ok(source) = copy_source.to_str() {
      return copy_object(state, &bucket, &key, source, &headers).await;
    }
  }

  // Check for multipart upload part
  if params.contains_key("partNumber") && params.contains_key("uploadId") {
    return super::upload_part(state, &bucket, &key, params, body).await;
  }

  // Default: put object
  put_object(state, &bucket, &key, headers, body).await
}

/// PUT /{bucket}/{key} - Put object
async fn put_object(
  state: Arc<StorageState>,
  bucket: &str,
  key: &str,
  headers: HeaderMap,
  body: Bytes,
) -> Result<Response, StorageError> {
  // Check bucket exists
  let bucket_info = state
    .backend
    .get_storage_bucket(bucket)
    .await?
    .ok_or_else(|| StorageError::no_such_bucket(bucket))?;

  // Check object size
  if body.len() as u64 > state.config.max_object_size {
    return Err(StorageError::new(
      crate::storage::error::StorageErrorCode::EntityTooLarge,
      "Object size exceeds maximum allowed size",
    ));
  }

  // Get content type
  let content_type = headers
    .get("content-type")
    .and_then(|v| v.to_str().ok())
    .unwrap_or("application/octet-stream")
    .to_string();

  // Extract user metadata (x-amz-meta-*)
  let mut metadata = serde_json::Map::new();
  for (name, value) in headers.iter() {
    if let Some(meta_key) = name.as_str().strip_prefix("x-amz-meta-") {
      if let Ok(v) = value.to_str() {
        metadata.insert(
          meta_key.to_string(),
          serde_json::Value::String(v.to_string()),
        );
      }
    }
  }

  // Generate version ID
  let version_id = Uuid::new_v4();

  // Write to storage
  let (storage_path, etag, size) = state
    .storage
    .write_object(bucket, key, version_id, &body)
    .await?;

  // Use atomic operations to reduce round-trips
  if bucket_info.versioning_enabled {
    // Versioning: just create new version with stats update (1 query instead of 2)
    state
      .backend
      .create_storage_object_with_stats(
        bucket,
        key,
        version_id,
        &etag,
        size,
        &content_type,
        &storage_path,
        serde_json::Value::Object(metadata),
      )
      .await?;
  } else {
    // No versioning: replace object atomically (1 query instead of 4)
    // Returns old storage path for file cleanup
    if let Some(old_path) = state
      .backend
      .replace_storage_object(
        bucket,
        key,
        version_id,
        &etag,
        size,
        &content_type,
        &storage_path,
        serde_json::Value::Object(metadata),
      )
      .await?
    {
      // Delete old storage file (fire and forget)
      let _ = state.storage.delete_object(&old_path).await;
    }
  }

  Ok(
    (
      StatusCode::OK,
      [
        ("ETag", format!("\"{}\"", etag)),
        ("x-amz-version-id", version_id.to_string()),
      ],
    )
      .into_response(),
  )
}

/// Copy object
async fn copy_object(
  state: Arc<StorageState>,
  dst_bucket: &str,
  dst_key: &str,
  copy_source: &str,
  _headers: &HeaderMap,
) -> Result<Response, StorageError> {
  let source = CopySource::parse(copy_source)
    .ok_or_else(|| StorageError::invalid_argument("Invalid x-amz-copy-source"))?;

  // Get source object
  let src_version_id = source
    .version_id
    .as_ref()
    .and_then(|v| Uuid::parse_str(v).ok());
  let src_object = state
    .backend
    .get_storage_object(&source.bucket, &source.key, src_version_id)
    .await?
    .ok_or_else(|| StorageError::no_such_key(&source.key))?;

  // Check destination bucket exists
  let dst_bucket_info = state
    .backend
    .get_storage_bucket(dst_bucket)
    .await?
    .ok_or_else(|| StorageError::no_such_bucket(dst_bucket))?;

  // Generate new version ID
  let version_id = Uuid::new_v4();

  // Copy storage file
  let (storage_path, etag, size) = state
    .storage
    .copy_object(&src_object.storage_path, dst_bucket, dst_key, version_id)
    .await?;

  // Use atomic operations to reduce round-trips
  if dst_bucket_info.versioning_enabled {
    // Versioning: just create new version with stats update (1 query instead of 2)
    state
      .backend
      .create_storage_object_with_stats(
        dst_bucket,
        dst_key,
        version_id,
        &etag,
        size,
        &src_object.content_type,
        &storage_path,
        src_object.metadata.clone(),
      )
      .await?;
  } else {
    // No versioning: replace object atomically (1 query instead of 4)
    if let Some(old_path) = state
      .backend
      .replace_storage_object(
        dst_bucket,
        dst_key,
        version_id,
        &etag,
        size,
        &src_object.content_type,
        &storage_path,
        src_object.metadata.clone(),
      )
      .await?
    {
      let _ = state.storage.delete_object(&old_path).await;
    }
  }

  let body = xml::copy_object_result_xml(&etag, chrono::Utc::now());
  Ok(
    (
      StatusCode::OK,
      [
        ("Content-Type", "application/xml"),
        ("x-amz-version-id", &version_id.to_string()),
      ],
      body,
    )
      .into_response(),
  )
}

/// PUT /{bucket}/{key}?acl
async fn put_object_acl(
  state: Arc<StorageState>,
  bucket: &str,
  key: &str,
  _body: Bytes,
) -> Result<Response, StorageError> {
  // Check object exists
  state
    .backend
    .get_storage_object(bucket, key, None)
    .await?
    .ok_or_else(|| StorageError::no_such_key(key))?;

  // Parse ACL from body (simplified - just accept empty body for now)
  let acl = ObjectAcl::default();

  // Update object ACL
  state
    .backend
    .update_storage_object_acl(bucket, key, acl)
    .await?;

  Ok(StatusCode::OK.into_response())
}

/// GET /{bucket}/{key} - Get object or special operation
pub async fn get_object_or_operation(
  State(state): State<Arc<StorageState>>,
  Path((bucket, key)): Path<(String, String)>,
  Query(params): Query<HashMap<String, String>>,
  headers: HeaderMap,
) -> Result<Response, StorageError> {
  // Validate object key to prevent path traversal attacks
  validate_object_key(&key).map_err(|e| {
    StorageError::new(
      crate::storage::error::StorageErrorCode::InvalidArgument,
      format!("Invalid object key: {}", e),
    )
  })?;

  // Check for special operations
  if params.contains_key("acl") {
    return get_object_acl(state, &bucket, &key).await;
  }
  if params.contains_key("uploadId") {
    return super::list_parts(state, &bucket, &key, params).await;
  }

  // Default: get object
  get_object(state, &bucket, &key, params, headers).await
}

/// GET /{bucket}/{key}
async fn get_object(
  state: Arc<StorageState>,
  bucket: &str,
  key: &str,
  params: HashMap<String, String>,
  headers: HeaderMap,
) -> Result<Response, StorageError> {
  // Check bucket exists
  state
    .backend
    .get_storage_bucket(bucket)
    .await?
    .ok_or_else(|| StorageError::no_such_bucket(bucket))?;

  // Get version ID if specified
  let version_id = params
    .get("versionId")
    .and_then(|v| Uuid::parse_str(v).ok());

  // Get object metadata
  let object = state
    .backend
    .get_storage_object(bucket, key, version_id)
    .await?
    .ok_or_else(|| StorageError::no_such_key(key))?;

  // Handle delete markers
  if object.is_delete_marker {
    return Err(StorageError::no_such_key(key));
  }

  // Parse range header if present
  let range = headers
    .get("range")
    .and_then(|v| v.to_str().ok())
    .and_then(parse_range);

  // Read object data
  let data = if let Some((start, end)) = range {
    state
      .storage
      .read_object_range(&object.storage_path, start, end)
      .await?
  } else {
    state.storage.read_object(&object.storage_path).await?
  };

  let status = if range.is_some() {
    StatusCode::PARTIAL_CONTENT
  } else {
    StatusCode::OK
  };

  // Build response with headers
  let mut response = Response::builder()
    .status(status)
    .header("Content-Type", object.content_type.clone())
    .header("ETag", format!("\"{}\"", object.etag))
    .header("Content-Length", data.len().to_string())
    .header("x-amz-version-id", object.version_id.to_string())
    .header("Last-Modified", object.created_at.to_rfc2822());

  // Add user metadata
  if let serde_json::Value::Object(meta) = &object.metadata {
    for (k, v) in meta {
      if let serde_json::Value::String(s) = v {
        response = response.header(format!("x-amz-meta-{}", k), s);
      }
    }
  }

  Ok(response.body(Body::from(data)).unwrap())
}

/// GET /{bucket}/{key}?acl
async fn get_object_acl(
  state: Arc<StorageState>,
  bucket: &str,
  key: &str,
) -> Result<Response, StorageError> {
  let object = state
    .backend
    .get_storage_object(bucket, key, None)
    .await?
    .ok_or_else(|| StorageError::no_such_key(key))?;

  let owner_id = "anonymous";
  let body = xml::acl_xml(owner_id, None, &object.acl.grants);
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// HEAD /{bucket}/{key}
pub async fn head_object(
  State(state): State<Arc<StorageState>>,
  Path((bucket, key)): Path<(String, String)>,
  Query(params): Query<HashMap<String, String>>,
) -> Result<Response, StorageError> {
  // Validate object key to prevent path traversal attacks
  validate_object_key(&key).map_err(|e| {
    StorageError::new(
      crate::storage::error::StorageErrorCode::InvalidArgument,
      format!("Invalid object key: {}", e),
    )
  })?;

  // Check bucket exists
  state
    .backend
    .get_storage_bucket(&bucket)
    .await?
    .ok_or_else(|| StorageError::no_such_bucket(&bucket))?;

  // Get version ID if specified
  let version_id = params
    .get("versionId")
    .and_then(|v| Uuid::parse_str(v).ok());

  // Get object metadata
  let object = state
    .backend
    .get_storage_object(&bucket, &key, version_id)
    .await?
    .ok_or_else(|| StorageError::no_such_key(&key))?;

  if object.is_delete_marker {
    return Err(StorageError::no_such_key(&key));
  }

  Ok(
    (
      StatusCode::OK,
      [
        ("Content-Type", object.content_type.as_str()),
        ("Content-Length", &object.size.to_string()),
        ("ETag", &format!("\"{}\"", object.etag)),
        ("x-amz-version-id", &object.version_id.to_string()),
      ],
    )
      .into_response(),
  )
}

/// DELETE /{bucket}/{key}
pub async fn delete_object_or_operation(
  State(state): State<Arc<StorageState>>,
  Path((bucket, key)): Path<(String, String)>,
  Query(params): Query<HashMap<String, String>>,
) -> Result<Response, StorageError> {
  // Validate object key to prevent path traversal attacks
  validate_object_key(&key).map_err(|e| {
    StorageError::new(
      crate::storage::error::StorageErrorCode::InvalidArgument,
      format!("Invalid object key: {}", e),
    )
  })?;

  // Check for multipart abort
  if params.contains_key("uploadId") {
    return super::abort_multipart_upload(state, &bucket, &key, params).await;
  }

  // Default: delete object
  delete_object(state, &bucket, &key, params).await
}

/// DELETE /{bucket}/{key}
async fn delete_object(
  state: Arc<StorageState>,
  bucket: &str,
  key: &str,
  params: HashMap<String, String>,
) -> Result<Response, StorageError> {
  // Check bucket exists
  let bucket_info = state
    .backend
    .get_storage_bucket(bucket)
    .await?
    .ok_or_else(|| StorageError::no_such_bucket(bucket))?;

  // Get version ID if specified
  let version_id = params
    .get("versionId")
    .and_then(|v| Uuid::parse_str(v).ok());

  if bucket_info.versioning_enabled {
    if let Some(vid) = version_id {
      // Delete specific version - atomic operation (1 query instead of 3)
      if let Some((storage_path, _size)) = state
        .backend
        .delete_storage_object_with_stats(bucket, key, Some(vid))
        .await?
      {
        // Delete storage file
        state.storage.delete_object(&storage_path).await?;
      }
    } else {
      // Create delete marker (atomic: unset latest + insert in 1 query)
      let marker_version_id = Uuid::new_v4();
      state
        .backend
        .create_storage_delete_marker(bucket, key, marker_version_id)
        .await?;
      return Ok(
        (
          StatusCode::NO_CONTENT,
          [
            ("x-amz-delete-marker", "true"),
            ("x-amz-version-id", &marker_version_id.to_string()),
          ],
        )
          .into_response(),
      );
    }
  } else {
    // Without versioning, delete atomically (1 query instead of 3)
    if let Some((storage_path, _size)) = state
      .backend
      .delete_storage_object_with_stats(bucket, key, None)
      .await?
    {
      state.storage.delete_object(&storage_path).await?;
    }
  }

  Ok(StatusCode::NO_CONTENT.into_response())
}

/// POST /{bucket}/{key} - Post object operation
pub async fn post_object_operation(
  State(state): State<Arc<StorageState>>,
  Path((bucket, key)): Path<(String, String)>,
  Query(params): Query<HashMap<String, String>>,
  body: Bytes,
) -> Result<Response, StorageError> {
  // Validate object key to prevent path traversal attacks
  validate_object_key(&key).map_err(|e| {
    StorageError::new(
      crate::storage::error::StorageErrorCode::InvalidArgument,
      format!("Invalid object key: {}", e),
    )
  })?;

  // Check for multipart operations
  if params.contains_key("uploads") {
    return super::initiate_multipart_upload(state, &bucket, &key).await;
  }
  if params.contains_key("uploadId") {
    return super::complete_multipart_upload(state, &bucket, &key, params, body).await;
  }

  Err(StorageError::new(
    crate::storage::error::StorageErrorCode::NotImplemented,
    "Operation not supported",
  ))
}

/// Parse Range header
fn parse_range(header: &str) -> Option<(u64, Option<u64>)> {
  let header = header.strip_prefix("bytes=")?;
  let mut parts = header.split('-');
  let start = parts.next()?.parse().ok()?;
  let end = parts
    .next()
    .and_then(|s| if s.is_empty() { None } else { s.parse().ok() });
  Some((start, end))
}
