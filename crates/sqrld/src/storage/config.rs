use serde::{Deserialize, Serialize};

/// Storage mode: builtin local storage or proxy to external S3
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
  /// Local filesystem storage (default)
  #[default]
  Builtin,
  /// Proxy to external S3 provider
  Proxy,
}

impl std::fmt::Display for StorageMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      StorageMode::Builtin => write!(f, "builtin"),
      StorageMode::Proxy => write!(f, "proxy"),
    }
  }
}

impl std::str::FromStr for StorageMode {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "builtin" | "local" => Ok(StorageMode::Builtin),
      "proxy" | "external" | "s3" => Ok(StorageMode::Proxy),
      _ => Err(format!("Unknown storage mode: {}", s)),
    }
  }
}

/// Configuration for proxy mode (external S3)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxyConfig {
  /// S3 endpoint URL (e.g., `https://s3.amazonaws.com`)
  #[serde(default)]
  pub endpoint: String,

  /// AWS access key ID
  #[serde(default)]
  pub access_key_id: String,

  /// AWS secret access key (should be encrypted at rest)
  #[serde(default)]
  pub secret_access_key: String,

  /// AWS region (e.g., us-east-1)
  #[serde(default = "default_region")]
  pub region: String,

  /// Optional bucket name prefix for multi-tenant scenarios
  #[serde(default)]
  pub bucket_prefix: Option<String>,

  /// Force path-style URLs (required for MinIO and self-hosted S3)
  #[serde(default)]
  pub force_path_style: bool,
}

fn default_region() -> String {
  "us-east-1".to_string()
}

impl ProxyConfig {
  /// Check if the proxy config has valid credentials
  pub fn is_configured(&self) -> bool {
    !self.access_key_id.is_empty() && !self.secret_access_key.is_empty()
  }
}

/// Storage feature configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
  /// Port for the storage HTTP server
  pub port: u16,

  /// Path for storing object data (used in builtin mode)
  pub storage_path: String,

  /// Maximum object size in bytes
  pub max_object_size: u64,

  /// Maximum part size for multipart uploads
  pub max_part_size: u64,

  /// Minimum part size for multipart uploads
  pub min_part_size: u64,

  /// Default region (used in builtin mode)
  pub region: String,

  /// Storage mode: builtin or proxy
  #[serde(default)]
  pub mode: StorageMode,

  /// Proxy configuration (used in proxy mode)
  #[serde(default)]
  pub proxy: ProxyConfig,
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
      mode: StorageMode::default(),
      proxy: ProxyConfig::default(),
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
      mode: StorageMode::default(),
      proxy: ProxyConfig::default(),
    }
  }
}
