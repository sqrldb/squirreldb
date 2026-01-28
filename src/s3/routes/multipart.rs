use axum::{
  body::Bytes,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::s3::error::S3Error;
use crate::s3::server::S3State;
use crate::s3::types::*;
use crate::s3::xml;

/// POST /{bucket}/{key}?uploads - Initiate multipart upload
pub async fn initiate_multipart_upload(
  state: Arc<S3State>,
  bucket: &str,
  key: &str,
) -> Result<Response, S3Error> {
  // Check bucket exists
  state
    .backend
    .get_s3_bucket(bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(bucket))?;

  // Create multipart upload record
  let upload_id = Uuid::new_v4();
  state
    .backend
    .create_s3_multipart_upload(
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
  state: Arc<S3State>,
  _bucket: &str,
  _key: &str,
  params: HashMap<String, String>,
  body: Bytes,
) -> Result<Response, S3Error> {
  let upload_id = params
    .get("uploadId")
    .and_then(|s| Uuid::parse_str(s).ok())
    .ok_or_else(|| S3Error::invalid_argument("Missing or invalid uploadId"))?;

  let part_number: i32 = params
    .get("partNumber")
    .and_then(|s| s.parse().ok())
    .ok_or_else(|| S3Error::invalid_argument("Missing or invalid partNumber"))?;

  // Validate part number
  if !(1..=10000).contains(&part_number) {
    return Err(S3Error::invalid_argument(
      "Part number must be between 1 and 10000",
    ));
  }

  // Check upload exists
  state
    .backend
    .get_s3_multipart_upload(upload_id)
    .await?
    .ok_or_else(|| S3Error::no_such_upload(upload_id.to_string()))?;

  // Check part size
  if body.len() as u64 > state.config.max_part_size {
    return Err(S3Error::new(
      crate::s3::error::S3ErrorCode::EntityTooLarge,
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
    .upsert_s3_multipart_part(upload_id, part_number, &etag, size, &storage_path)
    .await?;

  Ok((StatusCode::OK, [("ETag", format!("\"{}\"", etag))]).into_response())
}

/// POST /{bucket}/{key}?uploadId=X - Complete multipart upload
pub async fn complete_multipart_upload(
  state: Arc<S3State>,
  bucket: &str,
  key: &str,
  params: HashMap<String, String>,
  body: Bytes,
) -> Result<Response, S3Error> {
  let upload_id = params
    .get("uploadId")
    .and_then(|s| Uuid::parse_str(s).ok())
    .ok_or_else(|| S3Error::invalid_argument("Missing or invalid uploadId"))?;

  // Check upload exists
  let upload = state
    .backend
    .get_s3_multipart_upload(upload_id)
    .await?
    .ok_or_else(|| S3Error::no_such_upload(upload_id.to_string()))?;

  // Parse request body for part list
  let body_str = String::from_utf8_lossy(&body);
  let completed_parts = parse_complete_multipart_request(&body_str)?;

  // Verify all parts exist and get their storage paths in order
  let mut part_paths = Vec::new();
  for cp in &completed_parts {
    let part = state
      .backend
      .get_s3_multipart_part(upload_id, cp.part_number)
      .await?
      .ok_or_else(|| {
        S3Error::new(
          crate::s3::error::S3ErrorCode::InvalidPart,
          format!("Part {} not found", cp.part_number),
        )
      })?;

    // Verify ETag matches
    let expected_etag = cp.etag.trim_matches('"');
    if part.etag != expected_etag {
      return Err(S3Error::new(
        crate::s3::error::S3ErrorCode::InvalidPart,
        format!("Part {} ETag does not match", cp.part_number),
      ));
    }

    part_paths.push(part.storage_path);
  }

  // Verify parts are in order
  let mut prev_part_number = 0;
  for cp in &completed_parts {
    if cp.part_number <= prev_part_number {
      return Err(S3Error::new(
        crate::s3::error::S3ErrorCode::InvalidPartOrder,
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
    .get_s3_bucket(bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(bucket))?;

  // If versioning is not enabled, delete previous version
  if !bucket_info.versioning_enabled {
    if let Some(existing) = state.backend.get_s3_object(bucket, key, None).await? {
      let _ = state.storage.delete_object(&existing.storage_path).await;
      state.backend.unset_s3_object_latest(bucket, key).await?;
    }
  }

  // Create object record
  let content_type = upload
    .content_type
    .unwrap_or_else(|| "application/octet-stream".to_string());
  state
    .backend
    .create_s3_object(
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

  // Update bucket stats
  state
    .backend
    .update_s3_bucket_stats(bucket, size, 1)
    .await?;

  // Clean up multipart upload
  state.backend.delete_s3_multipart_upload(upload_id).await?;
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
  state: Arc<S3State>,
  _bucket: &str,
  _key: &str,
  params: HashMap<String, String>,
) -> Result<Response, S3Error> {
  let upload_id = params
    .get("uploadId")
    .and_then(|s| Uuid::parse_str(s).ok())
    .ok_or_else(|| S3Error::invalid_argument("Missing or invalid uploadId"))?;

  // Check upload exists
  state
    .backend
    .get_s3_multipart_upload(upload_id)
    .await?
    .ok_or_else(|| S3Error::no_such_upload(upload_id.to_string()))?;

  // Delete upload and parts from database
  state.backend.delete_s3_multipart_upload(upload_id).await?;

  // Clean up storage
  state.storage.cleanup_multipart(upload_id).await?;

  Ok(StatusCode::NO_CONTENT.into_response())
}

/// GET /{bucket}/{key}?uploadId=X - List parts
pub async fn list_parts(
  state: Arc<S3State>,
  bucket: &str,
  key: &str,
  params: HashMap<String, String>,
) -> Result<Response, S3Error> {
  let upload_id = params
    .get("uploadId")
    .and_then(|s| Uuid::parse_str(s).ok())
    .ok_or_else(|| S3Error::invalid_argument("Missing or invalid uploadId"))?;

  let max_parts: i32 = params
    .get("max-parts")
    .and_then(|s| s.parse().ok())
    .unwrap_or(1000);

  // Check upload exists
  state
    .backend
    .get_s3_multipart_upload(upload_id)
    .await?
    .ok_or_else(|| S3Error::no_such_upload(upload_id.to_string()))?;

  // Get parts
  let (parts, is_truncated) = state
    .backend
    .list_s3_multipart_parts(upload_id, max_parts)
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
fn parse_complete_multipart_request(xml: &str) -> Result<Vec<CompletedPart>, S3Error> {
  // Simple XML parsing (production should use a proper XML parser)
  let mut parts = Vec::new();

  // Find all <Part>...</Part> blocks
  let part_regex =
    regex::Regex::new(r"<Part>.*?<PartNumber>(\d+)</PartNumber>.*?<ETag>([^<]+)</ETag>.*?</Part>")
      .map_err(|_| S3Error::internal_error("Regex error"))?;

  for cap in part_regex.captures_iter(xml) {
    let part_number: i32 = cap[1].parse().map_err(|_| {
      S3Error::new(
        crate::s3::error::S3ErrorCode::MalformedXML,
        "Invalid part number",
      )
    })?;
    let etag = cap[2].to_string();

    parts.push(CompletedPart { part_number, etag });
  }

  if parts.is_empty() {
    return Err(S3Error::new(
      crate::s3::error::S3ErrorCode::MalformedXML,
      "No parts specified in request",
    ));
  }

  Ok(parts)
}
