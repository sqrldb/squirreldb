use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Storage bucket metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageBucket {
  pub name: String,
  pub owner_id: Option<Uuid>,
  pub versioning_enabled: bool,
  pub acl: BucketAcl,
  pub lifecycle_rules: Vec<LifecycleRule>,
  pub quota_bytes: Option<i64>,
  pub current_size: i64,
  pub object_count: i64,
  pub created_at: DateTime<Utc>,
}

/// Storage object metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageObject {
  pub bucket: String,
  pub key: String,
  pub version_id: Uuid,
  pub is_latest: bool,
  pub etag: String,
  pub size: i64,
  pub content_type: String,
  pub storage_path: String,
  pub metadata: serde_json::Value,
  pub acl: ObjectAcl,
  pub is_delete_marker: bool,
  pub created_at: DateTime<Utc>,
}

/// Multipart upload session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartUpload {
  pub upload_id: Uuid,
  pub bucket: String,
  pub key: String,
  pub content_type: Option<String>,
  pub metadata: serde_json::Value,
  pub initiated_at: DateTime<Utc>,
}

/// Multipart upload part
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartPart {
  pub upload_id: Uuid,
  pub part_number: i32,
  pub etag: String,
  pub size: i64,
  pub storage_path: String,
  pub created_at: DateTime<Utc>,
}

/// Storage access key for authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAccessKey {
  pub access_key_id: String,
  pub secret_access_key_hash: String,
  pub owner_id: Option<Uuid>,
  pub name: String,
  pub permissions: AccessKeyPermissions,
  pub created_at: DateTime<Utc>,
}

/// Access key permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessKeyPermissions {
  /// Bucket access: "*" for all, or list of bucket names
  #[serde(default = "default_star")]
  pub buckets: String,
  /// Action access: "*" for all, or comma-separated list of actions
  #[serde(default = "default_star")]
  pub actions: String,
}

impl Default for AccessKeyPermissions {
  fn default() -> Self {
    Self {
      buckets: "*".into(),
      actions: "*".into(),
    }
  }
}

fn default_star() -> String {
  "*".into()
}

/// Bucket ACL configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BucketAcl {
  #[serde(default)]
  pub grants: Vec<AclGrant>,
}

/// Object ACL configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObjectAcl {
  #[serde(default)]
  pub grants: Vec<AclGrant>,
}

/// ACL grant entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclGrant {
  pub grantee: Grantee,
  pub permission: Permission,
}

/// ACL grantee
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Grantee {
  CanonicalUser {
    id: String,
    display_name: Option<String>,
  },
  Group {
    uri: String,
  },
}

/// ACL permission
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
  FullControl,
  Write,
  WriteAcp,
  Read,
  ReadAcp,
}

/// Lifecycle rule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRule {
  pub id: String,
  pub enabled: bool,
  pub prefix: Option<String>,
  pub expiration_days: Option<i32>,
  pub noncurrent_version_expiration_days: Option<i32>,
}

/// Response for list buckets operation
#[derive(Debug, Clone, Serialize)]
pub struct ListBucketsResponse {
  pub buckets: Vec<BucketInfo>,
  pub owner: Option<Owner>,
}

/// Bucket info for listing
#[derive(Debug, Clone, Serialize)]
pub struct BucketInfo {
  pub name: String,
  pub creation_date: DateTime<Utc>,
}

/// Owner information
#[derive(Debug, Clone, Serialize)]
pub struct Owner {
  pub id: String,
  pub display_name: Option<String>,
}

/// Response for list objects operation
#[derive(Debug, Clone, Serialize)]
pub struct ListObjectsResponse {
  pub name: String,
  pub prefix: Option<String>,
  pub delimiter: Option<String>,
  pub max_keys: i32,
  pub is_truncated: bool,
  pub contents: Vec<ObjectInfo>,
  pub common_prefixes: Vec<CommonPrefix>,
  pub continuation_token: Option<String>,
  pub next_continuation_token: Option<String>,
  pub key_count: i32,
  pub encoding_type: Option<String>,
}

/// Object info for listing
#[derive(Debug, Clone, Serialize)]
pub struct ObjectInfo {
  pub key: String,
  pub last_modified: DateTime<Utc>,
  pub etag: String,
  pub size: i64,
  pub storage_class: String,
  pub owner: Option<Owner>,
}

/// Common prefix for listing with delimiter
#[derive(Debug, Clone, Serialize)]
pub struct CommonPrefix {
  pub prefix: String,
}

/// Response for initiate multipart upload
#[derive(Debug, Clone, Serialize)]
pub struct InitiateMultipartUploadResponse {
  pub bucket: String,
  pub key: String,
  pub upload_id: String,
}

/// Part info for complete multipart upload request
#[derive(Debug, Clone, Deserialize)]
pub struct CompletedPart {
  #[serde(rename = "PartNumber")]
  pub part_number: i32,
  #[serde(rename = "ETag")]
  pub etag: String,
}

/// Request body for complete multipart upload
#[derive(Debug, Clone, Deserialize)]
#[serde(rename = "CompleteMultipartUpload")]
pub struct CompleteMultipartUploadRequest {
  #[serde(rename = "Part")]
  pub parts: Vec<CompletedPart>,
}

/// Response for complete multipart upload
#[derive(Debug, Clone, Serialize)]
pub struct CompleteMultipartUploadResponse {
  pub location: String,
  pub bucket: String,
  pub key: String,
  pub etag: String,
}

/// Copy source parsed from x-amz-copy-source header
#[derive(Debug, Clone)]
pub struct CopySource {
  pub bucket: String,
  pub key: String,
  pub version_id: Option<String>,
}

impl CopySource {
  pub fn parse(header: &str) -> Option<Self> {
    let decoded = urlencoding::decode(header).ok()?;
    let path = decoded.trim_start_matches('/');
    let (bucket_key, version_id) = if let Some((bk, vid)) = path.split_once("?versionId=") {
      (bk, Some(vid.to_string()))
    } else {
      (path, None)
    };

    let (bucket, key) = bucket_key.split_once('/')?;
    Some(Self {
      bucket: bucket.to_string(),
      key: key.to_string(),
      version_id,
    })
  }
}

/// Versioning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersioningConfiguration {
  pub status: VersioningStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersioningStatus {
  Enabled,
  Suspended,
}

/// Delete objects request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename = "Delete")]
pub struct DeleteObjectsRequest {
  #[serde(rename = "Object")]
  pub objects: Vec<ObjectIdentifier>,
  #[serde(rename = "Quiet", default)]
  pub quiet: bool,
}

/// Object identifier for delete
#[derive(Debug, Clone, Deserialize)]
pub struct ObjectIdentifier {
  #[serde(rename = "Key")]
  pub key: String,
  #[serde(rename = "VersionId")]
  pub version_id: Option<String>,
}

/// Delete result for bulk delete
#[derive(Debug, Clone, Serialize)]
pub struct DeleteResult {
  pub deleted: Vec<DeletedObject>,
  pub errors: Vec<DeleteError>,
}

/// Successfully deleted object
#[derive(Debug, Clone, Serialize)]
pub struct DeletedObject {
  pub key: String,
  pub version_id: Option<String>,
  pub delete_marker: Option<bool>,
  pub delete_marker_version_id: Option<String>,
}

/// Delete error
#[derive(Debug, Clone, Serialize)]
pub struct DeleteError {
  pub key: String,
  pub version_id: Option<String>,
  pub code: String,
  pub message: String,
}
