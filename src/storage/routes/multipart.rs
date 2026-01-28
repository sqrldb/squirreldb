use axum::{
  body::Bytes,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::storage::error::StorageError;
use crate::storage::server::StorageState;
use crate::storage::types::*;
use crate::storage::xml;

/// POST /{bucket}/{key}?uploads - Initiate multipart upload
pub async fn initiate_multipart_upload(
  state: Arc<StorageState>,
  bucket: &str,
  key: &str,
) -> Result<Response, StorageError> {
  // Check bucket exists
  state
    .backend
    .get_storage_bucket(bucket)
    .await?
    .ok_or_else(|| StorageError::no_such_bucket(bucket))?;

  // Create multipart upload record
  let upload_id = Uuid::new_v4();
  state
    .backend
    .create_multipart_upload(
      upload_id,
      bucket,
      key,
      None,
      serde_json::Value::Object(serde_json::Map::new()),
    )
    .await?;

  let response = InitiateMultipartUploadResponse {
    bucket: bucket.to_string(),
    key: key.to_string(),
    upload_id: upload_id.to_string(),
  };

  let body = xml::initiate_multipart_upload_xml(&response);
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// PUT /{bucket}/{key}?partNumber=N&uploadId=X - Upload part
pub async fn upload_part(
  state: Arc<StorageState>,
  _bucket: &str,
  _key: &str,
  params: HashMap<String, String>,
  body: Bytes,
) -> Result<Response, StorageError> {
  let upload_id = params
    .get("uploadId")
    .and_then(|s| Uuid::parse_str(s).ok())
    .ok_or_else(|| StorageError::invalid_argument("Missing or invalid uploadId"))?;

  let part_number: i32 = params
    .get("partNumber")
    .and_then(|s| s.parse().ok())
    .ok_or_else(|| StorageError::invalid_argument("Missing or invalid partNumber"))?;

  // Validate part number
  if !(1..=10000).contains(&part_number) {
    return Err(StorageError::invalid_argument(
      "Part number must be between 1 and 10000",
    ));
  }

  // Check upload exists
  state
    .backend
    .get_multipart_upload(upload_id)
    .await?
    .ok_or_else(|| StorageError::no_such_upload(upload_id.to_string()))?;

  // Check part size
  if body.len() as u64 > state.config.max_part_size {
    return Err(StorageError::new(
      crate::storage::error::StorageErrorCode::EntityTooLarge,
      "Part size exceeds maximum allowed size",
    ));
  }

  // Write part to storage
  let (storage_path, etag, size) = state
    .storage
    .write_part(upload_id, part_number, &body)
    .await?;

  // Create part record (replace if exists)
  state
    .backend
    .upsert_multipart_part(upload_id, part_number, &etag, size, &storage_path)
    .await?;

  Ok((StatusCode::OK, [("ETag", format!("\"{}\"", etag))]).into_response())
}

/// POST /{bucket}/{key}?uploadId=X - Complete multipart upload
pub async fn complete_multipart_upload(
  state: Arc<StorageState>,
  bucket: &str,
  key: &str,
  params: HashMap<String, String>,
  body: Bytes,
) -> Result<Response, StorageError> {
  let upload_id = params
    .get("uploadId")
    .and_then(|s| Uuid::parse_str(s).ok())
    .ok_or_else(|| StorageError::invalid_argument("Missing or invalid uploadId"))?;

  // Check upload exists
  let upload = state
    .backend
    .get_multipart_upload(upload_id)
    .await?
    .ok_or_else(|| StorageError::no_such_upload(upload_id.to_string()))?;

  // Parse request body for part list
  let body_str = String::from_utf8_lossy(&body);
  let completed_parts = parse_complete_multipart_request(&body_str)?;

  // Verify all parts exist and get their storage paths in order
  let mut part_paths = Vec::new();
  for cp in &completed_parts {
    let part = state
      .backend
      .get_multipart_part(upload_id, cp.part_number)
      .await?
      .ok_or_else(|| {
        StorageError::new(
          crate::storage::error::StorageErrorCode::InvalidPart,
          format!("Part {} not found", cp.part_number),
        )
      })?;

    // Verify ETag matches
    let expected_etag = cp.etag.trim_matches('"');
    if part.etag != expected_etag {
      return Err(StorageError::new(
        crate::storage::error::StorageErrorCode::InvalidPart,
        format!("Part {} ETag does not match", cp.part_number),
      ));
    }

    part_paths.push(part.storage_path);
  }

  // Verify parts are in order
  let mut prev_part_number = 0;
  for cp in &completed_parts {
    if cp.part_number <= prev_part_number {
      return Err(StorageError::new(
        crate::storage::error::StorageErrorCode::InvalidPartOrder,
        "Parts must be in ascending order",
      ));
    }
    prev_part_number = cp.part_number;
  }

  // Generate version ID
  let version_id = Uuid::new_v4();

  // Assemble parts into final object
  let (storage_path, etag, size) = state
    .storage
    .assemble_parts(bucket, key, version_id, &part_paths)
    .await?;

  // Check bucket for versioning
  let bucket_info = state
    .backend
    .get_storage_bucket(bucket)
    .await?
    .ok_or_else(|| StorageError::no_such_bucket(bucket))?;

  // Create object record using atomic operations
  let content_type = upload
    .content_type
    .unwrap_or_else(|| "application/octet-stream".to_string());

  if bucket_info.versioning_enabled {
    // Versioning: create with stats atomically (1 query instead of 2)
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
        upload.metadata,
      )
      .await?;
  } else {
    // No versioning: replace atomically (1 query instead of 4)
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
        upload.metadata,
      )
      .await?
    {
      let _ = state.storage.delete_object(&old_path).await;
    }
  }

  // Clean up multipart upload
  state.backend.delete_multipart_upload(upload_id).await?;
  state.storage.cleanup_multipart(upload_id).await?;

  let response = CompleteMultipartUploadResponse {
    location: format!("/{}/{}", bucket, key),
    bucket: bucket.to_string(),
    key: key.to_string(),
    etag,
  };

  let body = xml::complete_multipart_upload_xml(&response);
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

/// DELETE /{bucket}/{key}?uploadId=X - Abort multipart upload
pub async fn abort_multipart_upload(
  state: Arc<StorageState>,
  _bucket: &str,
  _key: &str,
  params: HashMap<String, String>,
) -> Result<Response, StorageError> {
  let upload_id = params
    .get("uploadId")
    .and_then(|s| Uuid::parse_str(s).ok())
    .ok_or_else(|| StorageError::invalid_argument("Missing or invalid uploadId"))?;

  // Check upload exists
  state
    .backend
    .get_multipart_upload(upload_id)
    .await?
    .ok_or_else(|| StorageError::no_such_upload(upload_id.to_string()))?;

  // Delete upload and parts from database
  state.backend.delete_multipart_upload(upload_id).await?;

  // Clean up storage
  state.storage.cleanup_multipart(upload_id).await?;

  Ok(StatusCode::NO_CONTENT.into_response())
}

/// GET /{bucket}/{key}?uploadId=X - List parts
pub async fn list_parts(
  state: Arc<StorageState>,
  bucket: &str,
  key: &str,
  params: HashMap<String, String>,
) -> Result<Response, StorageError> {
  let upload_id = params
    .get("uploadId")
    .and_then(|s| Uuid::parse_str(s).ok())
    .ok_or_else(|| StorageError::invalid_argument("Missing or invalid uploadId"))?;

  let max_parts: i32 = params
    .get("max-parts")
    .and_then(|s| s.parse().ok())
    .unwrap_or(1000);

  // Check upload exists
  state
    .backend
    .get_multipart_upload(upload_id)
    .await?
    .ok_or_else(|| StorageError::no_such_upload(upload_id.to_string()))?;

  // Get parts
  let (parts, is_truncated) = state
    .backend
    .list_multipart_parts(upload_id, max_parts)
    .await?;

  let body = xml::list_parts_xml(
    bucket,
    key,
    &upload_id.to_string(),
    &parts,
    max_parts,
    is_truncated,
  );
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// Parse CompleteMultipartUpload XML request
fn parse_complete_multipart_request(xml: &str) -> Result<Vec<CompletedPart>, StorageError> {
  // Simple XML parsing (production should use a proper XML parser)
  let mut parts = Vec::new();

  // Find all <Part>...</Part> blocks
  let part_regex =
    regex::Regex::new(r"<Part>.*?<PartNumber>(\d+)</PartNumber>.*?<ETag>([^<]+)</ETag>.*?</Part>")
      .map_err(|_| StorageError::internal_error("Regex error"))?;

  for cap in part_regex.captures_iter(xml) {
    let part_number: i32 = cap[1].parse().map_err(|_| {
      StorageError::new(
        crate::storage::error::StorageErrorCode::MalformedXML,
        "Invalid part number",
      )
    })?;
    let etag = cap[2].to_string();

    parts.push(CompletedPart { part_number, etag });
  }

  if parts.is_empty() {
    return Err(StorageError::new(
      crate::storage::error::StorageErrorCode::MalformedXML,
      "No parts specified in request",
    ));
  }

  Ok(parts)
}
