//! Cache configuration

use serde::{Deserialize, Serialize};

use super::store::EvictionPolicy;

/// Cache mode: builtin in-memory or proxy to external Redis
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheMode {
  /// In-memory cache (default)
  #[default]
  Builtin,
  /// Proxy to external Redis server
  Proxy,
}

impl std::fmt::Display for CacheMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CacheMode::Builtin => write!(f, "builtin"),
      CacheMode::Proxy => write!(f, "proxy"),
    }
  }
}

impl std::str::FromStr for CacheMode {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "builtin" | "memory" | "inmemory" => Ok(CacheMode::Builtin),
      "proxy" | "external" | "redis" => Ok(CacheMode::Proxy),
      _ => Err(format!("Unknown cache mode: {}", s)),
    }
  }
}

/// Configuration for proxy mode (external Redis)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheProxyConfig {
  /// Redis host
  #[serde(default = "default_host")]
  pub host: String,

  /// Redis port
  #[serde(default = "default_redis_port")]
  pub port: u16,

  /// Redis password (optional)
  #[serde(default)]
  pub password: Option<String>,

  /// Redis database number
  #[serde(default)]
  pub database: u8,

  /// Enable TLS
  #[serde(default)]
  pub tls_enabled: bool,
}

fn default_host() -> String {
  "localhost".to_string()
}

fn default_redis_port() -> u16 {
  6379
}

impl CacheProxyConfig {
  /// Check if the proxy config has a host configured
  pub fn is_configured(&self) -> bool {
    !self.host.is_empty()
  }

  /// Generate Redis connection URL
  pub fn connection_url(&self) -> String {
    let scheme = if self.tls_enabled { "rediss" } else { "redis" };
    let auth = match &self.password {
      Some(pwd) if !pwd.is_empty() => format!(":{}@", pwd),
      _ => String::new(),
    };
    format!(
      "{}://{}{}:{}/{}",
      scheme, auth, self.host, self.port, self.database
    )
  }
}

/// Cache feature configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
  /// TCP port for Redis protocol (default: 6379)
  #[serde(default = "default_port")]
  pub port: u16,

  /// Maximum memory usage (e.g., "256mb", "1gb")
  #[serde(default = "default_max_memory")]
  pub max_memory: String,

  /// Eviction policy when memory limit is reached
  #[serde(default)]
  pub eviction: EvictionPolicy,

  /// Default TTL in seconds (0 = no expiry)
  #[serde(default)]
  pub default_ttl: u64,

  /// Snapshot configuration
  #[serde(default)]
  pub snapshot: CacheSnapshotConfig,

  /// Cache mode: builtin or proxy
  #[serde(default)]
  pub mode: CacheMode,

  /// Proxy configuration (used in proxy mode)
  #[serde(default)]
  pub proxy: CacheProxyConfig,
}

/// Snapshot persistence configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheSnapshotConfig {
  /// Enable periodic snapshots
  #[serde(default)]
  pub enabled: bool,

  /// Path to snapshot file
  #[serde(default = "default_snapshot_path")]
  pub path: String,

  /// Interval between snapshots in seconds
  #[serde(default = "default_snapshot_interval")]
  pub interval: u64,
}

fn default_port() -> u16 {
  6379
}

fn default_max_memory() -> String {
  "256mb".to_string()
}

fn default_snapshot_path() -> String {
  "./data/cache.snapshot".to_string()
}

fn default_snapshot_interval() -> u64 {
  300 // 5 minutes
}

impl Default for CacheConfig {
  fn default() -> Self {
    Self {
      port: default_port(),
      max_memory: default_max_memory(),
      eviction: EvictionPolicy::default(),
      default_ttl: 0,
      snapshot: CacheSnapshotConfig::default(),
      mode: CacheMode::default(),
      proxy: CacheProxyConfig::default(),
    }
  }
}

impl CacheConfig {
  /// Parse memory size string (e.g., "256mb", "1gb") to bytes
  pub fn max_memory_bytes(&self) -> usize {
    parse_memory_size(&self.max_memory).unwrap_or(256 * 1024 * 1024)
  }
}

impl From<&crate::server::CachingSection> for CacheConfig {
  fn from(section: &crate::server::CachingSection) -> Self {
    Self {
      port: section.port,
      max_memory: section.max_memory.clone(),
      eviction: section.eviction.parse().unwrap_or_default(),
      default_ttl: section.default_ttl,
      snapshot: CacheSnapshotConfig {
        enabled: section.snapshot.enabled,
        path: section.snapshot.path.clone(),
        interval: section.snapshot.interval,
      },
      mode: CacheMode::default(),
      proxy: CacheProxyConfig::default(),
    }
  }
}

/// Parse a memory size string to bytes
/// Supports: b, kb, mb, gb (case insensitive)
pub fn parse_memory_size(s: &str) -> Option<usize> {
  let s = s.trim().to_lowercase();

  if s.ends_with("gb") {
    s[..s.len() - 2]
      .trim()
      .parse::<usize>()
      .ok()
      .map(|n| n * 1024 * 1024 * 1024)
  } else if s.ends_with("mb") {
    s[..s.len() - 2]
      .trim()
      .parse::<usize>()
      .ok()
      .map(|n| n * 1024 * 1024)
  } else if s.ends_with("kb") {
    s[..s.len() - 2]
      .trim()
      .parse::<usize>()
      .ok()
      .map(|n| n * 1024)
  } else if s.ends_with('b') {
    s[..s.len() - 1].trim().parse::<usize>().ok()
  } else {
    // Assume bytes if no suffix
    s.parse::<usize>().ok()
  }
}

/// Format bytes as human-readable string
pub fn format_memory_size(bytes: usize) -> String {
  const GB: usize = 1024 * 1024 * 1024;
  const MB: usize = 1024 * 1024;
  const KB: usize = 1024;

  if bytes >= GB {
    format!("{:.1}GB", bytes as f64 / GB as f64)
  } else if bytes >= MB {
    format!("{:.1}MB", bytes as f64 / MB as f64)
  } else if bytes >= KB {
    format!("{:.1}KB", bytes as f64 / KB as f64)
  } else {
    format!("{}B", bytes)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_memory_size() {
    assert_eq!(parse_memory_size("256mb"), Some(256 * 1024 * 1024));
    assert_eq!(parse_memory_size("1gb"), Some(1024 * 1024 * 1024));
    assert_eq!(parse_memory_size("512kb"), Some(512 * 1024));
    assert_eq!(parse_memory_size("1024b"), Some(1024));
    assert_eq!(parse_memory_size("1024"), Some(1024));
    assert_eq!(parse_memory_size("256 MB"), Some(256 * 1024 * 1024));
    assert_eq!(parse_memory_size("invalid"), None);
  }

  #[test]
  fn test_format_memory_size() {
    assert_eq!(format_memory_size(1024 * 1024 * 1024), "1.0GB");
    assert_eq!(format_memory_size(256 * 1024 * 1024), "256.0MB");
    assert_eq!(format_memory_size(512 * 1024), "512.0KB");
    assert_eq!(format_memory_size(1024), "1.0KB");
    assert_eq!(format_memory_size(500), "500B");
  }
}
