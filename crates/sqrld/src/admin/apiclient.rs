//! API client for communicating with the server

#[cfg(feature = "csr")]
use gloo_net::http::{Request, RequestBuilder};
#[cfg(feature = "csr")]
use gloo_storage::{LocalStorage, Storage};
#[cfg(feature = "csr")]
use serde::{de::DeserializeOwned, Serialize};

#[cfg(feature = "csr")]
use crate::admin::state::{
  AdminUserInfo, AuthStatus, BucketInfo, CacheSettings, CacheStats, ProjectInfo, ProjectMemberInfo,
  S3AccessKey, S3Settings, Stats, TableInfo, TokenInfo,
};

const TOKEN_KEY: &str = "sqrl_admin_token";

#[cfg(feature = "csr")]
pub fn get_stored_token() -> Option<String> {
  LocalStorage::get(TOKEN_KEY).ok()
}

#[cfg(feature = "csr")]
pub fn set_stored_token(token: &str) {
  let _ = LocalStorage::set(TOKEN_KEY, token);
}

#[cfg(feature = "csr")]
pub fn clear_stored_token() {
  LocalStorage::delete(TOKEN_KEY);
}

#[cfg(feature = "csr")]
fn add_auth_header(req: RequestBuilder) -> RequestBuilder {
  if let Some(token) = get_stored_token() {
    req.header("Authorization", &format!("Bearer {}", token))
  } else {
    req
  }
}

#[cfg(feature = "csr")]
async fn fetch_with_auth<T: DeserializeOwned>(url: &str) -> Result<T, String> {
  let req = add_auth_header(Request::get(url));
  let resp = req.send().await.map_err(|e| e.to_string())?;
  if resp.status() == 401 {
    return Err("Unauthorized".to_string());
  }
  if !resp.ok() {
    return Err(format!("HTTP error: {}", resp.status()));
  }
  resp.json().await.map_err(|e| e.to_string())
}

#[cfg(feature = "csr")]
async fn post_with_auth<T: Serialize, R: DeserializeOwned>(
  url: &str,
  body: &T,
) -> Result<R, String> {
  let req = add_auth_header(Request::post(url))
    .json(body)
    .map_err(|e| e.to_string())?;
  let resp = req.send().await.map_err(|e| e.to_string())?;
  if resp.status() == 401 {
    return Err("Unauthorized".to_string());
  }
  if !resp.ok() {
    return Err(format!("HTTP error: {}", resp.status()));
  }
  resp.json().await.map_err(|e| e.to_string())
}

#[cfg(feature = "csr")]
async fn put_with_auth<T: Serialize, R: DeserializeOwned>(
  url: &str,
  body: &T,
) -> Result<R, String> {
  let req = add_auth_header(Request::put(url))
    .json(body)
    .map_err(|e| e.to_string())?;
  let resp = req.send().await.map_err(|e| e.to_string())?;
  if resp.status() == 401 {
    return Err("Unauthorized".to_string());
  }
  if !resp.ok() {
    return Err(format!("HTTP error: {}", resp.status()));
  }
  resp.json().await.map_err(|e| e.to_string())
}

#[cfg(feature = "csr")]
async fn delete_with_auth<R: DeserializeOwned>(url: &str) -> Result<R, String> {
  let req = add_auth_header(Request::delete(url));
  let resp = req.send().await.map_err(|e| e.to_string())?;
  if resp.status() == 401 {
    return Err("Unauthorized".to_string());
  }
  if !resp.ok() {
    return Err(format!("HTTP error: {}", resp.status()));
  }
  resp.json().await.map_err(|e| e.to_string())
}

// =============================================================================
// API Functions
// =============================================================================

#[cfg(feature = "csr")]
pub async fn fetch_status() -> Result<Stats, String> {
  #[derive(serde::Deserialize)]
  struct StatusResp {
    backend: String,
    uptime_secs: u64,
  }
  let status: StatusResp = fetch_with_auth("/api/status").await?;
  Ok(Stats {
    backend: status.backend,
    uptime_secs: status.uptime_secs,
    tables: 0,
    documents: 0,
  })
}

#[cfg(feature = "csr")]
pub async fn fetch_tables() -> Result<Vec<TableInfo>, String> {
  #[derive(serde::Deserialize)]
  struct CollResp {
    name: String,
    count: usize,
  }
  let collections: Vec<CollResp> = fetch_with_auth("/api/collections").await?;
  Ok(
    collections
      .into_iter()
      .map(|c| TableInfo {
        name: c.name,
        count: c.count,
      })
      .collect(),
  )
}

#[cfg(feature = "csr")]
pub async fn fetch_storage_settings() -> Result<S3Settings, String> {
  fetch_with_auth("/api/s3/settings").await
}

#[cfg(feature = "csr")]
pub async fn update_storage_settings(
  port: Option<u16>,
  storage_path: Option<String>,
  region: Option<String>,
) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct UpdateReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<String>,
  }
  put_with_auth(
    "/api/s3/settings",
    &UpdateReq {
      port,
      storage_path,
      region,
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn toggle_feature(name: &str, enabled: bool) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct ToggleReq {
    enabled: bool,
  }
  put_with_auth(&format!("/api/features/{}", name), &ToggleReq { enabled }).await
}

#[cfg(feature = "csr")]
pub async fn fetch_auth_settings() -> Result<bool, String> {
  #[derive(serde::Deserialize)]
  struct AuthSettingsResp {
    auth_required: bool,
  }
  let settings: AuthSettingsResp = fetch_with_auth("/api/settings/auth").await?;
  Ok(settings.auth_required)
}

#[cfg(feature = "csr")]
pub async fn update_auth_settings(auth_required: bool) -> Result<bool, String> {
  #[derive(Serialize)]
  struct UpdateReq {
    auth_required: bool,
  }
  #[derive(serde::Deserialize)]
  struct AuthSettingsResp {
    auth_required: bool,
  }
  let resp: AuthSettingsResp = put_with_auth("/api/settings/auth", &UpdateReq { auth_required }).await?;
  Ok(resp.auth_required)
}

#[cfg(feature = "csr")]
pub async fn fetch_protocol_settings() -> Result<crate::admin::state::ProtocolSettings, String> {
  fetch_with_auth("/api/settings/protocols").await
}

#[cfg(feature = "csr")]
pub async fn update_protocol_settings(
  rest: Option<bool>,
  websocket: Option<bool>,
  sse: Option<bool>,
  tcp: Option<bool>,
  mcp: Option<bool>,
) -> Result<crate::admin::state::ProtocolSettings, String> {
  #[derive(Serialize)]
  struct UpdateReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    rest: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    websocket: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tcp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mcp: Option<bool>,
  }
  put_with_auth("/api/settings/protocols", &UpdateReq { rest, websocket, sse, tcp, mcp }).await
}

#[cfg(feature = "csr")]
pub async fn fetch_cors_settings() -> Result<crate::admin::state::CorsSettings, String> {
  fetch_with_auth("/api/settings/cors").await
}

#[cfg(feature = "csr")]
pub async fn update_cors_settings(origins: Vec<String>) -> Result<crate::admin::state::CorsSettings, String> {
  #[derive(serde::Serialize)]
  struct UpdateReq {
    origins: Vec<String>,
  }
  put_with_auth("/api/settings/cors", &UpdateReq { origins }).await
}

#[cfg(feature = "csr")]
pub async fn restart_server() -> Result<(), String> {
  let _: serde_json::Value = post_with_auth("/api/server/restart", &serde_json::json!({})).await?;
  Ok(())
}

#[cfg(feature = "csr")]
pub async fn health_check() -> Result<bool, String> {
  let _: serde_json::Value = fetch_with_auth("/api/server/health").await?;
  Ok(true)
}

#[cfg(feature = "csr")]
pub async fn fetch_buckets() -> Result<Vec<BucketInfo>, String> {
  #[derive(serde::Deserialize)]
  struct BucketResp {
    name: String,
    object_count: i64,
    current_size: i64,
  }
  let buckets: Vec<BucketResp> = fetch_with_auth("/api/s3/buckets").await?;
  Ok(
    buckets
      .into_iter()
      .map(|b| BucketInfo {
        name: b.name,
        object_count: b.object_count,
        current_size: b.current_size,
      })
      .collect(),
  )
}

#[cfg(feature = "csr")]
pub async fn create_bucket(name: &str) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct CreateReq {
    name: String,
  }
  post_with_auth(
    "/api/s3/buckets",
    &CreateReq {
      name: name.to_string(),
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn delete_bucket(name: &str) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!("/api/s3/buckets/{}", name)).await
}

#[cfg(feature = "csr")]
pub async fn fetch_tokens(project_id: &str) -> Result<Vec<TokenInfo>, String> {
  #[derive(serde::Deserialize)]
  struct TokenResp {
    id: String,
    project_id: String,
    name: String,
    created_at: String,
  }
  let tokens: Vec<TokenResp> = fetch_with_auth(&format!("/api/projects/{}/tokens", project_id)).await?;
  Ok(
    tokens
      .into_iter()
      .map(|t| TokenInfo {
        id: t.id,
        project_id: t.project_id,
        name: t.name,
        created_at: t.created_at,
      })
      .collect(),
  )
}

#[cfg(feature = "csr")]
pub async fn create_token(project_id: &str, name: &str) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct CreateReq {
    name: String,
  }
  post_with_auth(
    &format!("/api/projects/{}/tokens", project_id),
    &CreateReq {
      name: name.to_string(),
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn delete_token(project_id: &str, id: &str) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!("/api/projects/{}/tokens/{}", project_id, id)).await
}

#[cfg(feature = "csr")]
pub async fn run_query(query: &str) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct QueryReq {
    query: String,
  }
  post_with_auth(
    "/api/query",
    &QueryReq {
      query: query.to_string(),
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn create_table(name: &str) -> Result<serde_json::Value, String> {
  // Create table by inserting and deleting a dummy doc, or use a dedicated endpoint
  // For now, we run a query to create the table
  run_query(&format!("db.tableCreate('{}').run()", name)).await
}

#[cfg(feature = "csr")]
pub async fn drop_table(name: &str) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!("/api/collections/{}", name)).await
}

#[cfg(feature = "csr")]
pub async fn validate_token(token: &str) -> bool {
  let req = Request::get("/api/settings").header("Authorization", &format!("Bearer {}", token));
  match req.send().await {
    Ok(resp) => resp.ok(),
    Err(_) => false,
  }
}

// =============================================================================
// S3 Access Keys
// =============================================================================

#[cfg(feature = "csr")]
pub async fn fetch_s3_keys() -> Result<Vec<S3AccessKey>, String> {
  #[derive(serde::Deserialize)]
  struct KeyResp {
    access_key_id: String,
    name: String,
    created_at: String,
  }
  let keys: Vec<KeyResp> = fetch_with_auth("/api/s3/keys").await?;
  Ok(
    keys
      .into_iter()
      .map(|k| S3AccessKey {
        access_key_id: k.access_key_id,
        name: k.name,
        created_at: k.created_at,
      })
      .collect(),
  )
}

#[cfg(feature = "csr")]
pub async fn create_s3_key(name: &str) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct CreateReq {
    name: String,
  }
  post_with_auth(
    "/api/s3/keys",
    &CreateReq {
      name: name.to_string(),
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn delete_s3_key(access_key_id: &str) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!("/api/s3/keys/{}", access_key_id)).await
}

// =============================================================================
// User Authentication
// =============================================================================

#[cfg(feature = "csr")]
pub async fn fetch_auth_status() -> Result<AuthStatus, String> {
  fetch_with_auth("/api/auth/status").await
}

#[cfg(feature = "csr")]
pub async fn setup_admin(
  username: &str,
  email: Option<&str>,
  password: &str,
) -> Result<serde_json::Value, String> {
  #[derive(serde::Serialize)]
  struct SetupReq {
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    password: String,
  }
  let resp: serde_json::Value = post_with_auth(
    "/api/auth/setup",
    &SetupReq {
      username: username.to_string(),
      email: email.map(|s| s.to_string()),
      password: password.to_string(),
    },
  )
  .await?;

  // Store the session token
  if let Some(token) = resp.get("token").and_then(|v| v.as_str()) {
    set_stored_token(token);
  }
  Ok(resp)
}

#[cfg(feature = "csr")]
pub async fn login(username: &str, password: &str) -> Result<serde_json::Value, String> {
  #[derive(serde::Serialize)]
  struct LoginReq {
    username: String,
    password: String,
  }
  let resp: serde_json::Value = post_with_auth(
    "/api/auth/login",
    &LoginReq {
      username: username.to_string(),
      password: password.to_string(),
    },
  )
  .await?;

  // Store the session token
  if let Some(token) = resp.get("token").and_then(|v| v.as_str()) {
    set_stored_token(token);
  }
  Ok(resp)
}

#[cfg(feature = "csr")]
pub async fn logout() -> Result<serde_json::Value, String> {
  let resp = post_with_auth("/api/auth/logout", &serde_json::json!({})).await;
  clear_stored_token();
  resp
}

#[cfg(feature = "csr")]
pub async fn change_password(current_password: &str, new_password: &str) -> Result<serde_json::Value, String> {
  #[derive(serde::Serialize)]
  struct ChangePasswordReq {
    current_password: String,
    new_password: String,
  }
  post_with_auth(
    "/api/auth/change-password",
    &ChangePasswordReq {
      current_password: current_password.to_string(),
      new_password: new_password.to_string(),
    },
  )
  .await
}

// =============================================================================
// User Management
// =============================================================================

#[cfg(feature = "csr")]
pub async fn fetch_admin_users() -> Result<Vec<AdminUserInfo>, String> {
  fetch_with_auth("/api/users").await
}

#[cfg(feature = "csr")]
pub async fn create_admin_user(
  username: &str,
  email: Option<&str>,
  password: &str,
  role: &str,
) -> Result<AdminUserInfo, String> {
  #[derive(serde::Serialize)]
  struct CreateReq {
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    password: String,
    role: String,
  }
  post_with_auth(
    "/api/users",
    &CreateReq {
      username: username.to_string(),
      email: email.map(|s| s.to_string()),
      password: password.to_string(),
      role: role.to_string(),
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn delete_admin_user(id: &str) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!("/api/users/{}", id)).await
}

#[cfg(feature = "csr")]
pub async fn update_admin_user_role(id: &str, role: &str) -> Result<serde_json::Value, String> {
  #[derive(serde::Serialize)]
  struct UpdateRoleReq {
    role: String,
  }
  put_with_auth(
    &format!("/api/users/{}/role", id),
    &UpdateRoleReq {
      role: role.to_string(),
    },
  )
  .await
}

// =============================================================================
// Cache Management
// =============================================================================

#[cfg(feature = "csr")]
pub async fn fetch_cache_settings() -> Result<CacheSettings, String> {
  fetch_with_auth("/api/cache/settings").await
}

#[cfg(feature = "csr")]
pub async fn update_cache_settings(
  port: Option<u16>,
  max_memory: Option<String>,
  eviction: Option<String>,
  default_ttl: Option<u64>,
  snapshot_enabled: Option<bool>,
  snapshot_path: Option<String>,
  snapshot_interval: Option<u64>,
) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct UpdateReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    eviction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_ttl: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_interval: Option<u64>,
  }
  put_with_auth(
    "/api/cache/settings",
    &UpdateReq {
      port,
      max_memory,
      eviction,
      default_ttl,
      snapshot_enabled,
      snapshot_path,
      snapshot_interval,
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn fetch_cache_stats() -> Result<CacheStats, String> {
  fetch_with_auth("/api/cache/stats").await
}

#[cfg(feature = "csr")]
pub async fn flush_cache() -> Result<serde_json::Value, String> {
  post_with_auth("/api/cache/flush", &serde_json::json!({})).await
}

// =============================================================================
// Project Management
// =============================================================================

#[cfg(feature = "csr")]
pub async fn fetch_projects() -> Result<Vec<ProjectInfo>, String> {
  fetch_with_auth("/api/projects").await
}

#[cfg(feature = "csr")]
pub async fn create_project(
  name: &str,
  description: Option<&str>,
) -> Result<ProjectInfo, String> {
  #[derive(serde::Serialize)]
  struct CreateReq {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
  }
  post_with_auth(
    "/api/projects",
    &CreateReq {
      name: name.to_string(),
      description: description.map(|s| s.to_string()),
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn get_project(id: &str) -> Result<ProjectInfo, String> {
  fetch_with_auth(&format!("/api/projects/{}", id)).await
}

#[cfg(feature = "csr")]
pub async fn update_project(
  id: &str,
  name: &str,
  description: Option<&str>,
) -> Result<ProjectInfo, String> {
  #[derive(serde::Serialize)]
  struct UpdateReq {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
  }
  put_with_auth(
    &format!("/api/projects/{}", id),
    &UpdateReq {
      name: name.to_string(),
      description: description.map(|s| s.to_string()),
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn delete_project(id: &str) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!("/api/projects/{}", id)).await
}

#[cfg(feature = "csr")]
pub async fn fetch_project_members(project_id: &str) -> Result<Vec<ProjectMemberInfo>, String> {
  fetch_with_auth(&format!("/api/projects/{}/members", project_id)).await
}

#[cfg(feature = "csr")]
pub async fn add_project_member(
  project_id: &str,
  user_id: &str,
  role: &str,
) -> Result<ProjectMemberInfo, String> {
  #[derive(serde::Serialize)]
  struct AddMemberReq {
    user_id: String,
    role: String,
  }
  post_with_auth(
    &format!("/api/projects/{}/members", project_id),
    &AddMemberReq {
      user_id: user_id.to_string(),
      role: role.to_string(),
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn update_project_member_role(
  project_id: &str,
  user_id: &str,
  role: &str,
) -> Result<serde_json::Value, String> {
  #[derive(serde::Serialize)]
  struct UpdateRoleReq {
    role: String,
  }
  put_with_auth(
    &format!("/api/projects/{}/members/{}", project_id, user_id),
    &UpdateRoleReq {
      role: role.to_string(),
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn remove_project_member(
  project_id: &str,
  user_id: &str,
) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!("/api/projects/{}/members/{}", project_id, user_id)).await
}

#[cfg(feature = "csr")]
pub async fn select_project(id: &str) -> Result<ProjectInfo, String> {
  post_with_auth(&format!("/api/projects/{}/select", id), &serde_json::json!({})).await
}

// =============================================================================
// Storage Browser
// =============================================================================

#[cfg(feature = "csr")]
use crate::admin::state::ObjectInfo;

#[cfg(feature = "csr")]
pub async fn list_bucket_objects(
  bucket: &str,
  prefix: Option<&str>,
) -> Result<(Vec<ObjectInfo>, Vec<String>), String> {
  #[derive(serde::Deserialize)]
  struct BrowserObject {
    key: String,
    is_folder: bool,
    size: Option<i64>,
    last_modified: Option<String>,
    etag: Option<String>,
  }
  #[derive(serde::Deserialize)]
  struct ListObjectsResp {
    objects: Vec<BrowserObject>,
    common_prefixes: Vec<String>,
  }

  let url = match prefix {
    Some(p) if !p.is_empty() => format!(
      "/api/s3/buckets/{}/objects?prefix={}&delimiter=/",
      bucket,
      urlencoding::encode(p)
    ),
    _ => format!("/api/s3/buckets/{}/objects?delimiter=/", bucket),
  };

  let resp: ListObjectsResp = fetch_with_auth(&url).await?;
  let objects = resp
    .objects
    .into_iter()
    .map(|o| ObjectInfo {
      key: o.key,
      is_folder: o.is_folder,
      size: o.size,
      last_modified: o.last_modified,
      etag: o.etag,
    })
    .collect();
  Ok((objects, resp.common_prefixes))
}

#[cfg(feature = "csr")]
pub async fn delete_bucket_object(bucket: &str, key: &str) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!(
    "/api/s3/buckets/{}/objects/{}",
    bucket,
    urlencoding::encode(key)
  ))
  .await
}

#[cfg(feature = "csr")]
pub fn get_download_url(bucket: &str, key: &str) -> String {
  let token = get_stored_token().unwrap_or_default();
  format!(
    "/api/s3/buckets/{}/download/{}?token={}",
    bucket,
    urlencoding::encode(key),
    token
  )
}

// =============================================================================
// Proxy Connection Tests
// =============================================================================

#[cfg(feature = "csr")]
pub async fn test_storage_connection(
  endpoint: &str,
  access_key_id: &str,
  secret_access_key: &str,
  region: &str,
  force_path_style: bool,
) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct TestReq {
    endpoint: String,
    access_key_id: String,
    secret_access_key: String,
    region: String,
    force_path_style: bool,
  }
  post_with_auth(
    "/api/s3/test-connection",
    &TestReq {
      endpoint: endpoint.to_string(),
      access_key_id: access_key_id.to_string(),
      secret_access_key: secret_access_key.to_string(),
      region: region.to_string(),
      force_path_style,
    },
  )
  .await
}

#[cfg(feature = "csr")]
pub async fn test_cache_connection(
  host: &str,
  port: u16,
  password: Option<&str>,
  database: u8,
  tls_enabled: bool,
) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct TestReq {
    host: String,
    port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    database: u8,
    tls_enabled: bool,
  }
  post_with_auth(
    "/api/cache/test-connection",
    &TestReq {
      host: host.to_string(),
      port,
      password: password.map(|s| s.to_string()),
      database,
      tls_enabled,
    },
  )
  .await
}

// =============================================================================
// Extended Storage Settings (with proxy support)
// =============================================================================

#[cfg(feature = "csr")]
pub async fn update_storage_settings_extended(
  port: Option<u16>,
  storage_path: Option<String>,
  region: Option<String>,
  mode: Option<String>,
  proxy_endpoint: Option<String>,
  proxy_access_key_id: Option<String>,
  proxy_secret_access_key: Option<String>,
  proxy_region: Option<String>,
  proxy_bucket_prefix: Option<String>,
  proxy_force_path_style: Option<bool>,
) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct UpdateReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_access_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_secret_access_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_bucket_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_force_path_style: Option<bool>,
  }
  put_with_auth(
    "/api/s3/settings",
    &UpdateReq {
      port,
      storage_path,
      region,
      mode,
      proxy_endpoint,
      proxy_access_key_id,
      proxy_secret_access_key,
      proxy_region,
      proxy_bucket_prefix,
      proxy_force_path_style,
    },
  )
  .await
}

// =============================================================================
// Extended Cache Settings (with proxy support)
// =============================================================================

#[cfg(feature = "csr")]
pub async fn update_cache_settings_extended(
  port: Option<u16>,
  max_memory: Option<String>,
  eviction: Option<String>,
  default_ttl: Option<u64>,
  snapshot_enabled: Option<bool>,
  snapshot_path: Option<String>,
  snapshot_interval: Option<u64>,
  mode: Option<String>,
  proxy_host: Option<String>,
  proxy_port: Option<u16>,
  proxy_password: Option<String>,
  proxy_database: Option<u8>,
  proxy_tls_enabled: Option<bool>,
) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct UpdateReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    eviction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_ttl: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_interval: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_database: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_tls_enabled: Option<bool>,
  }
  put_with_auth(
    "/api/cache/settings",
    &UpdateReq {
      port,
      max_memory,
      eviction,
      default_ttl,
      snapshot_enabled,
      snapshot_path,
      snapshot_interval,
      mode,
      proxy_host,
      proxy_port,
      proxy_password,
      proxy_database,
      proxy_tls_enabled,
    },
  )
  .await
}
