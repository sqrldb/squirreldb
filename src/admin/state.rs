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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Page {
  #[default]
  Dashboard,
  Tables,
  Buckets,
  Explorer,
  Console,
  Live,
  Logs,
  Settings(SettingsTab),
}

/// Settings sub-tabs
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsTab {
  #[default]
  General,
  Tokens,
  Storage,
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

/// S3 settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct S3Settings {
  pub enabled: bool,
  pub port: u16,
  pub storage_path: String,
  pub max_object_size: u64,
  pub max_part_size: u64,
  pub region: String,
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
    }
  }
}

/// API token info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenInfo {
  pub id: String,
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
  pub storage_settings: RwSignal<S3Settings>,
  pub storage_enabled: RwSignal<bool>,
  pub tokens: RwSignal<Vec<TokenInfo>>,
  pub connected: RwSignal<bool>,
  pub toast_counter: RwSignal<u32>,
  pub theme: RwSignal<Theme>,
  // Auth state
  pub auth_status: RwSignal<AuthStatus>,
  pub admin_users: RwSignal<Vec<AdminUserInfo>>,
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
      storage_settings: create_rw_signal(S3Settings::default()),
      storage_enabled: create_rw_signal(false),
      tokens: create_rw_signal(Vec::new()),
      connected: create_rw_signal(true),
      toast_counter: create_rw_signal(0),
      theme: create_rw_signal(Theme::System),
      auth_status: create_rw_signal(AuthStatus::default()),
      admin_users: create_rw_signal(Vec::new()),
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
