//! Global state management for admin UI using Leptos signals

#[cfg(feature = "csr")]
use leptos::*;
#[cfg(feature = "csr")]
use serde::{Deserialize, Serialize};

/// Theme setting
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
  Light,
  Dark,
  #[default]
  System,
}

/// Current page in the admin UI
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Page {
  #[default]
  Dashboard,
  Tables,
  Buckets,
  Browser(String), // bucket name
  Explorer,
  Console,
  Live,
  Logs,
  Projects,
  Settings(SettingsTab),
}

/// Settings sub-tabs
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsTab {
  #[default]
  General,
  Tokens,
  Storage,
  Caching,
  Users,
}

/// Toast notification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Toast {
  pub id: u32,
  pub message: String,
  pub level: ToastLevel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToastLevel {
  Info,
  Success,
  Warning,
  Error,
}

/// Table info for sidebar
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableInfo {
  pub name: String,
  pub count: usize,
}

/// Bucket info for sidebar
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BucketInfo {
  pub name: String,
  pub object_count: i64,
  pub current_size: i64,
}

/// Protocol settings
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProtocolSettings {
  pub rest: bool,
  pub websocket: bool,
  pub sse: bool,
  pub tcp: bool,
  pub mcp: bool,
}

/// CORS settings
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CorsSettings {
  pub origins: Vec<String>,
}

/// S3 settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct S3Settings {
  pub enabled: bool,
  pub port: u16,
  pub storage_path: String,
  pub max_object_size: u64,
  pub max_part_size: u64,
  pub region: String,
  // Proxy mode settings
  pub mode: String,
  pub proxy_endpoint: String,
  pub proxy_access_key_id: String,
  pub proxy_region: String,
  pub proxy_bucket_prefix: Option<String>,
  pub proxy_force_path_style: bool,
}

impl Default for S3Settings {
  fn default() -> Self {
    Self {
      enabled: false,
      port: 9000,
      storage_path: "./data/s3".to_string(),
      max_object_size: 5 * 1024 * 1024 * 1024,
      max_part_size: 5 * 1024 * 1024 * 1024,
      region: "us-east-1".to_string(),
      mode: "builtin".to_string(),
      proxy_endpoint: String::new(),
      proxy_access_key_id: String::new(),
      proxy_region: "us-east-1".to_string(),
      proxy_bucket_prefix: None,
      proxy_force_path_style: false,
    }
  }
}

/// Cache settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheSettings {
  pub enabled: bool,
  pub port: u16,
  pub max_memory: String,
  pub eviction: String,
  pub default_ttl: u64,
  pub snapshot_enabled: bool,
  pub snapshot_path: String,
  pub snapshot_interval: u64,
  // Proxy mode settings
  pub mode: String,
  pub proxy_host: String,
  pub proxy_port: u16,
  pub proxy_database: u8,
  pub proxy_tls_enabled: bool,
}

impl Default for CacheSettings {
  fn default() -> Self {
    Self {
      enabled: false,
      port: 6379,
      max_memory: "256mb".to_string(),
      eviction: "lru".to_string(),
      default_ttl: 0,
      snapshot_enabled: false,
      snapshot_path: "./data/cache.snapshot".to_string(),
      snapshot_interval: 300,
      mode: "builtin".to_string(),
      proxy_host: "localhost".to_string(),
      proxy_port: 6379,
      proxy_database: 0,
      proxy_tls_enabled: false,
    }
  }
}

/// Object info for browser
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectInfo {
  pub key: String,
  pub is_folder: bool,
  pub size: Option<i64>,
  pub last_modified: Option<String>,
  pub etag: Option<String>,
}

/// Browser state for navigating buckets
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BrowserState {
  pub bucket: Option<String>,
  pub prefix: String,
  pub objects: Vec<ObjectInfo>,
  pub folders: Vec<String>,
  pub loading: bool,
}

/// Cache statistics
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CacheStats {
  pub keys: usize,
  pub memory_used: usize,
  pub memory_limit: usize,
  pub hits: u64,
  pub misses: u64,
  pub evictions: u64,
  pub expired: u64,
}

/// API token info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenInfo {
  pub id: String,
  pub project_id: String,
  pub name: String,
  pub created_at: String,
}

/// S3 access key info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct S3AccessKey {
  pub access_key_id: String,
  pub name: String,
  pub created_at: String,
}

/// Admin user info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminUserInfo {
  pub id: String,
  pub username: String,
  pub email: Option<String>,
  pub role: String,
  pub created_at: String,
}

/// Project info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
  pub id: String,
  pub name: String,
  pub description: Option<String>,
  pub owner_id: String,
  pub created_at: String,
  pub updated_at: String,
}

/// Project member info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectMemberInfo {
  pub id: String,
  pub project_id: String,
  pub user_id: String,
  pub role: String,
  pub created_at: String,
  pub user: Option<AdminUserInfo>,
}

/// Auth status
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuthStatus {
  pub needs_setup: bool,
  pub logged_in: bool,
  pub user: Option<AdminUserInfo>,
}

/// Server stats
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Stats {
  pub tables: usize,
  pub documents: usize,
  pub backend: String,
  pub uptime_secs: u64,
}

#[cfg(feature = "csr")]
/// Global application state with reactive signals
#[derive(Clone)]
pub struct AppState {
  pub current_page: RwSignal<Page>,
  pub tables: RwSignal<Vec<TableInfo>>,
  pub buckets: RwSignal<Vec<BucketInfo>>,
  pub toasts: RwSignal<Vec<Toast>>,
  pub auth_token: RwSignal<Option<String>>,
  pub stats: RwSignal<Stats>,
  pub protocol_settings: RwSignal<ProtocolSettings>,
  pub cors_settings: RwSignal<CorsSettings>,
  pub storage_settings: RwSignal<S3Settings>,
  pub storage_enabled: RwSignal<bool>,
  pub cache_settings: RwSignal<CacheSettings>,
  pub cache_enabled: RwSignal<bool>,
  pub cache_stats: RwSignal<CacheStats>,
  pub tokens: RwSignal<Vec<TokenInfo>>,
  pub api_auth_required: RwSignal<bool>,
  pub connected: RwSignal<bool>,
  pub toast_counter: RwSignal<u32>,
  pub theme: RwSignal<Theme>,
  // Auth state
  pub auth_status: RwSignal<AuthStatus>,
  pub admin_users: RwSignal<Vec<AdminUserInfo>>,
  // Project state
  pub projects: RwSignal<Vec<ProjectInfo>>,
  pub current_project: RwSignal<Option<String>>,
  pub project_members: RwSignal<Vec<ProjectMemberInfo>>,
  // Browser state
  pub browser_state: RwSignal<BrowserState>,
}

#[cfg(feature = "csr")]
impl AppState {
  pub fn new() -> Self {
    Self {
      current_page: create_rw_signal(Page::Dashboard),
      tables: create_rw_signal(Vec::new()),
      buckets: create_rw_signal(Vec::new()),
      toasts: create_rw_signal(Vec::new()),
      auth_token: create_rw_signal(None),
      stats: create_rw_signal(Stats::default()),
      protocol_settings: create_rw_signal(ProtocolSettings {
        rest: true,
        websocket: true,
        sse: false,
        tcp: true,
        mcp: false,
      }),
      cors_settings: create_rw_signal(CorsSettings {
        origins: vec!["*".to_string()],
      }),
      storage_settings: create_rw_signal(S3Settings::default()),
      storage_enabled: create_rw_signal(false),
      cache_settings: create_rw_signal(CacheSettings::default()),
      cache_enabled: create_rw_signal(false),
      cache_stats: create_rw_signal(CacheStats::default()),
      tokens: create_rw_signal(Vec::new()),
      api_auth_required: create_rw_signal(false),
      connected: create_rw_signal(true),
      toast_counter: create_rw_signal(0),
      theme: create_rw_signal(Theme::System),
      auth_status: create_rw_signal(AuthStatus::default()),
      admin_users: create_rw_signal(Vec::new()),
      projects: create_rw_signal(Vec::new()),
      current_project: create_rw_signal(None),
      project_members: create_rw_signal(Vec::new()),
      browser_state: create_rw_signal(BrowserState::default()),
    }
  }

  pub fn show_toast(&self, message: &str, level: ToastLevel) {
    let id = self.toast_counter.get() + 1;
    self.toast_counter.set(id);
    self.toasts.update(|toasts| {
      toasts.push(Toast {
        id,
        message: message.to_string(),
        level,
      });
    });
  }

  pub fn remove_toast(&self, id: u32) {
    self.toasts.update(|toasts| {
      toasts.retain(|t| t.id != id);
    });
  }

  pub fn navigate(&self, page: Page) {
    self.current_page.set(page);
  }
}

#[cfg(feature = "csr")]
impl Default for AppState {
  fn default() -> Self {
    Self::new()
  }
}
