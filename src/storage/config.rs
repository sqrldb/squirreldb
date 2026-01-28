use serde::{Deserialize, Serialize};

/// Storage feature configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
  /// Port for the storage HTTP server
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

impl Default for StorageConfig {
  fn default() -> Self {
    Self {
      port: 9000,
      storage_path: "./data/storage".into(),
      max_object_size: 5 * 1024 * 1024 * 1024,
      max_part_size: 5 * 1024 * 1024 * 1024,
      min_part_size: 5 * 1024 * 1024,
      region: "us-east-1".into(),
    }
  }
}

impl From<&crate::server::StorageSection> for StorageConfig {
  fn from(section: &crate::server::StorageSection) -> Self {
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
