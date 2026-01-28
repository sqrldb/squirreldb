//! API client for communicating with the server

#[cfg(feature = "csr")]
use gloo_net::http::{Request, RequestBuilder};
#[cfg(feature = "csr")]
use gloo_storage::{LocalStorage, Storage};
#[cfg(feature = "csr")]
use serde::{de::DeserializeOwned, Serialize};

#[cfg(feature = "csr")]
use crate::admin::state::{BucketInfo, S3Settings, Stats, TableInfo, TokenInfo};

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
async fn post_with_auth<T: Serialize, R: DeserializeOwned>(url: &str, body: &T) -> Result<R, String> {
  let req = add_auth_header(Request::post(url)).json(body).map_err(|e| e.to_string())?;
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
async fn put_with_auth<T: Serialize, R: DeserializeOwned>(url: &str, body: &T) -> Result<R, String> {
  let req = add_auth_header(Request::put(url)).json(body).map_err(|e| e.to_string())?;
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
  Ok(collections.into_iter().map(|c| TableInfo { name: c.name, count: c.count }).collect())
}

#[cfg(feature = "csr")]
pub async fn fetch_s3_settings() -> Result<S3Settings, String> {
  fetch_with_auth("/api/s3/settings").await
}

#[cfg(feature = "csr")]
pub async fn update_s3_settings(port: Option<u16>, storage_path: Option<String>, region: Option<String>) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct UpdateReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<String>,
  }
  put_with_auth("/api/s3/settings", &UpdateReq { port, storage_path, region }).await
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
pub async fn fetch_buckets() -> Result<Vec<BucketInfo>, String> {
  #[derive(serde::Deserialize)]
  struct BucketResp {
    name: String,
    object_count: i64,
    current_size: i64,
  }
  let buckets: Vec<BucketResp> = fetch_with_auth("/api/s3/buckets").await?;
  Ok(buckets.into_iter().map(|b| BucketInfo {
    name: b.name,
    object_count: b.object_count,
    current_size: b.current_size,
  }).collect())
}

#[cfg(feature = "csr")]
pub async fn create_bucket(name: &str) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct CreateReq {
    name: String,
  }
  post_with_auth("/api/s3/buckets", &CreateReq { name: name.to_string() }).await
}

#[cfg(feature = "csr")]
pub async fn delete_bucket(name: &str) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!("/api/s3/buckets/{}", name)).await
}

#[cfg(feature = "csr")]
pub async fn fetch_tokens() -> Result<Vec<TokenInfo>, String> {
  #[derive(serde::Deserialize)]
  struct TokenResp {
    id: String,
    name: String,
    created_at: String,
  }
  let tokens: Vec<TokenResp> = fetch_with_auth("/api/tokens").await?;
  Ok(tokens.into_iter().map(|t| TokenInfo {
    id: t.id,
    name: t.name,
    created_at: t.created_at,
  }).collect())
}

#[cfg(feature = "csr")]
pub async fn create_token(name: &str) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct CreateReq {
    name: String,
  }
  post_with_auth("/api/tokens", &CreateReq { name: name.to_string() }).await
}

#[cfg(feature = "csr")]
pub async fn delete_token(id: &str) -> Result<serde_json::Value, String> {
  delete_with_auth(&format!("/api/tokens/{}", id)).await
}

#[cfg(feature = "csr")]
pub async fn run_query(query: &str) -> Result<serde_json::Value, String> {
  #[derive(Serialize)]
  struct QueryReq {
    query: String,
  }
  post_with_auth("/api/query", &QueryReq { query: query.to_string() }).await
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
  let req = Request::get("/api/settings")
    .header("Authorization", &format!("Bearer {}", token));
  match req.send().await {
    Ok(resp) => resp.ok(),
    Err(_) => false,
  }
}
