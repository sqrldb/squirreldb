use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Expand environment variables in a string.
/// Supports $VAR_NAME and ${VAR_NAME} syntax.
fn expand_env_vars(input: &str) -> String {
  let mut result = input.to_string();

  // Handle ${VAR_NAME} syntax first (more specific)
  while let Some(start) = result.find("${") {
    if let Some(end) = result[start..].find('}') {
      let var_name = &result[start + 2..start + end];
      let value = std::env::var(var_name).unwrap_or_default();
      result = format!(
        "{}{}{}",
        &result[..start],
        value,
        &result[start + end + 1..]
      );
    } else {
      break;
    }
  }

  // Handle $VAR_NAME syntax (word boundary: alphanumeric + underscore)
  let mut i = 0;
  while i < result.len() {
    if result[i..].starts_with('$') && !result[i..].starts_with("${") {
      let rest = &result[i + 1..];
      let var_len = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .count();
      if var_len > 0 {
        let var_name = &rest[..var_len];
        let value = std::env::var(var_name).unwrap_or_default();
        result = format!("{}{}{}", &result[..i], value, &rest[var_len..]);
        i += value.len();
        continue;
      }
    }
    i += 1;
  }

  result
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
  #[default]
  Postgres,
  Sqlite,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerConfig {
  #[serde(default)]
  pub server: ServerSection,
  #[serde(default)]
  pub backend: BackendType,
  #[serde(default)]
  pub postgres: PostgresSection,
  #[serde(default)]
  pub sqlite: SqliteSection,
  #[serde(default)]
  pub logging: LoggingSection,
  #[serde(default)]
  pub auth: AuthSection,
  #[serde(default)]
  pub limits: LimitsSection,
  #[serde(default)]
  pub features: FeaturesSection,
  #[serde(default)]
  pub storage: StorageSection,
}

/// Feature toggle configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeaturesSection {
  /// Enable object storage
  #[serde(default)]
  pub storage: bool,
}

/// Object storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSection {
  /// Port for storage HTTP server
  #[serde(default = "default_storage_port")]
  pub port: u16,

  /// Storage path for objects
  #[serde(default = "default_storage_path")]
  pub storage_path: String,

  /// Maximum object size in bytes (default 5GB)
  #[serde(default = "default_storage_max_object_size")]
  pub max_object_size: u64,

  /// Maximum part size for multipart uploads (default 5GB)
  #[serde(default = "default_storage_max_part_size")]
  pub max_part_size: u64,

  /// Minimum part size for multipart uploads (default 5MB)
  #[serde(default = "default_storage_min_part_size")]
  pub min_part_size: u64,

  /// Default region for storage operations
  #[serde(default = "default_storage_region")]
  pub region: String,

  /// Feature-specific configuration overrides
  #[serde(default)]
  pub config: HashMap<String, serde_json::Value>,
}

fn default_storage_port() -> u16 {
  9000
}

fn default_storage_path() -> String {
  "./data/storage".into()
}

fn default_storage_max_object_size() -> u64 {
  5 * 1024 * 1024 * 1024 // 5GB
}

fn default_storage_max_part_size() -> u64 {
  5 * 1024 * 1024 * 1024 // 5GB
}

fn default_storage_min_part_size() -> u64 {
  5 * 1024 * 1024 // 5MB
}

fn default_storage_region() -> String {
  "us-east-1".into()
}

impl Default for StorageSection {
  fn default() -> Self {
    Self {
      port: default_storage_port(),
      storage_path: default_storage_path(),
      max_object_size: default_storage_max_object_size(),
      max_part_size: default_storage_max_part_size(),
      min_part_size: default_storage_min_part_size(),
      region: default_storage_region(),
      config: HashMap::new(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSection {
  #[serde(default = "default_host")]
  pub host: String,
  #[serde(default)]
  pub ports: PortsSection,
  #[serde(default)]
  pub protocols: ProtocolsSection,
  /// CORS allowed origins for browser SDK support
  /// Use ["*"] for permissive mode, or specify origins like ["http://localhost:3000"]
  #[serde(default)]
  pub cors_origins: Vec<String>,
}

fn default_host() -> String {
  "0.0.0.0".into()
}

impl Default for ServerSection {
  fn default() -> Self {
    Self {
      host: default_host(),
      ports: PortsSection::default(),
      protocols: ProtocolsSection::default(),
      cors_origins: vec!["*".to_string()], // Permissive by default for development
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortsSection {
  #[serde(default = "default_http_port")]
  pub http: u16,
  #[serde(default = "default_admin_port")]
  pub admin: u16,
  #[serde(default = "default_tcp_port")]
  pub tcp: u16,
  #[serde(default = "default_mcp_port")]
  pub mcp: u16,
}

fn default_http_port() -> u16 {
  8080
}
fn default_admin_port() -> u16 {
  8081
}
fn default_tcp_port() -> u16 {
  8082
}
fn default_mcp_port() -> u16 {
  8083
}

impl Default for PortsSection {
  fn default() -> Self {
    Self {
      http: default_http_port(),
      admin: default_admin_port(),
      tcp: default_tcp_port(),
      mcp: default_mcp_port(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolsSection {
  #[serde(default = "default_true")]
  pub rest: bool,
  #[serde(default = "default_true")]
  pub websocket: bool,
  #[serde(default)]
  pub sse: bool,
  #[serde(default = "default_true")]
  pub tcp: bool,
  #[serde(default)]
  pub mcp: bool,
}

fn default_true() -> bool {
  true
}

impl Default for ProtocolsSection {
  fn default() -> Self {
    Self {
      rest: true,
      websocket: true,
      sse: false,
      tcp: true,
      mcp: false,
    }
  }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthSection {
  #[serde(default)]
  pub enabled: bool,
  #[serde(default)]
  pub admin_token: Option<String>,
}

/// Rate limiting and resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsSection {
  /// Maximum connections per IP address (0 = unlimited)
  #[serde(default = "default_max_connections_per_ip")]
  pub max_connections_per_ip: u32,

  /// Maximum requests per second per client (0 = unlimited)
  #[serde(default = "default_requests_per_second")]
  pub requests_per_second: u32,

  /// Burst size for rate limiting (requests allowed in a burst)
  #[serde(default = "default_burst_size")]
  pub burst_size: u32,

  /// Query execution timeout in milliseconds (0 = no timeout)
  #[serde(default = "default_query_timeout_ms")]
  pub query_timeout_ms: u64,

  /// Maximum concurrent queries per client (0 = unlimited)
  #[serde(default = "default_max_concurrent_queries")]
  pub max_concurrent_queries: u32,

  /// Maximum message size in bytes
  #[serde(default = "default_max_message_size")]
  pub max_message_size: usize,
}

fn default_max_connections_per_ip() -> u32 {
  100
}
fn default_requests_per_second() -> u32 {
  100
}
fn default_burst_size() -> u32 {
  50
}
fn default_query_timeout_ms() -> u64 {
  30000 // 30 seconds
}
fn default_max_concurrent_queries() -> u32 {
  10
}
fn default_max_message_size() -> usize {
  16 * 1024 * 1024 // 16 MB
}

impl Default for LimitsSection {
  fn default() -> Self {
    Self {
      max_connections_per_ip: default_max_connections_per_ip(),
      requests_per_second: default_requests_per_second(),
      burst_size: default_burst_size(),
      query_timeout_ms: default_query_timeout_ms(),
      max_concurrent_queries: default_max_concurrent_queries(),
      max_message_size: default_max_message_size(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresSection {
  #[serde(default = "default_pg_url")]
  pub url: String,
  #[serde(default = "default_max_conn")]
  pub max_connections: usize,
}
fn default_pg_url() -> String {
  "postgres://localhost/squirreldb".into()
}
fn default_max_conn() -> usize {
  20
}
impl Default for PostgresSection {
  fn default() -> Self {
    Self {
      url: default_pg_url(),
      max_connections: default_max_conn(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteSection {
  #[serde(default = "default_sqlite_path")]
  pub path: String,
}
fn default_sqlite_path() -> String {
  "squirreldb.db".into()
}
impl Default for SqliteSection {
  fn default() -> Self {
    Self {
      path: default_sqlite_path(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSection {
  #[serde(default = "default_level")]
  pub level: String,
}
fn default_level() -> String {
  "info".into()
}
impl Default for LoggingSection {
  fn default() -> Self {
    Self {
      level: default_level(),
    }
  }
}

impl ServerConfig {
  pub fn from_file(path: impl AsRef<Path>) -> Result<Self, anyhow::Error> {
    let content = std::fs::read_to_string(&path)?;
    let expanded = expand_env_vars(&content);
    Ok(serde_yaml::from_str(&expanded)?)
  }

  pub fn find_and_load() -> Result<Option<Self>, anyhow::Error> {
    for p in ["squirreldb.yaml", "squirreldb.yml"] {
      if Path::new(p).exists() {
        tracing::info!("Loading config from {}", p);
        return Ok(Some(Self::from_file(p)?));
      }
    }
    Ok(None)
  }

  pub fn address(&self) -> String {
    format!("{}:{}", self.server.host, self.server.ports.http)
  }

  pub fn admin_address(&self) -> String {
    format!("{}:{}", self.server.host, self.server.ports.admin)
  }

  pub fn tcp_address(&self) -> String {
    format!("{}:{}", self.server.host, self.server.ports.tcp)
  }

  pub fn mcp_address(&self) -> String {
    format!("{}:{}", self.server.host, self.server.ports.mcp)
  }

  pub fn storage_address(&self) -> String {
    format!("{}:{}", self.server.host, self.storage.port)
  }
}
