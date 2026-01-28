use serde::{Deserialize, Serialize};

/// S3 feature configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
  /// Port for the S3 HTTP server
  pub port: u16,

  /// Path for storing object data
  pub storage_path: String,

  /// Maximum object size in bytes
  pub max_object_size: u64,

  /// Maximum part size for multipart uploads
  pub max_part_size: u64,

  /// Minimum part size for multipart uploads
  pub min_part_size: u64,

  /// Default region
  pub region: String,
}

impl Default for S3Config {
  fn default() -> Self {
    Self {
      port: 9000,
      storage_path: "./data/s3".into(),
      max_object_size: 5 * 1024 * 1024 * 1024,
      max_part_size: 5 * 1024 * 1024 * 1024,
      min_part_size: 5 * 1024 * 1024,
      region: "us-east-1".into(),
    }
  }
}

impl From<&crate::server::S3Section> for S3Config {
  fn from(section: &crate::server::S3Section) -> Self {
    Self {
      port: section.port,
      storage_path: section.storage_path.clone(),
      max_object_size: section.max_object_size,
      max_part_size: section.max_part_size,
      min_part_size: section.min_part_size,
      region: section.region.clone(),
    }
  }
}
