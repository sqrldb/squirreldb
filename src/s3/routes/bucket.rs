use axum::{
  extract::{Path, Query, State},
  http::StatusCode,
  response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::s3::error::S3Error;
use crate::s3::server::S3State;
use crate::s3::types::*;
use crate::s3::xml;

/// GET / - List all buckets
pub async fn list_buckets(State(state): State<Arc<S3State>>) -> Result<Response, S3Error> {
  let buckets = state.backend.list_s3_buckets().await?;

  let response = ListBucketsResponse {
    buckets: buckets
      .into_iter()
      .map(|b| BucketInfo {
        name: b.name,
        creation_date: b.created_at,
      })
      .collect(),
    owner: None,
  };

  let body = xml::list_buckets_xml(&response);
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// PUT /{bucket} - Create bucket
pub async fn create_bucket(
  State(state): State<Arc<S3State>>,
  Path(bucket): Path<String>,
) -> Result<Response, S3Error> {
  // Validate bucket name
  validate_bucket_name(&bucket)?;

  // Check if bucket already exists
  if state.backend.get_s3_bucket(&bucket).await?.is_some() {
    return Err(S3Error::bucket_already_exists(&bucket));
  }

  // Create bucket in database
  state.backend.create_s3_bucket(&bucket, None).await?;

  // Initialize storage directory
  state.storage.init_bucket(&bucket).await?;

  Ok((StatusCode::OK, [("Location", format!("/{}", bucket))]).into_response())
}

/// DELETE /{bucket} - Delete bucket
pub async fn delete_bucket(
  State(state): State<Arc<S3State>>,
  Path(bucket): Path<String>,
) -> Result<Response, S3Error> {
  // Check if bucket exists
  let b = state
    .backend
    .get_s3_bucket(&bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(&bucket))?;

  // Check if bucket is empty
  if b.object_count > 0 {
    return Err(S3Error::bucket_not_empty(&bucket));
  }

  // Delete from database
  state.backend.delete_s3_bucket(&bucket).await?;

  // Delete storage directory
  state.storage.delete_bucket(&bucket).await?;

  Ok(StatusCode::NO_CONTENT.into_response())
}

/// HEAD /{bucket} - Check if bucket exists
pub async fn head_bucket(
  State(state): State<Arc<S3State>>,
  Path(bucket): Path<String>,
) -> Result<Response, S3Error> {
  // Check if bucket exists
  state
    .backend
    .get_s3_bucket(&bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(&bucket))?;

  Ok(
    (
      StatusCode::OK,
      [("x-amz-bucket-region", state.config.region.clone())],
    )
      .into_response(),
  )
}

/// GET /{bucket} - List objects or bucket operation
pub async fn list_objects_or_operation(
  State(state): State<Arc<S3State>>,
  Path(bucket): Path<String>,
  Query(params): Query<HashMap<String, String>>,
) -> Result<Response, S3Error> {
  // Check for special operations
  if params.contains_key("versioning") {
    return get_bucket_versioning(state, &bucket).await;
  }
  if params.contains_key("acl") {
    return get_bucket_acl(state, &bucket).await;
  }
  if params.contains_key("lifecycle") {
    return get_bucket_lifecycle(state, &bucket).await;
  }
  if params.contains_key("uploads") {
    return list_multipart_uploads(state, &bucket, params).await;
  }
  if params.contains_key("versions") {
    return list_object_versions(state, &bucket, params).await;
  }

  // Default: list objects
  list_objects_v2(state, &bucket, params).await
}

/// GET /{bucket}?list-type=2 - List objects V2
async fn list_objects_v2(
  state: Arc<S3State>,
  bucket: &str,
  params: HashMap<String, String>,
) -> Result<Response, S3Error> {
  // Check bucket exists
  state
    .backend
    .get_s3_bucket(bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(bucket))?;

  let prefix = params.get("prefix").cloned();
  let delimiter = params.get("delimiter").cloned();
  let max_keys = params
    .get("max-keys")
    .and_then(|s| s.parse().ok())
    .unwrap_or(1000);
  let continuation_token = params.get("continuation-token").cloned();

  let (objects, is_truncated, next_token) = state
    .backend
    .list_s3_objects(
      bucket,
      prefix.as_deref(),
      delimiter.as_deref(),
      max_keys,
      continuation_token.as_deref(),
    )
    .await?;

  // Build common prefixes if delimiter is set
  let common_prefixes = if delimiter.is_some() {
    state
      .backend
      .list_s3_common_prefixes(bucket, prefix.as_deref(), delimiter.as_deref())
      .await?
  } else {
    vec![]
  };

  let response = ListObjectsResponse {
    name: bucket.to_string(),
    prefix,
    delimiter,
    max_keys,
    is_truncated,
    contents: objects
      .into_iter()
      .map(|o| ObjectInfo {
        key: o.key,
        last_modified: o.created_at,
        etag: o.etag,
        size: o.size,
        storage_class: "STANDARD".to_string(),
        owner: None,
      })
      .collect(),
    common_prefixes: common_prefixes
      .into_iter()
      .map(|p| CommonPrefix { prefix: p })
      .collect(),
    continuation_token,
    next_continuation_token: next_token,
    key_count: 0, // Will be set after building contents
    encoding_type: None,
  };

  let body = xml::list_objects_v2_xml(&response);
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// GET /{bucket}?versioning
async fn get_bucket_versioning(state: Arc<S3State>, bucket: &str) -> Result<Response, S3Error> {
  let b = state
    .backend
    .get_s3_bucket(bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(bucket))?;

  let body = xml::versioning_config_xml(b.versioning_enabled);
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// GET /{bucket}?acl
async fn get_bucket_acl(state: Arc<S3State>, bucket: &str) -> Result<Response, S3Error> {
  let b = state
    .backend
    .get_s3_bucket(bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(bucket))?;

  let owner_id = b
    .owner_id
    .map(|u| u.to_string())
    .unwrap_or_else(|| "anonymous".to_string());
  let body = xml::acl_xml(&owner_id, None, &b.acl.grants);
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// GET /{bucket}?lifecycle
async fn get_bucket_lifecycle(state: Arc<S3State>, bucket: &str) -> Result<Response, S3Error> {
  let b = state
    .backend
    .get_s3_bucket(bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(bucket))?;

  if b.lifecycle_rules.is_empty() {
    return Err(S3Error::new(
      crate::s3::error::S3ErrorCode::NoSuchLifecycleConfiguration,
      "The lifecycle configuration does not exist",
    ));
  }

  let body = xml::lifecycle_config_xml(&b.lifecycle_rules);
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// GET /{bucket}?uploads - List multipart uploads
async fn list_multipart_uploads(
  state: Arc<S3State>,
  bucket: &str,
  params: HashMap<String, String>,
) -> Result<Response, S3Error> {
  // Check bucket exists
  state
    .backend
    .get_s3_bucket(bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(bucket))?;

  let max_uploads = params
    .get("max-uploads")
    .and_then(|s| s.parse().ok())
    .unwrap_or(1000);

  let (uploads, is_truncated) = state
    .backend
    .list_s3_multipart_uploads(bucket, max_uploads)
    .await?;

  let body = xml::list_multipart_uploads_xml(bucket, &uploads, max_uploads, is_truncated);
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// GET /{bucket}?versions - List object versions
async fn list_object_versions(
  state: Arc<S3State>,
  bucket: &str,
  params: HashMap<String, String>,
) -> Result<Response, S3Error> {
  // Check bucket exists
  state
    .backend
    .get_s3_bucket(bucket)
    .await?
    .ok_or_else(|| S3Error::no_such_bucket(bucket))?;

  let prefix = params.get("prefix").cloned();
  let max_keys = params
    .get("max-keys")
    .and_then(|s| s.parse().ok())
    .unwrap_or(1000);

  let (objects, is_truncated, _next_token) = state
    .backend
    .list_s3_object_versions(bucket, prefix.as_deref(), max_keys)
    .await?;

  // Build XML response for versions (simplified)
  let response = ListObjectsResponse {
    name: bucket.to_string(),
    prefix,
    delimiter: None,
    max_keys,
    is_truncated,
    contents: objects
      .into_iter()
      .map(|o| ObjectInfo {
        key: o.key,
        last_modified: o.created_at,
        etag: o.etag,
        size: o.size,
        storage_class: "STANDARD".to_string(),
        owner: None,
      })
      .collect(),
    common_prefixes: vec![],
    continuation_token: None,
    next_continuation_token: None,
    key_count: 0,
    encoding_type: None,
  };

  let body = xml::list_objects_v2_xml(&response);
  Ok((StatusCode::OK, [("Content-Type", "application/xml")], body).into_response())
}

/// Validate S3 bucket name
fn validate_bucket_name(name: &str) -> Result<(), S3Error> {
  // Must be 3-63 characters
  if name.len() < 3 || name.len() > 63 {
    return Err(S3Error::invalid_bucket_name(name));
  }

  // Must start with a letter or number
  let first = name.chars().next().unwrap();
  if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
    return Err(S3Error::invalid_bucket_name(name));
  }

  // Must contain only lowercase letters, numbers, hyphens
  for c in name.chars() {
    if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
      return Err(S3Error::invalid_bucket_name(name));
    }
  }

  // Must not end with a hyphen
  if name.ends_with('-') {
    return Err(S3Error::invalid_bucket_name(name));
  }

  // Must not contain consecutive hyphens
  if name.contains("--") {
    return Err(S3Error::invalid_bucket_name(name));
  }

  Ok(())
}
