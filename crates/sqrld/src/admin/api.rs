use axum::extract::Request;
use axum::{
  body::Body,
  extract::{
    ws::{Message, WebSocket, WebSocketUpgrade},
    Multipart, Path, Query, State,
  },
  http::{header, HeaderMap, StatusCode},
  middleware::Next,
  response::{Html, IntoResponse, Response},
  routing::{delete, get, post, put},
  Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

use super::auth;
use crate::cache::CacheStore;
use crate::db::{AdminRole, AdminUser, ApiTokenInfo, DatabaseBackend, SqlDialect};
use crate::features::{FeatureInfo, FeatureRegistry};
use crate::query::{QueryEngine, QueryEnginePool};
use crate::security::headers::SecurityHeadersLayer;
use crate::server::{MessageHandler, RateLimiter, ServerConfig};
use crate::subscriptions::SubscriptionManager;
use crate::types::{ClientMessage, ServerMessage, DEFAULT_PROJECT_ID};

type Backend = Arc<dyn DatabaseBackend>;
type WsClients = Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<ServerMessage>>>>;

/// Log entry for streaming to clients
#[derive(Clone, Serialize, Debug)]
pub struct LogEntry {
  pub timestamp: String,
  pub level: String,
  pub target: String,
  pub message: String,
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
  pub backend: Backend,
  pub dialect: SqlDialect,
  pub engine: Arc<Mutex<QueryEngine>>,
  pub engine_pool: Arc<QueryEnginePool>,
  pub start_time: std::time::Instant,
  pub subs: Arc<SubscriptionManager>,
  pub ws_clients: WsClients,
  pub config: ServerConfig,
  pub log_tx: broadcast::Sender<LogEntry>,
  pub feature_registry: Arc<FeatureRegistry>,
  pub shutdown_tx: Option<broadcast::Sender<()>>,
  pub rate_limiter: Arc<RateLimiter>,
}

/// Global log broadcaster - initialized once and used throughout the app
static LOG_BROADCASTER: std::sync::OnceLock<broadcast::Sender<LogEntry>> =
  std::sync::OnceLock::new();

/// Get or create the global log broadcaster
pub fn get_log_broadcaster() -> broadcast::Sender<LogEntry> {
  LOG_BROADCASTER
    .get_or_init(|| {
      let (tx, _) = broadcast::channel(1000);
      tx
    })
    .clone()
}

/// Emit a log entry to all connected log viewers
pub fn emit_log(level: &str, target: &str, message: &str) {
  let tx = get_log_broadcaster();
  let entry = LogEntry {
    timestamp: chrono::Utc::now().to_rfc3339(),
    level: level.to_string(),
    target: target.to_string(),
    message: message.to_string(),
  };
  // Ignore send errors (no subscribers)
  let _ = tx.send(entry);
}

/// Admin server with Leptos integration
pub struct AdminServer {
  backend: Backend,
  subs: Arc<SubscriptionManager>,
  engine_pool: Arc<QueryEnginePool>,
  shutdown_rx: broadcast::Receiver<()>,
  shutdown_tx: broadcast::Sender<()>,
  config: ServerConfig,
  feature_registry: Arc<FeatureRegistry>,
  rate_limiter: Arc<RateLimiter>,
}

impl AdminServer {
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    backend: Backend,
    subs: Arc<SubscriptionManager>,
    engine_pool: Arc<QueryEnginePool>,
    shutdown_rx: broadcast::Receiver<()>,
    shutdown_tx: broadcast::Sender<()>,
    config: ServerConfig,
    feature_registry: Arc<FeatureRegistry>,
    rate_limiter: Arc<RateLimiter>,
  ) -> Self {
    Self {
      backend,
      subs,
      engine_pool,
      shutdown_rx,
      shutdown_tx,
      config,
      feature_registry,
      rate_limiter,
    }
  }

  pub async fn run(mut self, addr: &str) -> Result<(), anyhow::Error> {
    let dialect = self.backend.dialect();
    let ws_clients: WsClients = Arc::new(RwLock::new(HashMap::new()));
    let log_tx = get_log_broadcaster();

    let state = AppState {
      dialect,
      engine: Arc::new(Mutex::new(QueryEngine::new(dialect))),
      engine_pool: self.engine_pool,
      backend: self.backend,
      start_time: std::time::Instant::now(),
      subs: self.subs.clone(),
      ws_clients: ws_clients.clone(),
      config: self.config.clone(),
      log_tx,
      feature_registry: self.feature_registry.clone(),
      shutdown_tx: Some(self.shutdown_tx.clone()),
      rate_limiter: self.rate_limiter.clone(),
    };

    // Spawn task to forward subscription changes to WebSocket clients
    let subs = self.subs.clone();
    let clients = ws_clients.clone();
    tokio::spawn(async move {
      let mut rx = subs.subscribe_to_outgoing();
      while let Ok((client_id, msg)) = rx.recv().await {
        if let Some(tx) = clients.read().await.get(&client_id) {
          let _ = tx.send(msg);
        }
      }
    });

    // Build router with conditional protocol support
    let mut app = Router::new()
            // Health endpoints (no /api prefix for k8s probes) - always public
            .route("/health", get(health_check))
            .route("/ready", get(readiness_check))
            // Static assets - always public
            .route("/style.css", get(serve_css))
            // Auth pages - always public
            .route("/setup", get(serve_setup_page))
            .route("/login", get(serve_login_page))
            // Setup API - only works when no tokens exist
            .route("/api/setup", post(api_setup_token))
            // User authentication endpoints - public
            .route("/api/auth/status", get(api_auth_status))
            .route("/api/auth/setup", post(api_auth_setup))
            .route("/api/auth/login", post(api_auth_login))
            .route("/api/auth/logout", post(api_auth_logout))
            .route("/api/auth/change-password", post(api_auth_change_password));

    // Admin API routes (protected by admin auth)
    let admin_routes = Router::new()
      .route("/api/settings", get(api_get_settings))
      .route("/api/settings", put(api_update_settings))
      .route("/api/projects/{project_id}/tokens", get(api_list_tokens))
      .route("/api/projects/{project_id}/tokens", post(api_create_token))
      .route("/api/projects/{project_id}/tokens/{id}", delete(api_delete_token))
      // Feature management
      .route("/api/features", get(api_list_features))
      .route("/api/features/{name}", put(api_toggle_feature))
      // Auth settings
      .route(
        "/api/settings/auth",
        get(api_get_auth_settings).put(api_update_auth_settings),
      )
      // Protocol settings
      .route(
        "/api/settings/protocols",
        get(api_get_protocol_settings).put(api_update_protocol_settings),
      )
      // Server control
      .route("/api/server/restart", post(api_restart_server))
      .route("/api/server/health", get(api_health_check))
      // CORS settings
      .route(
        "/api/settings/cors",
        get(api_get_cors_settings).put(api_update_cors_settings),
      )
      // S3 management
      .route(
        "/api/s3/settings",
        get(api_get_storage_settings).put(api_update_storage_settings),
      )
      .route("/api/s3/buckets", get(api_list_storage_buckets))
      .route("/api/s3/buckets", post(api_create_storage_bucket))
      .route("/api/s3/buckets/{name}", delete(api_delete_storage_bucket))
      .route("/api/s3/buckets/{name}/stats", get(api_get_storage_bucket_stats))
      .route("/api/s3/keys", get(api_list_s3_keys))
      .route("/api/s3/keys", post(api_create_s3_key))
      .route("/api/s3/keys/{id}", delete(api_delete_s3_key))
      // Browser API
      .route("/api/s3/buckets/{bucket}/objects", get(api_list_bucket_objects))
      .route("/api/s3/buckets/{bucket}/objects/{*key}", delete(api_delete_bucket_object))
      .route("/api/s3/buckets/{bucket}/download/{*key}", get(api_download_object))
      .route("/api/s3/buckets/{bucket}/upload", post(api_upload_object))
      // Proxy test endpoints
      .route("/api/s3/test-connection", post(api_test_storage_connection))
      .route("/api/cache/test-connection", post(api_test_cache_connection))
      // Cache management
      .route(
        "/api/cache/settings",
        get(api_get_cache_settings).put(api_update_cache_settings),
      )
      .route("/api/cache/stats", get(api_get_cache_stats))
      .route("/api/cache/flush", post(api_flush_cache))
      // Backup management
      .route(
        "/api/backup/settings",
        get(api_get_backup_settings).put(api_update_backup_settings),
      )
      .route("/api/backup/list", get(api_list_backups))
      .route("/api/backup/create", post(api_create_backup))
      .route("/api/backup/{id}", delete(api_delete_backup))
      // User management (owner only)
      .route("/api/users", get(api_list_users))
      .route("/api/users", post(api_create_user))
      .route("/api/users/{id}", delete(api_delete_user))
      .route("/api/users/{id}/role", put(api_update_user_role))
      // Project management
      .route("/api/projects", get(api_list_projects))
      .route("/api/projects", post(api_create_project))
      .route("/api/projects/{id}", get(api_get_project))
      .route("/api/projects/{id}", put(api_update_project))
      .route("/api/projects/{id}", delete(api_delete_project))
      .route("/api/projects/{id}/members", get(api_list_project_members))
      .route("/api/projects/{id}/members", post(api_add_project_member))
      .route(
        "/api/projects/{id}/members/{user_id}",
        put(api_update_project_member_role),
      )
      .route(
        "/api/projects/{id}/members/{user_id}",
        delete(api_remove_project_member),
      )
      .route("/api/projects/{id}/select", post(api_select_project))
      .layer(axum::middleware::from_fn_with_state(
        state.clone(),
        admin_auth_middleware,
      ))
      .layer(axum::middleware::from_fn_with_state(
        state.clone(),
        rate_limit_middleware,
      ));
    app = app.merge(admin_routes);

    // REST API routes (conditional, public - no auth required, but rate limited)
    if self.config.server.protocols.rest {
      let rest_routes = Router::new()
        .route("/api/status", get(api_status))
        .route("/api/collections", get(api_collections))
        .route("/api/collections/{name}", get(api_collection_docs))
        .route("/api/collections/{name}", delete(api_drop_collection))
        .route("/api/collections/{name}/documents", post(api_insert_doc))
        .route("/api/collections/{name}/documents/{id}", get(api_get_doc))
        .route(
          "/api/collections/{name}/documents/{id}",
          put(api_update_doc),
        )
        .route(
          "/api/collections/{name}/documents/{id}",
          delete(api_delete_doc),
        )
        .route("/api/query", post(api_query))
        .layer(axum::middleware::from_fn_with_state(
          state.clone(),
          rate_limit_middleware,
        ));
      app = app.merge(rest_routes);
    }

    // WebSocket endpoint (conditional, public - no auth required)
    if self.config.server.protocols.websocket {
      app = app.route("/ws", get(ws_handler));
    }

    // Log streaming WebSocket (admin only, protected by auth)
    app = app.route("/ws/logs", get(ws_logs_handler));

    // Build CORS layer based on config
    let cors = if self.config.server.cors_origins.is_empty()
      || self.config.server.cors_origins.iter().any(|o| o == "*")
    {
      CorsLayer::permissive()
    } else {
      let origins: Vec<_> = self
        .config
        .server
        .cors_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
      CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(Any)
        .allow_headers(Any)
    };

    // Serve WASM bundle from target/admin, fallback to index.html for SPA routing
    let app = app
      .fallback_service(
        ServeDir::new("target/admin").not_found_service(ServeFile::new("target/admin/index.html")),
      )
      .layer(SecurityHeadersLayer)
      .layer(cors)
      .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Admin UI at http://{}", addr);

    axum::serve(listener, app.into_make_service())
      .with_graceful_shutdown(async move {
        let _ = self.shutdown_rx.recv().await;
        tracing::info!("Admin server shutting down");
      })
      .await?;
    Ok(())
  }
}

/// Serve CSS stylesheet
async fn serve_css() -> impl IntoResponse {
  (
    [(header::CONTENT_TYPE, "text/css")],
    include_str!("styles.css"),
  )
}

/// Check if first-time setup is needed (auth enabled but no tokens exist)
async fn needs_setup(state: &AppState) -> bool {
  if !state.config.auth.enabled {
    return false;
  }
  // If admin_token is configured, no setup needed
  if let Some(ref admin_token) = state.config.auth.admin_token {
    if !admin_token.is_empty() {
      return false;
    }
  }
  // Check if any admin users exist
  match state.backend.list_admin_users().await {
    Ok(users) => users.is_empty(),
    Err(_) => false, // On error, assume setup not needed
  }
}

/// Serve the setup page for first-time admin configuration
async fn serve_setup_page(State(state): State<AppState>) -> Response {
  // Only allow setup if no tokens exist
  let setup_needed = needs_setup(&state).await;
  if !setup_needed {
    return axum::response::Redirect::to("/login").into_response();
  }

  Html(
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SquirrelDB Setup</title>
    <link rel="stylesheet" href="/style.css">
    <style>
        .setup-container {
            max-width: 480px;
            margin: 100px auto;
            padding: 40px;
            background: var(--bg-secondary);
            border-radius: 12px;
            box-shadow: 0 4px 20px rgba(0,0,0,0.1);
        }
        .setup-container h1 {
            margin: 0 0 8px 0;
            font-size: 24px;
            color: var(--text-primary);
        }
        .setup-container p {
            margin: 0 0 24px 0;
            color: var(--text-secondary);
        }
        .setup-form input {
            width: 100%;
            padding: 12px;
            border: 1px solid var(--border-color);
            border-radius: 6px;
            font-size: 14px;
            margin-bottom: 16px;
            background: var(--bg-primary);
            color: var(--text-primary);
            box-sizing: border-box;
        }
        .setup-form button {
            width: 100%;
            padding: 12px;
            background: var(--accent-color);
            color: white;
            border: none;
            border-radius: 6px;
            font-size: 14px;
            font-weight: 500;
            cursor: pointer;
        }
        .setup-form button:hover {
            opacity: 0.9;
        }
        .token-display {
            background: var(--bg-tertiary);
            padding: 16px;
            border-radius: 6px;
            margin: 16px 0;
            word-break: break-all;
            font-family: monospace;
            font-size: 13px;
        }
        .warning {
            background: #fef3c7;
            border: 1px solid #f59e0b;
            color: #92400e;
            padding: 12px;
            border-radius: 6px;
            margin-bottom: 16px;
            font-size: 13px;
        }
        .success {
            background: #d1fae5;
            border: 1px solid #10b981;
            color: #065f46;
            padding: 12px;
            border-radius: 6px;
            margin-bottom: 16px;
        }
        .hidden { display: none; }
    </style>
</head>
<body>
    <div class="setup-container">
        <h1>Welcome to SquirrelDB</h1>
        <p>Create your first admin token to secure the admin panel.</p>

        <div id="setup-form-section">
            <div class="warning">
                <strong>Important:</strong> Save your token securely after creation.
                You won't be able to see it again.
            </div>
            <form class="setup-form" id="setup-form">
                <input type="text" id="token-name" placeholder="Token name (e.g., Admin)" required>
                <button type="submit">Create Admin Token</button>
            </form>
        </div>

        <div id="success-section" class="hidden">
            <div class="success">
                <strong>Success!</strong> Your admin token has been created.
            </div>
            <p><strong>Your token:</strong></p>
            <div class="token-display" id="token-value"></div>
            <p style="color: var(--text-secondary); font-size: 13px;">
                Copy this token and store it securely. You'll need it to log in.
            </p>
            <form class="setup-form">
                <button type="button" onclick="window.location.href='/login'">
                    Continue to Login
                </button>
            </form>
        </div>
    </div>
    <script>
        document.getElementById('setup-form').addEventListener('submit', async (e) => {
            e.preventDefault();
            const name = document.getElementById('token-name').value;

            try {
                const resp = await fetch('/api/setup', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ name })
                });

                if (!resp.ok) {
                    const err = await resp.json();
                    alert('Error: ' + (err.error || 'Failed to create token'));
                    return;
                }

                const data = await resp.json();
                document.getElementById('token-value').textContent = data.token;
                document.getElementById('setup-form-section').classList.add('hidden');
                document.getElementById('success-section').classList.remove('hidden');
            } catch (err) {
                alert('Error: ' + err.message);
            }
        });
    </script>
</body>
</html>"#,
  )
  .into_response()
}

/// Serve the login page
async fn serve_login_page(State(state): State<AppState>) -> Response {
  // If setup is needed, redirect to setup
  if needs_setup(&state).await {
    return axum::response::Redirect::to("/setup").into_response();
  }

  // If auth is disabled, redirect to admin
  if !state.config.auth.enabled {
    return axum::response::Redirect::to("/").into_response();
  }

  Html(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SquirrelDB Login</title>
    <link rel="stylesheet" href="/style.css">
    <style>
        .login-container {
            max-width: 400px;
            margin: 100px auto;
            padding: 40px;
            background: var(--bg-secondary);
            border-radius: 12px;
            box-shadow: 0 4px 20px rgba(0,0,0,0.1);
        }
        .login-container h1 {
            margin: 0 0 8px 0;
            font-size: 24px;
            color: var(--text-primary);
            display: flex;
            align-items: center;
            gap: 12px;
        }
        .login-container h1 svg {
            width: 32px;
            height: 32px;
        }
        .login-container p {
            margin: 0 0 24px 0;
            color: var(--text-secondary);
        }
        .login-form input {
            width: 100%;
            padding: 12px;
            border: 1px solid var(--border-color);
            border-radius: 6px;
            font-size: 14px;
            margin-bottom: 16px;
            background: var(--bg-primary);
            color: var(--text-primary);
            box-sizing: border-box;
        }
        .login-form button {
            width: 100%;
            padding: 12px;
            background: var(--accent-color);
            color: white;
            border: none;
            border-radius: 6px;
            font-size: 14px;
            font-weight: 500;
            cursor: pointer;
        }
        .login-form button:hover {
            opacity: 0.9;
        }
        .error {
            background: #fee2e2;
            border: 1px solid #ef4444;
            color: #991b1b;
            padding: 12px;
            border-radius: 6px;
            margin-bottom: 16px;
            font-size: 13px;
        }
        .hidden { display: none; }
    </style>
</head>
<body>
    <div class="login-container">
        <h1>
            <svg viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/>
            </svg>
            SquirrelDB
        </h1>
        <p>Enter your admin token to access the dashboard.</p>

        <div id="error-message" class="error hidden"></div>

        <form class="login-form" id="login-form">
            <input type="password" id="token-input" placeholder="Admin token (sqrl_...)" required>
            <button type="submit">Sign In</button>
        </form>
    </div>
    <script>
        // Check if already authenticated
        const savedToken = localStorage.getItem('sqrl_admin_token');
        if (savedToken) {
            // Validate token
            fetch('/api/settings', {
                headers: { 'Authorization': 'Bearer ' + savedToken }
            }).then(resp => {
                if (resp.ok) {
                    window.location.href = '/';
                }
            });
        }

        document.getElementById('login-form').addEventListener('submit', async (e) => {
            e.preventDefault();
            const token = document.getElementById('token-input').value.trim();
            const errorEl = document.getElementById('error-message');

            try {
                const resp = await fetch('/api/settings', {
                    headers: { 'Authorization': 'Bearer ' + token }
                });

                if (resp.ok) {
                    localStorage.setItem('sqrl_admin_token', token);
                    window.location.href = '/';
                } else {
                    errorEl.textContent = 'Invalid token. Please try again.';
                    errorEl.classList.remove('hidden');
                }
            } catch (err) {
                errorEl.textContent = 'Connection error. Please try again.';
                errorEl.classList.remove('hidden');
            }
        });
    </script>
</body>
</html>"#).into_response()
}

#[derive(Serialize)]
struct StatusResponse {
  name: &'static str,
  version: &'static str,
  backend: String,
  uptime_secs: u64,
}

async fn api_status(State(state): State<AppState>) -> Json<StatusResponse> {
  Json(StatusResponse {
    name: "SquirrelDB",
    version: env!("CARGO_PKG_VERSION"),
    backend: format!("{:?}", state.dialect),
    uptime_secs: state.start_time.elapsed().as_secs(),
  })
}

/// Liveness probe - returns 200 if server is running
async fn health_check() -> StatusCode {
  StatusCode::OK
}

/// Readiness probe - returns 200 if database is accessible
async fn readiness_check(State(state): State<AppState>) -> StatusCode {
  match state.backend.list_collections(DEFAULT_PROJECT_ID).await {
    Ok(_) => StatusCode::OK,
    Err(_) => StatusCode::SERVICE_UNAVAILABLE,
  }
}

async fn api_collections(
  State(state): State<AppState>,
) -> Result<Json<Vec<CollectionInfo>>, AppError> {
  let names = state.backend.list_collections(DEFAULT_PROJECT_ID).await?;
  let mut collections = Vec::with_capacity(names.len());
  for name in names {
    let docs = state
      .backend
      .list(DEFAULT_PROJECT_ID, &name, None, None, None, None)
      .await?;
    collections.push(CollectionInfo {
      name,
      count: docs.len(),
    });
  }
  Ok(Json(collections))
}

#[derive(Serialize)]
struct CollectionInfo {
  name: String,
  count: usize,
}

#[derive(Deserialize)]
struct ListQuery {
  limit: Option<usize>,
  offset: Option<usize>,
}

async fn api_collection_docs(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
  // Use database-level pagination for better performance
  let docs = state
    .backend
    .list(DEFAULT_PROJECT_ID, &name, None, None, q.limit, q.offset)
    .await?;
  Ok(Json(serde_json::to_value(docs)?))
}

async fn api_drop_collection(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
  let docs = state
    .backend
    .list(DEFAULT_PROJECT_ID, &name, None, None, None, None)
    .await?;
  let mut deleted = 0;
  for doc in docs {
    state
      .backend
      .delete(DEFAULT_PROJECT_ID, &name, doc.id)
      .await?;
    deleted += 1;
  }
  Ok(Json(serde_json::json!({ "deleted": deleted })))
}

async fn api_insert_doc(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Json(data): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
  let doc = state
    .backend
    .insert(DEFAULT_PROJECT_ID, &name, data)
    .await?;
  emit_log(
    "info",
    "squirreldb::api",
    &format!("Document inserted in '{}': {}", name, doc.id),
  );
  Ok(Json(serde_json::to_value(doc)?))
}

async fn api_get_doc(
  State(state): State<AppState>,
  Path((name, id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
  let id = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid UUID".into()))?;
  let doc = state.backend.get(DEFAULT_PROJECT_ID, &name, id).await?;
  match doc {
    Some(d) => Ok(Json(serde_json::to_value(d)?)),
    None => Err(AppError::NotFound("Not found".to_string())),
  }
}

async fn api_update_doc(
  State(state): State<AppState>,
  Path((name, id)): Path<(String, String)>,
  Json(data): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
  let id = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid UUID".into()))?;
  let doc = state
    .backend
    .update(DEFAULT_PROJECT_ID, &name, id, data)
    .await?;
  match doc {
    Some(d) => Ok(Json(serde_json::to_value(d)?)),
    None => Err(AppError::NotFound("Not found".to_string())),
  }
}

async fn api_delete_doc(
  State(state): State<AppState>,
  Path((name, id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
  let id = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid UUID".into()))?;
  let doc = state.backend.delete(DEFAULT_PROJECT_ID, &name, id).await?;
  match doc {
    Some(d) => {
      emit_log(
        "info",
        "squirreldb::api",
        &format!("Document deleted from '{}': {}", name, id),
      );
      Ok(Json(serde_json::to_value(d)?))
    }
    None => Err(AppError::NotFound("Not found".to_string())),
  }
}

#[derive(Deserialize)]
struct QueryRequest {
  query: String,
}

async fn api_query(
  State(state): State<AppState>,
  Json(req): Json<QueryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  emit_log(
    "debug",
    "squirreldb::query",
    &format!("Executing query: {}", req.query),
  );

  // Parse query while holding lock, then execute without lock
  let spec = {
    let engine = state.engine.lock();
    engine.parse_query(&req.query)?
  };

  let sql_filter = spec.filter.as_ref().and_then(|f| f.compiled_sql.as_deref());
  let project_id = spec.project_id.unwrap_or(DEFAULT_PROJECT_ID);
  let docs = state
    .backend
    .list(
      project_id,
      &spec.table,
      sql_filter,
      spec.order_by.as_ref(),
      spec.limit,
      spec.offset,
    )
    .await?;

  emit_log(
    "info",
    "squirreldb::query",
    &format!("Query on '{}' returned {} results", spec.table, docs.len()),
  );
  Ok(Json(serde_json::to_value(&docs)?))
}

// =============================================================================
// Auth Middleware
// =============================================================================

/// Hash a token using SHA-256
fn hash_token(token: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(token.as_bytes());
  format!("{:x}", hasher.finalize())
}

/// Generate a new API token
fn generate_token() -> String {
  const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
  let mut rng = rand::thread_rng();
  let token: String = (0..32)
    .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
    .collect();
  format!("sqrl_{}", token)
}

/// Extract token from request (Authorization header or query param)
fn extract_token(req: &Request) -> Option<String> {
  extract_token_from_headers(req.headers()).or_else(|| extract_token_from_query(req.uri().query()))
}

/// Extract token from headers only
fn extract_token_from_headers(headers: &HeaderMap) -> Option<String> {
  headers
    .get("Authorization")
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.strip_prefix("Bearer "))
    .map(|s| s.to_string())
}

/// Extract token from query string
fn extract_token_from_query(query: Option<&str>) -> Option<String> {
  query.and_then(|q| {
    for pair in q.split('&') {
      if let Some(token) = pair.strip_prefix("token=") {
        return Some(token.to_string());
      }
    }
    None
  })
}

/// Auth middleware for admin UI routes
/// Allows access if: auth disabled, valid session, admin_token matches, or valid API token
async fn admin_auth_middleware(
  State(state): State<AppState>,
  req: Request,
  next: Next,
) -> Response {
  // Skip auth if disabled
  if !state.config.auth.enabled {
    return next.run(req).await;
  }

  // Extract token
  let token = extract_token(&req);

  match token {
    Some(t) => {
      // Check if it's a session token (starts with "session_")
      if let Some(session_token) = t.strip_prefix("session_") {
        let session_hash = auth::hash_session_token(session_token);
        if let Ok(Some(_)) = state.backend.validate_admin_session(&session_hash).await {
          return next.run(req).await;
        }
      }

      // Check if it matches admin_token (if configured)
      // Uses constant-time comparison to prevent timing attacks
      if let Some(ref admin_token) = state.config.auth.admin_token {
        if !admin_token.is_empty() && crate::security::constant_time_compare(&t, admin_token) {
          return next.run(req).await;
        }
      }

      // Otherwise validate as API token
      let token_hash = hash_token(&t);
      match state.backend.validate_token(&token_hash).await {
        Ok(Some(_project_id)) => next.run(req).await,
        _ => (
          StatusCode::UNAUTHORIZED,
          Json(serde_json::json!({"error": "Invalid token"})),
        )
          .into_response(),
      }
    }
    None => (
      StatusCode::UNAUTHORIZED,
      Json(serde_json::json!({"error": "Authentication required"})),
    )
      .into_response(),
  }
}

/// Rate limiting middleware for admin API routes
/// Extracts client IP and checks against the rate limiter
async fn rate_limit_middleware(
  State(state): State<AppState>,
  req: Request,
  next: Next,
) -> Response {
  // Extract client IP from headers (X-Forwarded-For, X-Real-IP) or socket
  let ip = extract_client_ip(&req);

  // Check rate limit
  if let Err(e) = state.rate_limiter.check_request(ip) {
    return (
      StatusCode::TOO_MANY_REQUESTS,
      [(header::RETRY_AFTER, "1")],
      Json(serde_json::json!({
        "error": "Rate limit exceeded",
        "message": e.to_string()
      })),
    )
      .into_response();
  }

  next.run(req).await
}

/// Extract client IP from request headers or connection info
fn extract_client_ip(req: &Request) -> std::net::IpAddr {
  // Try X-Forwarded-For first (common for proxies/load balancers)
  if let Some(forwarded) = req.headers().get("X-Forwarded-For") {
    if let Ok(s) = forwarded.to_str() {
      // Take the first IP in the chain (original client)
      if let Some(ip_str) = s.split(',').next() {
        if let Ok(ip) = ip_str.trim().parse() {
          return ip;
        }
      }
    }
  }

  // Try X-Real-IP (nginx)
  if let Some(real_ip) = req.headers().get("X-Real-IP") {
    if let Ok(s) = real_ip.to_str() {
      if let Ok(ip) = s.parse() {
        return ip;
      }
    }
  }

  // Fallback to localhost if no IP can be extracted
  // In production, you should ensure connection info is available
  std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
}

// =============================================================================
// User Authentication API
// =============================================================================

/// Auth status response
#[derive(Serialize)]
struct AuthStatusResponse {
  needs_setup: bool,
  logged_in: bool,
  user: Option<AdminUserResponse>,
}

#[derive(Serialize)]
struct AdminUserResponse {
  id: String,
  username: String,
  email: Option<String>,
  role: String,
  created_at: String,
}

impl From<AdminUser> for AdminUserResponse {
  fn from(u: AdminUser) -> Self {
    Self {
      id: u.id.to_string(),
      username: u.username,
      email: u.email,
      role: u.role.to_string(),
      created_at: u.created_at.to_rfc3339(),
    }
  }
}

/// GET /api/auth/status - Check if setup is needed or if logged in
async fn api_auth_status(
  State(state): State<AppState>,
  headers: HeaderMap,
) -> Result<Json<AuthStatusResponse>, AppError> {
  // Check if any admin users exist
  let has_users = state.backend.has_admin_users().await?;

  if !has_users {
    return Ok(Json(AuthStatusResponse {
      needs_setup: true,
      logged_in: false,
      user: None,
    }));
  }

  // Check if user is logged in via session
  if let Some(token) = extract_token_from_headers(&headers) {
    if let Some(session_token) = token.strip_prefix("session_") {
      let session_hash = auth::hash_session_token(session_token);
      if let Ok(Some((_, user))) = state.backend.validate_admin_session(&session_hash).await {
        return Ok(Json(AuthStatusResponse {
          needs_setup: false,
          logged_in: true,
          user: Some(user.into()),
        }));
      }
    }
  }

  Ok(Json(AuthStatusResponse {
    needs_setup: false,
    logged_in: false,
    user: None,
  }))
}

#[derive(Deserialize)]
struct SetupRequest {
  username: String,
  email: Option<String>,
  password: String,
}

#[derive(Serialize)]
struct LoginResponse {
  token: String,
  user: AdminUserResponse,
}

/// POST /api/auth/setup - Create the first owner user
async fn api_auth_setup(
  State(state): State<AppState>,
  Json(req): Json<SetupRequest>,
) -> Result<Json<LoginResponse>, AppError> {
  // Check if setup is already done
  if state.backend.has_admin_users().await? {
    return Err(AppError::BadRequest(
      "Setup already completed. Use login instead.".to_string(),
    ));
  }

  // Validate input
  if req.username.trim().is_empty() {
    return Err(AppError::BadRequest("Username is required".to_string()));
  }
  if req.password.len() < 8 {
    return Err(AppError::BadRequest(
      "Password must be at least 8 characters".to_string(),
    ));
  }

  // Hash password
  let password_hash = auth::hash_password(&req.password)
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Password hash error: {}", e)))?;

  // Create owner user
  let user = state
    .backend
    .create_admin_user(
      &req.username.trim().to_lowercase(),
      req.email.as_deref(),
      &password_hash,
      AdminRole::Owner,
    )
    .await?;

  // Create session
  let session_token = auth::generate_session_token();
  let session_hash = auth::hash_session_token(&session_token);
  let expires_at = chrono::Utc::now() + chrono::Duration::days(30);
  state
    .backend
    .create_admin_session(user.id, &session_hash, expires_at)
    .await?;

  Ok(Json(LoginResponse {
    token: format!("session_{}", session_token),
    user: user.into(),
  }))
}

#[derive(Deserialize)]
struct LoginRequest {
  username: String,
  password: String,
}

/// POST /api/auth/login - Login with username/password
async fn api_auth_login(
  State(state): State<AppState>,
  Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
  // Find user
  let (user, password_hash) = state
    .backend
    .get_admin_user_by_username(&req.username.trim().to_lowercase())
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid credentials".to_string()))?;

  // Verify password
  if !auth::verify_password(&req.password, &password_hash) {
    return Err(AppError::Unauthorized("Invalid credentials".to_string()));
  }

  // Create session
  let session_token = auth::generate_session_token();
  let session_hash = auth::hash_session_token(&session_token);
  let expires_at = chrono::Utc::now() + chrono::Duration::days(30);
  state
    .backend
    .create_admin_session(user.id, &session_hash, expires_at)
    .await?;

  Ok(Json(LoginResponse {
    token: format!("session_{}", session_token),
    user: user.into(),
  }))
}

/// POST /api/auth/logout - Logout (invalidate session)
async fn api_auth_logout(
  State(state): State<AppState>,
  headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
  if let Some(token) = extract_token_from_headers(&headers) {
    if let Some(session_token) = token.strip_prefix("session_") {
      let session_hash = auth::hash_session_token(session_token);
      if let Ok(Some((session, _))) = state.backend.validate_admin_session(&session_hash).await {
        state.backend.delete_admin_session(session.id).await?;
      }
    }
  }
  Ok(Json(serde_json::json!({"message": "Logged out"})))
}

#[derive(Deserialize)]
struct ChangePasswordRequest {
  current_password: String,
  new_password: String,
}

/// POST /api/auth/change-password - Change current user's password
async fn api_auth_change_password(
  State(state): State<AppState>,
  headers: HeaderMap,
  Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  // Get current user from session
  let token = extract_token_from_headers(&headers)
    .ok_or_else(|| AppError::Unauthorized("Not logged in".to_string()))?;
  let session_token = token
    .strip_prefix("session_")
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  let session_hash = auth::hash_session_token(session_token);
  let (_, user) = state
    .backend
    .validate_admin_session(&session_hash)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;

  // Get user with password hash for verification
  let (_, password_hash) = state
    .backend
    .get_admin_user_by_username(&user.username)
    .await?
    .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;

  // Verify current password
  if !auth::verify_password(&req.current_password, &password_hash) {
    return Err(AppError::Unauthorized(
      "Current password is incorrect".to_string(),
    ));
  }

  // Validate new password
  if req.new_password.len() < 8 {
    return Err(AppError::BadRequest(
      "New password must be at least 8 characters".to_string(),
    ));
  }

  // Hash and update password
  let new_hash = auth::hash_password(&req.new_password)
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Password hash error: {}", e)))?;
  state
    .backend
    .update_admin_user_password(&user.id, &new_hash)
    .await?;

  Ok(Json(
    serde_json::json!({"message": "Password changed successfully"}),
  ))
}

// =============================================================================
// User Management API (owner only)
// =============================================================================

/// Helper to check if current user is owner
async fn require_owner(state: &AppState, headers: &HeaderMap) -> Result<AdminUser, AppError> {
  let token = extract_token_from_headers(headers)
    .ok_or_else(|| AppError::Unauthorized("Not logged in".to_string()))?;
  let session_token = token
    .strip_prefix("session_")
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  let session_hash = auth::hash_session_token(session_token);
  let (_, user) = state
    .backend
    .validate_admin_session(&session_hash)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  if user.role != AdminRole::Owner {
    return Err(AppError::Forbidden("Owner access required".to_string()));
  }
  Ok(user)
}

/// GET /api/users - List all admin users (owner only)
async fn api_list_users(
  State(state): State<AppState>,
  headers: HeaderMap,
) -> Result<Json<Vec<AdminUserResponse>>, AppError> {
  require_owner(&state, &headers).await?;
  let users = state.backend.list_admin_users().await?;
  Ok(Json(users.into_iter().map(|u| u.into()).collect()))
}

#[derive(Deserialize)]
struct CreateUserRequest {
  username: String,
  email: Option<String>,
  password: String,
  role: String,
}

/// POST /api/users - Create a new admin user (owner only)
async fn api_create_user(
  State(state): State<AppState>,
  headers: HeaderMap,
  Json(body): Json<CreateUserRequest>,
) -> Result<Json<AdminUserResponse>, AppError> {
  require_owner(&state, &headers).await?;

  // Validate
  if body.username.trim().is_empty() {
    return Err(AppError::BadRequest("Username is required".to_string()));
  }
  if body.password.len() < 8 {
    return Err(AppError::BadRequest(
      "Password must be at least 8 characters".to_string(),
    ));
  }
  let role: AdminRole = body
    .role
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid role".to_string()))?;

  // Hash password
  let password_hash = auth::hash_password(&body.password)
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Password hash error: {}", e)))?;

  // Create user
  let user = state
    .backend
    .create_admin_user(
      &body.username.trim().to_lowercase(),
      body.email.as_deref(),
      &password_hash,
      role,
    )
    .await?;

  Ok(Json(user.into()))
}

/// DELETE /api/users/:id - Delete an admin user (owner only)
async fn api_delete_user(
  State(state): State<AppState>,
  headers: HeaderMap,
  Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
  let current_user = require_owner(&state, &headers).await?;
  let user_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid user ID".to_string()))?;

  // Prevent self-deletion
  if user_id == current_user.id {
    return Err(AppError::BadRequest("Cannot delete yourself".to_string()));
  }

  let deleted = state.backend.delete_admin_user(user_id).await?;
  if !deleted {
    return Err(AppError::NotFound("User not found".to_string()));
  }

  // Also delete all sessions for this user
  state
    .backend
    .delete_admin_sessions_for_user(user_id)
    .await?;

  Ok(Json(serde_json::json!({"deleted": true})))
}

#[derive(Deserialize)]
struct UpdateRoleRequest {
  role: String,
}

/// PUT /api/users/:id/role - Update user role (owner only)
async fn api_update_user_role(
  State(state): State<AppState>,
  headers: HeaderMap,
  Path(id): Path<String>,
  Json(body): Json<UpdateRoleRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  let current_user = require_owner(&state, &headers).await?;
  let user_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid user ID".to_string()))?;

  // Prevent changing own role
  if user_id == current_user.id {
    return Err(AppError::BadRequest(
      "Cannot change your own role".to_string(),
    ));
  }

  let role: AdminRole = body
    .role
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid role".to_string()))?;

  let updated = state.backend.update_admin_user_role(user_id, role).await?;
  if !updated {
    return Err(AppError::NotFound("User not found".to_string()));
  }

  Ok(Json(serde_json::json!({"updated": true})))
}

// =============================================================================
// Settings API
// =============================================================================

#[derive(Serialize)]
struct SettingsResponse {
  protocols: ProtocolsResponse,
  auth: AuthResponse,
}

#[derive(Serialize)]
struct ProtocolsResponse {
  rest: bool,
  websocket: bool,
  sse: bool,
}

#[derive(Serialize)]
struct AuthResponse {
  enabled: bool,
}

async fn api_get_settings(State(state): State<AppState>) -> Json<SettingsResponse> {
  Json(SettingsResponse {
    protocols: ProtocolsResponse {
      rest: state.config.server.protocols.rest,
      websocket: state.config.server.protocols.websocket,
      sse: state.config.server.protocols.sse,
    },
    auth: AuthResponse {
      enabled: state.config.auth.enabled,
    },
  })
}

#[derive(Deserialize)]
#[allow(dead_code)] // Fields used for future runtime config updates
struct UpdateSettingsRequest {
  auth_enabled: Option<bool>,
}

async fn api_update_settings(
  State(_state): State<AppState>,
  Json(_req): Json<UpdateSettingsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  // Note: Changing settings at runtime requires a restart
  // This endpoint is a placeholder for future runtime config support
  Ok(Json(serde_json::json!({
      "message": "Settings updated. Restart required for changes to take effect."
  })))
}

// =============================================================================
// Token Management API
// =============================================================================

async fn api_list_tokens(
  State(state): State<AppState>,
  Path(project_id): Path<String>,
) -> Result<Json<Vec<ApiTokenInfo>>, AppError> {
  let project_id: Uuid = project_id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;
  let tokens = state.backend.list_tokens(project_id).await?;
  Ok(Json(tokens))
}

#[derive(Deserialize)]
struct CreateTokenRequest {
  name: String,
}

#[derive(Serialize)]
struct CreateTokenResponse {
  token: String,
  info: ApiTokenInfo,
}

/// Setup endpoint - creates the first admin token during first-time setup
/// Only works when no tokens exist and auth is enabled
async fn api_setup_token(
  State(state): State<AppState>,
  Json(req): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, AppError> {
  // Verify setup is actually needed
  if !needs_setup(&state).await {
    return Err(AppError::BadRequest(
      "Setup already completed. Use /api/projects/{project_id}/tokens to create additional tokens."
        .into(),
    ));
  }

  if req.name.is_empty() {
    return Err(AppError::BadRequest("Token name is required".into()));
  }

  // Generate new token
  let token = generate_token();
  let token_hash = hash_token(&token);

  // Store in database (default project)
  let info = state
    .backend
    .create_token(DEFAULT_PROJECT_ID, &req.name, &token_hash)
    .await?;

  emit_log(
    "info",
    "squirreldb::admin",
    &format!("First admin token '{}' created during setup", req.name),
  );

  // Return full token only once
  Ok(Json(CreateTokenResponse { token, info }))
}

async fn api_create_token(
  State(state): State<AppState>,
  Path(project_id): Path<String>,
  Json(req): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, AppError> {
  let project_id: Uuid = project_id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;

  if req.name.is_empty() {
    return Err(AppError::BadRequest("Token name is required".into()));
  }

  // Generate new token
  let token = generate_token();
  let token_hash = hash_token(&token);

  // Store in database
  let info = state
    .backend
    .create_token(project_id, &req.name, &token_hash)
    .await?;

  // Return full token only once
  Ok(Json(CreateTokenResponse { token, info }))
}

#[derive(Deserialize)]
struct DeleteTokenPath {
  project_id: String,
  id: String,
}

async fn api_delete_token(
  State(state): State<AppState>,
  Path(path): Path<DeleteTokenPath>,
) -> Result<Json<serde_json::Value>, AppError> {
  let project_id: Uuid = path
    .project_id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;
  let id: Uuid = path
    .id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid token ID".into()))?;
  let deleted = state.backend.delete_token(project_id, id).await?;
  if deleted {
    Ok(Json(serde_json::json!({"deleted": true})))
  } else {
    Err(AppError::NotFound("Not found".to_string()))
  }
}

// =============================================================================
// Feature Management API
// =============================================================================

async fn api_list_features(State(state): State<AppState>) -> Json<Vec<FeatureInfo>> {
  Json(state.feature_registry.list())
}

#[derive(Deserialize)]
struct ToggleFeatureRequest {
  enabled: bool,
}

async fn api_toggle_feature(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Json(req): Json<ToggleFeatureRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  let feature_state = Arc::new(crate::features::AppState {
    backend: state.backend.clone(),
    engine_pool: state.engine_pool.clone(),
    config: state.config.clone(),
  });

  // Get existing settings from database (or empty)
  let existing_settings = state
    .backend
    .get_feature_settings(&name)
    .await
    .ok()
    .flatten()
    .map(|(_, s)| s)
    .unwrap_or(serde_json::json!({}));

  // Update enabled state in database
  state
    .backend
    .update_feature_settings(&name, req.enabled, existing_settings)
    .await
    .map_err(AppError::Internal)?;

  if req.enabled {
    state
      .feature_registry
      .start(&name, feature_state)
      .await
      .map_err(|e| AppError::BadRequest(e.to_string()))?;
    emit_log(
      "info",
      "squirreldb::admin",
      &format!("Feature '{}' enabled", name),
    );
  } else {
    state
      .feature_registry
      .stop(&name)
      .await
      .map_err(|e| AppError::BadRequest(e.to_string()))?;
    emit_log(
      "info",
      "squirreldb::admin",
      &format!("Feature '{}' disabled", name),
    );
  }

  Ok(Json(serde_json::json!({
    "name": name,
    "enabled": req.enabled
  })))
}

// =============================================================================
// Auth Settings API
// =============================================================================

#[derive(Serialize)]
struct AuthSettingsResponse {
  auth_required: bool,
}

async fn api_get_auth_settings(State(state): State<AppState>) -> Json<AuthSettingsResponse> {
  // Get auth settings from database, fallback to config
  let (auth_required, _) = state
    .backend
    .get_feature_settings("auth")
    .await
    .ok()
    .flatten()
    .unwrap_or((state.config.auth.enabled, serde_json::json!({})));

  Json(AuthSettingsResponse { auth_required })
}

#[derive(Deserialize)]
struct UpdateAuthSettingsRequest {
  auth_required: bool,
}

async fn api_update_auth_settings(
  State(state): State<AppState>,
  Json(req): Json<UpdateAuthSettingsRequest>,
) -> Result<Json<AuthSettingsResponse>, AppError> {
  // Store the auth settings in the database
  state
    .backend
    .update_feature_settings("auth", req.auth_required, serde_json::json!({}))
    .await
    .map_err(AppError::Internal)?;

  emit_log(
    "info",
    "squirreldb::admin",
    &format!(
      "API authentication {}",
      if req.auth_required {
        "enabled"
      } else {
        "disabled"
      }
    ),
  );

  Ok(Json(AuthSettingsResponse {
    auth_required: req.auth_required,
  }))
}

// =============================================================================
// Server Control API
// =============================================================================

async fn api_restart_server(State(state): State<AppState>) -> Json<serde_json::Value> {
  emit_log(
    "info",
    "squirreldb::admin",
    "Server restart requested via admin UI",
  );

  // Clone the shutdown signal sender
  let shutdown_tx = state.shutdown_tx.clone();

  // Spawn a task to trigger shutdown after a brief delay
  // This allows the HTTP response to be sent first
  tokio::spawn(async move {
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    if let Some(tx) = shutdown_tx {
      let _ = tx.send(());
    }
  });

  Json(serde_json::json!({
    "status": "restarting",
    "message": "Server is restarting..."
  }))
}

async fn api_health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "status": "ok",
    "uptime_secs": state.start_time.elapsed().as_secs()
  }))
}

// =============================================================================
// Protocol Settings API
// =============================================================================

#[derive(Serialize, Deserialize)]
struct ProtocolSettingsResponse {
  rest: bool,
  websocket: bool,
  sse: bool,
  tcp: bool,
  mcp: bool,
}

async fn api_get_protocol_settings(
  State(state): State<AppState>,
) -> Json<ProtocolSettingsResponse> {
  // Get protocol settings from database, fallback to config
  let (_, settings) = state
    .backend
    .get_feature_settings("protocols")
    .await
    .ok()
    .flatten()
    .unwrap_or((true, serde_json::json!({})));

  Json(ProtocolSettingsResponse {
    rest: settings
      .get("rest")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.rest),
    websocket: settings
      .get("websocket")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.websocket),
    sse: settings
      .get("sse")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.sse),
    tcp: settings
      .get("tcp")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.tcp),
    mcp: settings
      .get("mcp")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.mcp),
  })
}

#[derive(Deserialize)]
struct UpdateProtocolSettingsRequest {
  rest: Option<bool>,
  websocket: Option<bool>,
  sse: Option<bool>,
  tcp: Option<bool>,
  mcp: Option<bool>,
}

async fn api_update_protocol_settings(
  State(state): State<AppState>,
  Json(req): Json<UpdateProtocolSettingsRequest>,
) -> Result<Json<ProtocolSettingsResponse>, AppError> {
  // Get existing settings
  let (_, existing) = state
    .backend
    .get_feature_settings("protocols")
    .await
    .ok()
    .flatten()
    .unwrap_or((true, serde_json::json!({})));

  // Merge with new values
  let rest = req.rest.unwrap_or_else(|| {
    existing
      .get("rest")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.rest)
  });
  let websocket = req.websocket.unwrap_or_else(|| {
    existing
      .get("websocket")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.websocket)
  });
  let sse = req.sse.unwrap_or_else(|| {
    existing
      .get("sse")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.sse)
  });
  let tcp = req.tcp.unwrap_or_else(|| {
    existing
      .get("tcp")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.tcp)
  });
  let mcp = req.mcp.unwrap_or_else(|| {
    existing
      .get("mcp")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.server.protocols.mcp)
  });

  let new_settings = serde_json::json!({
    "rest": rest,
    "websocket": websocket,
    "sse": sse,
    "tcp": tcp,
    "mcp": mcp,
  });

  // Store in database
  state
    .backend
    .update_feature_settings("protocols", true, new_settings)
    .await
    .map_err(AppError::Internal)?;

  emit_log(
    "info",
    "squirreldb::admin",
    "Protocol settings updated (restart required)",
  );

  Ok(Json(ProtocolSettingsResponse {
    rest,
    websocket,
    sse,
    tcp,
    mcp,
  }))
}

// =============================================================================
// CORS Settings API
// =============================================================================

#[derive(Serialize, Deserialize)]
struct CorsSettingsResponse {
  origins: Vec<String>,
}

async fn api_get_cors_settings(State(state): State<AppState>) -> Json<CorsSettingsResponse> {
  // Get CORS settings from database, fallback to config
  let (_, settings) = state
    .backend
    .get_feature_settings("cors")
    .await
    .ok()
    .flatten()
    .unwrap_or((true, serde_json::json!({})));

  let origins = settings
    .get("origins")
    .and_then(|v| v.as_array())
    .map(|arr| {
      arr
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect()
    })
    .unwrap_or_else(|| state.config.server.cors_origins.clone());

  Json(CorsSettingsResponse { origins })
}

#[derive(Deserialize)]
struct UpdateCorsSettingsRequest {
  origins: Vec<String>,
}

async fn api_update_cors_settings(
  State(state): State<AppState>,
  Json(req): Json<UpdateCorsSettingsRequest>,
) -> Result<Json<CorsSettingsResponse>, AppError> {
  // Validate origins
  let mut valid_origins = Vec::new();
  for origin in &req.origins {
    let trimmed = origin.trim();
    if trimmed.is_empty() {
      continue;
    }
    // Allow "*" for permissive mode
    if trimmed == "*" {
      valid_origins.push("*".to_string());
      continue;
    }
    // Validate URL format for specific origins
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
      valid_origins.push(trimmed.to_string());
    } else {
      return Err(AppError::BadRequest(format!(
        "Invalid origin '{}': must be '*' or start with http:// or https://",
        trimmed
      )));
    }
  }

  // Empty list means no cross-origin requests allowed (strict restricted mode)
  // This is a valid security posture - only same-origin requests work

  let new_settings = serde_json::json!({
    "origins": valid_origins,
  });

  // Store in database
  state
    .backend
    .update_feature_settings("cors", true, new_settings)
    .await
    .map_err(AppError::Internal)?;

  emit_log(
    "info",
    "squirreldb::admin",
    &format!(
      "CORS settings updated: {:?} (restart required)",
      valid_origins
    ),
  );

  Ok(Json(CorsSettingsResponse {
    origins: valid_origins,
  }))
}

// =============================================================================
// S3 Management API
// =============================================================================

#[derive(Serialize)]
struct S3SettingsResponse {
  enabled: bool,
  port: u16,
  storage_path: String,
  max_object_size: u64,
  max_part_size: u64,
  region: String,
  mode: String,
  proxy_endpoint: String,
  proxy_access_key_id: String,
  proxy_region: String,
  proxy_bucket_prefix: Option<String>,
  proxy_force_path_style: bool,
}

async fn api_get_storage_settings(State(state): State<AppState>) -> Json<S3SettingsResponse> {
  let s3_running = state.feature_registry.is_enabled("storage");

  // Try to load settings from database, fallback to config
  let (enabled, settings) = state
    .backend
    .get_feature_settings("storage")
    .await
    .ok()
    .flatten()
    .unwrap_or((s3_running, serde_json::json!({})));

  let port = settings
    .get("port")
    .and_then(|v| v.as_u64())
    .map(|v| v as u16)
    .unwrap_or(state.config.storage.port);
  let storage_path = settings
    .get("storage_path")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_else(|| state.config.storage.storage_path.clone());
  let max_object_size = settings
    .get("max_object_size")
    .and_then(|v| v.as_u64())
    .unwrap_or(state.config.storage.max_object_size);
  let max_part_size = settings
    .get("max_part_size")
    .and_then(|v| v.as_u64())
    .unwrap_or(state.config.storage.max_part_size);
  let region = settings
    .get("region")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_else(|| state.config.storage.region.clone());

  let mode = settings
    .get("mode")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_else(|| "builtin".to_string());
  let proxy_endpoint = settings
    .get("proxy_endpoint")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_default();
  let proxy_access_key_id = settings
    .get("proxy_access_key_id")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_default();
  let proxy_region = settings
    .get("proxy_region")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_else(|| "us-east-1".to_string());
  let proxy_bucket_prefix = settings
    .get("proxy_bucket_prefix")
    .and_then(|v| v.as_str())
    .map(String::from);
  let proxy_force_path_style = settings
    .get("proxy_force_path_style")
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

  Json(S3SettingsResponse {
    enabled: enabled || s3_running,
    port,
    storage_path,
    max_object_size,
    max_part_size,
    region,
    mode,
    proxy_endpoint,
    proxy_access_key_id,
    proxy_region,
    proxy_bucket_prefix,
    proxy_force_path_style,
  })
}

#[derive(Deserialize)]
struct UpdateS3SettingsRequest {
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
}

async fn api_update_storage_settings(
  State(state): State<AppState>,
  Json(req): Json<UpdateS3SettingsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  // Build the settings JSON
  let mut settings = serde_json::Map::new();

  // Get current settings from config as defaults
  let current_port = state.config.storage.port;
  let current_storage = state.config.storage.storage_path.clone();
  let current_region = state.config.storage.region.clone();
  let current_max_object = state.config.storage.max_object_size;
  let current_max_part = state.config.storage.max_part_size;
  let current_min_part = state.config.storage.min_part_size;

  // Try to load existing settings from database
  let (existing_enabled, existing_settings) = state
    .backend
    .get_feature_settings("storage")
    .await
    .ok()
    .flatten()
    .unwrap_or((
      state.feature_registry.is_enabled("storage"),
      serde_json::json!({}),
    ));

  // Merge existing settings with updates
  let port = req.port.unwrap_or_else(|| {
    existing_settings
      .get("port")
      .and_then(|v| v.as_u64())
      .map(|v| v as u16)
      .unwrap_or(current_port)
  });
  let storage_path = req.storage_path.clone().unwrap_or_else(|| {
    existing_settings
      .get("storage_path")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or(current_storage)
  });
  let region = req.region.clone().unwrap_or_else(|| {
    existing_settings
      .get("region")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or(current_region)
  });
  let max_object_size = existing_settings
    .get("max_object_size")
    .and_then(|v| v.as_u64())
    .unwrap_or(current_max_object);
  let max_part_size = existing_settings
    .get("max_part_size")
    .and_then(|v| v.as_u64())
    .unwrap_or(current_max_part);
  let min_part_size = existing_settings
    .get("min_part_size")
    .and_then(|v| v.as_u64())
    .unwrap_or(current_min_part);

  settings.insert("port".to_string(), serde_json::json!(port));
  settings.insert("storage_path".to_string(), serde_json::json!(storage_path));
  settings.insert("region".to_string(), serde_json::json!(region));
  settings.insert(
    "max_object_size".to_string(),
    serde_json::json!(max_object_size),
  );
  settings.insert(
    "max_part_size".to_string(),
    serde_json::json!(max_part_size),
  );
  settings.insert(
    "min_part_size".to_string(),
    serde_json::json!(min_part_size),
  );

  // Handle proxy mode settings
  let mode = req.mode.clone().unwrap_or_else(|| {
    existing_settings
      .get("mode")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or_else(|| "builtin".to_string())
  });
  settings.insert("mode".to_string(), serde_json::json!(mode));

  let proxy_endpoint = req.proxy_endpoint.clone().unwrap_or_else(|| {
    existing_settings
      .get("proxy_endpoint")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or_default()
  });
  settings.insert(
    "proxy_endpoint".to_string(),
    serde_json::json!(proxy_endpoint),
  );

  let proxy_access_key_id = req.proxy_access_key_id.clone().unwrap_or_else(|| {
    existing_settings
      .get("proxy_access_key_id")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or_default()
  });
  settings.insert(
    "proxy_access_key_id".to_string(),
    serde_json::json!(proxy_access_key_id),
  );

  // Only update secret if provided (don't overwrite with empty)
  if let Some(secret) = req.proxy_secret_access_key.clone() {
    if !secret.is_empty() {
      settings.insert(
        "proxy_secret_access_key".to_string(),
        serde_json::json!(secret),
      );
    }
  } else if let Some(existing_secret) = existing_settings.get("proxy_secret_access_key") {
    settings.insert(
      "proxy_secret_access_key".to_string(),
      existing_secret.clone(),
    );
  }

  let proxy_region = req.proxy_region.clone().unwrap_or_else(|| {
    existing_settings
      .get("proxy_region")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or_else(|| "us-east-1".to_string())
  });
  settings.insert("proxy_region".to_string(), serde_json::json!(proxy_region));

  let proxy_bucket_prefix = req.proxy_bucket_prefix.clone().or_else(|| {
    existing_settings
      .get("proxy_bucket_prefix")
      .and_then(|v| v.as_str())
      .map(String::from)
  });
  if let Some(prefix) = &proxy_bucket_prefix {
    settings.insert("proxy_bucket_prefix".to_string(), serde_json::json!(prefix));
  }

  let proxy_force_path_style = req.proxy_force_path_style.unwrap_or_else(|| {
    existing_settings
      .get("proxy_force_path_style")
      .and_then(|v| v.as_bool())
      .unwrap_or(false)
  });
  settings.insert(
    "proxy_force_path_style".to_string(),
    serde_json::json!(proxy_force_path_style),
  );

  // Save settings to database
  let settings_json = serde_json::Value::Object(settings.clone());
  state
    .backend
    .update_feature_settings("storage", existing_enabled, settings_json)
    .await
    .map_err(AppError::Internal)?;

  emit_log(
    "info",
    "squirreldb::admin",
    &format!(
      "S3 settings saved to database: port={}, storage_path={}, region={}",
      port, storage_path, region
    ),
  );

  // If S3 is running, restart it with new settings
  if state.feature_registry.is_enabled("storage") {
    let feature_state = Arc::new(crate::features::AppState {
      backend: state.backend.clone(),
      engine_pool: state.engine_pool.clone(),
      config: state.config.clone(),
    });

    state
      .feature_registry
      .restart("storage", feature_state)
      .await
      .map_err(AppError::Internal)?;

    emit_log(
      "info",
      "squirreldb::admin",
      "S3 feature restarted with new settings",
    );
  }

  Ok(Json(serde_json::json!({
    "message": "S3 settings updated successfully",
    "settings": {
      "port": port,
      "storage_path": storage_path,
      "region": region
    },
    "restarted": state.feature_registry.is_enabled("storage")
  })))
}

#[derive(Serialize)]
struct StorageBucketResponse {
  name: String,
  versioning_enabled: bool,
  object_count: i64,
  current_size: i64,
  created_at: chrono::DateTime<chrono::Utc>,
}

async fn api_list_storage_buckets(
  State(state): State<AppState>,
) -> Result<Json<Vec<StorageBucketResponse>>, AppError> {
  let buckets = state.backend.list_storage_buckets().await?;
  let response: Vec<StorageBucketResponse> = buckets
    .into_iter()
    .map(|b| StorageBucketResponse {
      name: b.name,
      versioning_enabled: b.versioning_enabled,
      object_count: b.object_count,
      current_size: b.current_size,
      created_at: b.created_at,
    })
    .collect();
  Ok(Json(response))
}

#[derive(Deserialize)]
struct CreateStorageBucketRequest {
  name: String,
}

async fn api_create_storage_bucket(
  State(state): State<AppState>,
  Json(req): Json<CreateStorageBucketRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  if req.name.is_empty() {
    return Err(AppError::BadRequest("Bucket name is required".into()));
  }

  // Validate bucket name (S3 rules: 3-63 chars, lowercase, alphanumeric + hyphens)
  if req.name.len() < 3 || req.name.len() > 63 {
    return Err(AppError::BadRequest(
      "Bucket name must be 3-63 characters".into(),
    ));
  }

  for c in req.name.chars() {
    if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
      return Err(AppError::BadRequest(
        "Bucket name must contain only lowercase letters, numbers, and hyphens".into(),
      ));
    }
  }

  state.backend.create_storage_bucket(&req.name, None).await?;

  emit_log(
    "info",
    "squirreldb::admin",
    &format!("S3 bucket '{}' created", req.name),
  );

  Ok(Json(serde_json::json!({
    "name": req.name,
    "created": true
  })))
}

async fn api_delete_storage_bucket(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
  // Check if bucket exists and is empty
  let bucket = state
    .backend
    .get_storage_bucket(&name)
    .await?
    .ok_or_else(|| AppError::NotFound("Not found".to_string()))?;

  if bucket.object_count > 0 {
    return Err(AppError::BadRequest(
      "Bucket must be empty before deletion".into(),
    ));
  }

  state.backend.delete_storage_bucket(&name).await?;

  emit_log(
    "info",
    "squirreldb::admin",
    &format!("S3 bucket '{}' deleted", name),
  );

  Ok(Json(serde_json::json!({
    "name": name,
    "deleted": true
  })))
}

#[derive(Serialize)]
struct StorageBucketStatsResponse {
  name: String,
  object_count: i64,
  total_size: i64,
  versioning_enabled: bool,
}

async fn api_get_storage_bucket_stats(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Json<StorageBucketStatsResponse>, AppError> {
  let bucket = state
    .backend
    .get_storage_bucket(&name)
    .await?
    .ok_or_else(|| AppError::NotFound("Not found".to_string()))?;

  Ok(Json(StorageBucketStatsResponse {
    name: bucket.name,
    object_count: bucket.object_count,
    total_size: bucket.current_size,
    versioning_enabled: bucket.versioning_enabled,
  }))
}

#[derive(Serialize)]
struct StorageAccessKeyResponse {
  access_key_id: String,
  name: String,
  created_at: chrono::DateTime<chrono::Utc>,
}

async fn api_list_s3_keys(
  State(state): State<AppState>,
) -> Result<Json<Vec<StorageAccessKeyResponse>>, AppError> {
  let keys = state.backend.list_storage_access_keys().await?;
  let response: Vec<StorageAccessKeyResponse> = keys
    .into_iter()
    .map(|k| StorageAccessKeyResponse {
      access_key_id: k.access_key_id,
      name: k.name,
      created_at: k.created_at,
    })
    .collect();
  Ok(Json(response))
}

#[derive(Deserialize)]
struct CreateS3KeyRequest {
  name: String,
}

#[derive(Serialize)]
struct CreateS3KeyResponse {
  access_key_id: String,
  secret_access_key: String,
  name: String,
}

async fn api_create_s3_key(
  State(state): State<AppState>,
  Json(req): Json<CreateS3KeyRequest>,
) -> Result<Json<CreateS3KeyResponse>, AppError> {
  if req.name.is_empty() {
    return Err(AppError::BadRequest("Key name is required".into()));
  }

  // Generate access key ID (20 chars, uppercase alphanumeric, starts with AKIA)
  let access_key_id = generate_s3_access_key_id();

  // Generate secret access key (40 chars, base64-like)
  let secret_access_key = generate_s3_secret_key();

  // Hash the secret key for storage
  let secret_hash = hash_token(&secret_access_key);

  // Store in database (no owner for admin-created keys)
  state
    .backend
    .create_storage_access_key(&access_key_id, &secret_hash, None, &req.name)
    .await?;

  emit_log(
    "info",
    "squirreldb::admin",
    &format!("S3 access key '{}' created", req.name),
  );

  // Return the plaintext secret key only once
  Ok(Json(CreateS3KeyResponse {
    access_key_id,
    secret_access_key,
    name: req.name,
  }))
}

async fn api_delete_s3_key(
  State(state): State<AppState>,
  Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
  let deleted = state.backend.delete_storage_access_key(&id).await?;
  if deleted {
    emit_log(
      "info",
      "squirreldb::admin",
      &format!("S3 access key '{}' deleted", id),
    );
    Ok(Json(serde_json::json!({"deleted": true})))
  } else {
    Err(AppError::NotFound("Not found".to_string()))
  }
}

/// Generate an AWS-style access key ID (20 chars, starts with AKIA)
fn generate_s3_access_key_id() -> String {
  const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
  let mut rng = rand::thread_rng();
  let suffix: String = (0..16)
    .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
    .collect();
  format!("AKIA{}", suffix)
}

/// Generate an AWS-style secret access key (40 chars)
fn generate_s3_secret_key() -> String {
  const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let mut rng = rand::thread_rng();
  (0..40)
    .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
    .collect()
}

// =============================================================================
// Cache Management API
// =============================================================================

#[derive(Serialize)]
struct CacheSettingsResponse {
  enabled: bool,
  port: u16,
  max_memory: String,
  eviction: String,
  default_ttl: u64,
  snapshot_enabled: bool,
  snapshot_path: String,
  snapshot_interval: u64,
  mode: String,
  proxy_host: String,
  proxy_port: u16,
  proxy_database: u8,
  proxy_tls_enabled: bool,
}

async fn api_get_cache_settings(State(state): State<AppState>) -> Json<CacheSettingsResponse> {
  let cache_running = state.feature_registry.is_enabled("caching");

  // Try to load settings from database, fallback to config
  let (enabled, settings) = state
    .backend
    .get_feature_settings("caching")
    .await
    .ok()
    .flatten()
    .unwrap_or((cache_running, serde_json::json!({})));

  let port = settings
    .get("port")
    .and_then(|v| v.as_u64())
    .map(|v| v as u16)
    .unwrap_or(state.config.caching.port);
  let max_memory = settings
    .get("max_memory")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_else(|| state.config.caching.max_memory.clone());
  let eviction = settings
    .get("eviction")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_else(|| state.config.caching.eviction.clone());
  let default_ttl = settings
    .get("default_ttl")
    .and_then(|v| v.as_u64())
    .unwrap_or(state.config.caching.default_ttl);
  let snapshot_enabled = settings
    .get("snapshot_enabled")
    .and_then(|v| v.as_bool())
    .unwrap_or(state.config.caching.snapshot.enabled);
  let snapshot_path = settings
    .get("snapshot_path")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_else(|| state.config.caching.snapshot.path.clone());
  let snapshot_interval = settings
    .get("snapshot_interval")
    .and_then(|v| v.as_u64())
    .unwrap_or(state.config.caching.snapshot.interval);

  let mode = settings
    .get("mode")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_else(|| "builtin".to_string());
  let proxy_host = settings
    .get("proxy_host")
    .and_then(|v| v.as_str())
    .map(String::from)
    .unwrap_or_else(|| "localhost".to_string());
  let proxy_port = settings
    .get("proxy_port")
    .and_then(|v| v.as_u64())
    .map(|v| v as u16)
    .unwrap_or(6379);
  let proxy_database = settings
    .get("proxy_database")
    .and_then(|v| v.as_u64())
    .map(|v| v as u8)
    .unwrap_or(0);
  let proxy_tls_enabled = settings
    .get("proxy_tls_enabled")
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

  Json(CacheSettingsResponse {
    enabled: enabled || cache_running,
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
    proxy_database,
    proxy_tls_enabled,
  })
}

#[derive(Deserialize)]
struct UpdateCacheSettingsRequest {
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
}

async fn api_update_cache_settings(
  State(state): State<AppState>,
  Json(req): Json<UpdateCacheSettingsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  // Get existing settings from database or config
  let (existing_enabled, existing_settings) = state
    .backend
    .get_feature_settings("caching")
    .await
    .ok()
    .flatten()
    .unwrap_or((
      state.feature_registry.is_enabled("caching"),
      serde_json::json!({}),
    ));

  // Merge existing settings with updates
  let port = req.port.unwrap_or_else(|| {
    existing_settings
      .get("port")
      .and_then(|v| v.as_u64())
      .map(|v| v as u16)
      .unwrap_or(state.config.caching.port)
  });
  let max_memory = req.max_memory.clone().unwrap_or_else(|| {
    existing_settings
      .get("max_memory")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or_else(|| state.config.caching.max_memory.clone())
  });
  let eviction = req.eviction.clone().unwrap_or_else(|| {
    existing_settings
      .get("eviction")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or_else(|| state.config.caching.eviction.clone())
  });
  let default_ttl = req.default_ttl.unwrap_or_else(|| {
    existing_settings
      .get("default_ttl")
      .and_then(|v| v.as_u64())
      .unwrap_or(state.config.caching.default_ttl)
  });
  let snapshot_enabled = req.snapshot_enabled.unwrap_or_else(|| {
    existing_settings
      .get("snapshot_enabled")
      .and_then(|v| v.as_bool())
      .unwrap_or(state.config.caching.snapshot.enabled)
  });
  let snapshot_path = req.snapshot_path.clone().unwrap_or_else(|| {
    existing_settings
      .get("snapshot_path")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or_else(|| state.config.caching.snapshot.path.clone())
  });
  let snapshot_interval = req.snapshot_interval.unwrap_or_else(|| {
    existing_settings
      .get("snapshot_interval")
      .and_then(|v| v.as_u64())
      .unwrap_or(state.config.caching.snapshot.interval)
  });

  // Handle proxy mode settings
  let mode = req.mode.clone().unwrap_or_else(|| {
    existing_settings
      .get("mode")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or_else(|| "builtin".to_string())
  });
  let proxy_host = req.proxy_host.clone().unwrap_or_else(|| {
    existing_settings
      .get("proxy_host")
      .and_then(|v| v.as_str())
      .map(String::from)
      .unwrap_or_else(|| "localhost".to_string())
  });
  let proxy_port = req.proxy_port.unwrap_or_else(|| {
    existing_settings
      .get("proxy_port")
      .and_then(|v| v.as_u64())
      .map(|v| v as u16)
      .unwrap_or(6379)
  });
  let proxy_database = req.proxy_database.unwrap_or_else(|| {
    existing_settings
      .get("proxy_database")
      .and_then(|v| v.as_u64())
      .map(|v| v as u8)
      .unwrap_or(0)
  });
  let proxy_tls_enabled = req.proxy_tls_enabled.unwrap_or_else(|| {
    existing_settings
      .get("proxy_tls_enabled")
      .and_then(|v| v.as_bool())
      .unwrap_or(false)
  });

  // Build settings JSON
  let mut settings_map = serde_json::Map::new();
  settings_map.insert("port".to_string(), serde_json::json!(port));
  settings_map.insert("max_memory".to_string(), serde_json::json!(max_memory));
  settings_map.insert("eviction".to_string(), serde_json::json!(eviction));
  settings_map.insert("default_ttl".to_string(), serde_json::json!(default_ttl));
  settings_map.insert(
    "snapshot_enabled".to_string(),
    serde_json::json!(snapshot_enabled),
  );
  settings_map.insert(
    "snapshot_path".to_string(),
    serde_json::json!(snapshot_path),
  );
  settings_map.insert(
    "snapshot_interval".to_string(),
    serde_json::json!(snapshot_interval),
  );
  settings_map.insert("mode".to_string(), serde_json::json!(mode));
  settings_map.insert("proxy_host".to_string(), serde_json::json!(proxy_host));
  settings_map.insert("proxy_port".to_string(), serde_json::json!(proxy_port));
  settings_map.insert(
    "proxy_database".to_string(),
    serde_json::json!(proxy_database),
  );
  settings_map.insert(
    "proxy_tls_enabled".to_string(),
    serde_json::json!(proxy_tls_enabled),
  );

  // Only update password if provided
  if let Some(pwd) = req.proxy_password.clone() {
    if !pwd.is_empty() {
      settings_map.insert("proxy_password".to_string(), serde_json::json!(pwd));
    }
  } else if let Some(existing_pwd) = existing_settings.get("proxy_password") {
    settings_map.insert("proxy_password".to_string(), existing_pwd.clone());
  }

  let settings_json = serde_json::Value::Object(settings_map);

  // Save settings to database
  state
    .backend
    .update_feature_settings("caching", existing_enabled, settings_json)
    .await
    .map_err(AppError::Internal)?;

  emit_log(
    "info",
    "squirreldb::admin",
    &format!(
      "Cache settings saved: port={}, max_memory={}, eviction={}",
      port, max_memory, eviction
    ),
  );

  // If cache is running, restart it with new settings
  if state.feature_registry.is_enabled("caching") {
    let feature_state = Arc::new(crate::features::AppState {
      backend: state.backend.clone(),
      engine_pool: state.engine_pool.clone(),
      config: state.config.clone(),
    });

    state
      .feature_registry
      .restart("caching", feature_state)
      .await
      .map_err(AppError::Internal)?;

    emit_log(
      "info",
      "squirreldb::admin",
      "Cache feature restarted with new settings",
    );
  }

  Ok(Json(serde_json::json!({
    "message": "Cache settings updated successfully",
    "settings": {
      "port": port,
      "max_memory": max_memory,
      "eviction": eviction,
      "default_ttl": default_ttl,
      "snapshot_enabled": snapshot_enabled,
      "snapshot_path": snapshot_path,
      "snapshot_interval": snapshot_interval,
    },
    "restarted": state.feature_registry.is_enabled("caching")
  })))
}

#[derive(Serialize)]
struct CacheStatsResponse {
  keys: usize,
  memory_used: usize,
  memory_limit: usize,
  hits: u64,
  misses: u64,
  evictions: u64,
  expired: u64,
}

async fn api_get_cache_stats(State(state): State<AppState>) -> Json<CacheStatsResponse> {
  // Try to get stats from running cache feature
  if let Some(feature) = state.feature_registry.get("caching") {
    if feature.is_running() {
      // Downcast to CacheFeature to get stats
      if let Some(cache_feature) = feature
        .as_any()
        .downcast_ref::<crate::cache::CacheFeature>()
      {
        if let Some(store) = cache_feature.get_store() {
          let stats = store.info().await;
          return Json(CacheStatsResponse {
            keys: stats.keys,
            memory_used: stats.memory_used,
            memory_limit: stats.memory_limit,
            hits: stats.hits,
            misses: stats.misses,
            evictions: stats.evictions,
            expired: stats.expired,
          });
        }
      }
    }
  }

  // Return empty stats if cache not running
  Json(CacheStatsResponse {
    keys: 0,
    memory_used: 0,
    memory_limit: 0,
    hits: 0,
    misses: 0,
    evictions: 0,
    expired: 0,
  })
}

async fn api_flush_cache(
  State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
  // Try to flush running cache
  if let Some(feature) = state.feature_registry.get("caching") {
    if feature.is_running() {
      if let Some(cache_feature) = feature
        .as_any()
        .downcast_ref::<crate::cache::CacheFeature>()
      {
        if let Some(store) = cache_feature.get_store() {
          store.flush().await;
          emit_log("info", "squirreldb::admin", "Cache flushed via admin API");
          return Ok(Json(
            serde_json::json!({"message": "Cache flushed", "flushed": true}),
          ));
        }
      }
    }
  }

  Err(AppError::BadRequest("Cache is not running".to_string()))
}

// =============================================================================
// Backup API
// =============================================================================

#[derive(Serialize)]
struct BackupSettingsResponse {
  enabled: bool,
  interval: u64,
  retention: u32,
  local_path: String,
  storage_path: String,
  last_backup: Option<String>,
  next_backup: Option<String>,
  storage_enabled: bool,
}

async fn api_get_backup_settings(State(state): State<AppState>) -> Json<BackupSettingsResponse> {
  let backup_config = &state.config.backup;
  let storage_enabled = state.feature_registry.is_enabled("storage");

  // Get last/next backup times from the feature if running
  let (last_backup, next_backup) = if let Some(feature) = state.feature_registry.get("backup") {
    if let Some(backup_feature) = feature
      .as_any()
      .downcast_ref::<crate::backup::BackupFeature>()
    {
      (
        backup_feature.last_backup().map(|t| t.to_rfc3339()),
        backup_feature.next_backup().map(|t| t.to_rfc3339()),
      )
    } else {
      (None, None)
    }
  } else {
    (None, None)
  };

  Json(BackupSettingsResponse {
    enabled: state.feature_registry.is_enabled("backup"),
    interval: backup_config.interval,
    retention: backup_config.retention,
    local_path: backup_config.local_path.clone(),
    storage_path: backup_config.storage_path.clone(),
    last_backup,
    next_backup,
    storage_enabled,
  })
}

#[derive(Deserialize)]
struct UpdateBackupSettingsReq {
  interval: Option<u64>,
  retention: Option<u32>,
  local_path: Option<String>,
  storage_path: Option<String>,
}

async fn api_update_backup_settings(
  State(state): State<AppState>,
  Json(req): Json<UpdateBackupSettingsReq>,
) -> Result<Json<serde_json::Value>, AppError> {
  // Get current settings
  let (_, current_settings) = state
    .backend
    .get_feature_settings("backup")
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?
    .unwrap_or((false, serde_json::json!({})));

  // Merge updates
  let mut settings = current_settings.clone();
  if let Some(interval) = req.interval {
    settings["interval"] = serde_json::json!(interval);
  }
  if let Some(retention) = req.retention {
    settings["retention"] = serde_json::json!(retention);
  }
  if let Some(local_path) = req.local_path {
    settings["local_path"] = serde_json::json!(local_path);
  }
  if let Some(storage_path) = req.storage_path {
    settings["storage_path"] = serde_json::json!(storage_path);
  }

  // Save to database
  let enabled = state.feature_registry.is_enabled("backup");
  state
    .backend
    .update_feature_settings("backup", enabled, settings.clone())
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

  emit_log("info", "squirreldb::admin", "Backup settings updated");

  Ok(Json(serde_json::json!({
    "message": "Backup settings updated",
    "settings": settings,
    "restart_required": true
  })))
}

#[derive(Serialize)]
struct BackupInfoResponse {
  id: String,
  filename: String,
  size: i64,
  created_at: String,
  backend: String,
  location: String,
}

async fn api_list_backups(
  State(state): State<AppState>,
) -> Result<Json<Vec<BackupInfoResponse>>, AppError> {
  if let Some(feature) = state.feature_registry.get("backup") {
    if let Some(backup_feature) = feature
      .as_any()
      .downcast_ref::<crate::backup::BackupFeature>()
    {
      let backups = backup_feature
        .list_backups(&state.config)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

      let response: Vec<BackupInfoResponse> = backups
        .into_iter()
        .map(|b| BackupInfoResponse {
          id: b.id,
          filename: b.filename,
          size: b.size,
          created_at: b.created_at.to_rfc3339(),
          backend: b.backend,
          location: b.location,
        })
        .collect();

      return Ok(Json(response));
    }
  }

  // If backup feature not available, try to list from filesystem
  let local_path = std::path::PathBuf::from(&state.config.backup.local_path);
  let mut backups = Vec::new();

  if local_path.exists() {
    if let Ok(mut entries) = tokio::fs::read_dir(&local_path).await {
      while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "sql") {
          if let Ok(metadata) = entry.metadata().await {
            let filename = path
              .file_name()
              .unwrap_or_default()
              .to_string_lossy()
              .to_string();
            backups.push(BackupInfoResponse {
              id: filename
                .split('_')
                .next_back()
                .unwrap_or("unknown")
                .replace(".sql", ""),
              filename: filename.clone(),
              size: metadata.len() as i64,
              created_at: metadata
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                .unwrap_or_default(),
              backend: "unknown".to_string(),
              location: path.to_string_lossy().to_string(),
            });
          }
        }
      }
    }
  }

  Ok(Json(backups))
}

async fn api_create_backup(
  State(state): State<AppState>,
) -> Result<Json<BackupInfoResponse>, AppError> {
  if let Some(feature) = state.feature_registry.get("backup") {
    if let Some(backup_feature) = feature
      .as_any()
      .downcast_ref::<crate::backup::BackupFeature>()
    {
      let backup = backup_feature
        .create_backup(&state.backend, &state.config)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

      emit_log(
        "info",
        "squirreldb::admin",
        &format!("Manual backup created: {}", backup.filename),
      );

      return Ok(Json(BackupInfoResponse {
        id: backup.id,
        filename: backup.filename,
        size: backup.size,
        created_at: backup.created_at.to_rfc3339(),
        backend: backup.backend,
        location: backup.location,
      }));
    }
  }

  Err(AppError::BadRequest(
    "Backup feature is not available".to_string(),
  ))
}

async fn api_delete_backup(
  Path(id): Path<String>,
  State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
  if let Some(feature) = state.feature_registry.get("backup") {
    if let Some(backup_feature) = feature
      .as_any()
      .downcast_ref::<crate::backup::BackupFeature>()
    {
      let deleted = backup_feature
        .delete_backup(&state.config, &id)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

      if deleted {
        emit_log(
          "info",
          "squirreldb::admin",
          &format!("Backup deleted: {}", id),
        );
        return Ok(Json(serde_json::json!({ "deleted": true, "id": id })));
      } else {
        return Err(AppError::NotFound(format!("Backup '{}' not found", id)));
      }
    }
  }

  Err(AppError::BadRequest(
    "Backup feature is not available".to_string(),
  ))
}

// =============================================================================
// WebSocket Handler
// =============================================================================

#[derive(Deserialize)]
struct WsAuthParams {
  token: Option<String>,
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
  // Data WebSocket - no auth required (auth is only for admin UI)
  ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
    .into_response()
}

async fn handle_ws_connection(socket: WebSocket, state: AppState) {
  let client_id = Uuid::new_v4();
  let (mut sink, mut stream) = socket.split();
  let (tx, mut rx) = mpsc::unbounded_channel();

  // Register client
  state.ws_clients.write().await.insert(client_id, tx);

  let handler = MessageHandler::new(
    state.backend.clone(),
    state.subs.clone(),
    state.engine_pool.clone(),
  );

  // Task to send messages to client
  let clients = state.ws_clients.clone();
  let send_task = tokio::spawn(async move {
    while let Some(msg) = rx.recv().await {
      if let Ok(json) = serde_json::to_string(&msg) {
        if sink.send(Message::Text(json.into())).await.is_err() {
          break;
        }
      }
    }
  });

  // Process incoming messages
  while let Some(Ok(msg)) = stream.next().await {
    if let Message::Text(text) = msg {
      if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
        let resp = handler.handle(client_id, client_msg).await;
        if let Some(tx) = clients.read().await.get(&client_id) {
          let _ = tx.send(resp);
        }
      }
    }
  }

  // Cleanup
  state.ws_clients.write().await.remove(&client_id);
  state.subs.remove_client(client_id).await;
  send_task.abort();
}

// =============================================================================
// Log Streaming WebSocket Handler
// =============================================================================

async fn ws_logs_handler(
  ws: WebSocketUpgrade,
  Query(params): Query<WsAuthParams>,
  State(state): State<AppState>,
) -> Response {
  // Check auth if enabled (admin-only access)
  if state.config.auth.enabled {
    match params.token {
      Some(ref t) => {
        // Check admin token first
        let mut authorized = false;
        if let Some(ref admin_token) = state.config.auth.admin_token {
          if !admin_token.is_empty() && t == admin_token {
            authorized = true;
          }
        }

        // Check API token
        if !authorized {
          let token_hash = hash_token(t);
          authorized = state
            .backend
            .validate_token(&token_hash)
            .await
            .ok()
            .flatten()
            .is_some();
        }

        if !authorized {
          return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Invalid token"})),
          )
            .into_response();
        }
      }
      None => {
        return (
          StatusCode::UNAUTHORIZED,
          Json(serde_json::json!({"error": "Missing token. Use ?token=sqrl_xxx"})),
        )
          .into_response();
      }
    }
  }

  ws.on_upgrade(move |socket| handle_log_stream(socket, state))
    .into_response()
}

async fn handle_log_stream(socket: WebSocket, state: AppState) {
  let (mut sink, mut stream) = socket.split();
  let mut log_rx = state.log_tx.subscribe();

  // Emit a welcome log entry
  emit_log("info", "squirreldb::admin", "Log stream connected");

  // Task to send log entries to client
  let send_task = tokio::spawn(async move {
    while let Ok(entry) = log_rx.recv().await {
      if let Ok(json) = serde_json::to_string(&entry) {
        if sink.send(Message::Text(json.into())).await.is_err() {
          break;
        }
      }
    }
  });

  // Keep connection alive, handle client disconnect
  while let Some(Ok(msg)) = stream.next().await {
    // Handle ping/pong or close messages
    if let Message::Close(_) = msg {
      break;
    }
  }

  send_task.abort();
}

// =============================================================================
// Project API
// =============================================================================

#[derive(Serialize)]
struct ProjectResponse {
  id: String,
  name: String,
  description: Option<String>,
  owner_id: String,
  created_at: String,
  updated_at: String,
}

impl From<crate::types::Project> for ProjectResponse {
  fn from(p: crate::types::Project) -> Self {
    Self {
      id: p.id.to_string(),
      name: p.name,
      description: p.description,
      owner_id: p.owner_id.to_string(),
      created_at: p.created_at.to_rfc3339(),
      updated_at: p.updated_at.to_rfc3339(),
    }
  }
}

#[derive(Serialize)]
struct ProjectMemberResponse {
  id: String,
  project_id: String,
  user_id: String,
  role: String,
  created_at: String,
  user: Option<AdminUserResponse>,
}

/// GET /api/projects - List all projects the user has access to
async fn api_list_projects(
  State(state): State<AppState>,
  headers: HeaderMap,
) -> Result<Json<Vec<ProjectResponse>>, AppError> {
  let token = extract_token_from_headers(&headers)
    .ok_or_else(|| AppError::Unauthorized("Missing auth token".to_string()))?;
  let session_token = token
    .strip_prefix("session_")
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  let session_hash = auth::hash_session_token(session_token);
  let (_, user) = state
    .backend
    .validate_admin_session(&session_hash)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;

  // For owners, return all projects. For others, return only their projects.
  let projects = if user.role == AdminRole::Owner {
    state.backend.list_projects().await?
  } else {
    state.backend.list_user_projects(user.id).await?
  };
  Ok(Json(projects.into_iter().map(|p| p.into()).collect()))
}

#[derive(Deserialize)]
struct CreateProjectRequest {
  name: String,
  description: Option<String>,
}

/// POST /api/projects - Create a new project
async fn api_create_project(
  State(state): State<AppState>,
  headers: HeaderMap,
  Json(body): Json<CreateProjectRequest>,
) -> Result<Json<ProjectResponse>, AppError> {
  let token = extract_token_from_headers(&headers)
    .ok_or_else(|| AppError::Unauthorized("Missing auth token".to_string()))?;
  let session_token = token
    .strip_prefix("session_")
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  let session_hash = auth::hash_session_token(session_token);
  let (_, user) = state
    .backend
    .validate_admin_session(&session_hash)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;

  if body.name.trim().is_empty() {
    return Err(AppError::BadRequest("Project name is required".to_string()));
  }

  let project = state
    .backend
    .create_project(body.name.trim(), body.description.as_deref(), user.id)
    .await?;
  Ok(Json(project.into()))
}

/// GET /api/projects/:id - Get a specific project
async fn api_get_project(
  State(state): State<AppState>,
  Path(id): Path<String>,
) -> Result<Json<ProjectResponse>, AppError> {
  let project_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;

  let project = state
    .backend
    .get_project(project_id)
    .await?
    .ok_or_else(|| AppError::NotFound("Project not found".to_string()))?;
  Ok(Json(project.into()))
}

#[derive(Deserialize)]
struct UpdateProjectRequest {
  name: String,
  description: Option<String>,
}

/// PUT /api/projects/:id - Update a project
async fn api_update_project(
  State(state): State<AppState>,
  headers: HeaderMap,
  Path(id): Path<String>,
  Json(body): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>, AppError> {
  let project_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;

  // Verify user has permission to update this project
  let token = extract_token_from_headers(&headers)
    .ok_or_else(|| AppError::Unauthorized("Missing auth token".to_string()))?;
  let session_token = token
    .strip_prefix("session_")
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  let session_hash = auth::hash_session_token(session_token);
  let (_, user) = state
    .backend
    .validate_admin_session(&session_hash)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;

  // Check permission
  let role = state
    .backend
    .get_user_project_role(project_id, user.id)
    .await?;
  match role {
    Some(crate::types::ProjectRole::Owner) | Some(crate::types::ProjectRole::Admin) => {}
    _ if user.role == AdminRole::Owner => {} // System owners can manage all
    _ => {
      return Err(AppError::Forbidden(
        "Cannot update this project".to_string(),
      ))
    }
  }

  let project = state
    .backend
    .update_project(project_id, &body.name, body.description.as_deref())
    .await?
    .ok_or_else(|| AppError::NotFound("Project not found".to_string()))?;
  Ok(Json(project.into()))
}

/// DELETE /api/projects/:id - Delete a project
async fn api_delete_project(
  State(state): State<AppState>,
  headers: HeaderMap,
  Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
  let project_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;

  // Only project owner or system owner can delete
  let token = extract_token_from_headers(&headers)
    .ok_or_else(|| AppError::Unauthorized("Missing auth token".to_string()))?;
  let session_token = token
    .strip_prefix("session_")
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  let session_hash = auth::hash_session_token(session_token);
  let (_, user) = state
    .backend
    .validate_admin_session(&session_hash)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;

  let role = state
    .backend
    .get_user_project_role(project_id, user.id)
    .await?;
  match role {
    Some(crate::types::ProjectRole::Owner) => {}
    _ if user.role == AdminRole::Owner => {} // System owners can delete all
    _ => {
      return Err(AppError::Forbidden(
        "Only project owner can delete".to_string(),
      ))
    }
  }

  // Prevent deleting default project
  if project_id == DEFAULT_PROJECT_ID {
    return Err(AppError::BadRequest(
      "Cannot delete default project".to_string(),
    ));
  }

  let deleted = state.backend.delete_project(project_id).await?;
  if !deleted {
    return Err(AppError::NotFound("Project not found".to_string()));
  }
  Ok(Json(serde_json::json!({"deleted": true})))
}

/// GET /api/projects/:id/members - List project members
async fn api_list_project_members(
  State(state): State<AppState>,
  Path(id): Path<String>,
) -> Result<Json<Vec<ProjectMemberResponse>>, AppError> {
  let project_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;

  let members = state.backend.get_project_members(project_id).await?;
  Ok(Json(
    members
      .into_iter()
      .map(|(m, u)| ProjectMemberResponse {
        id: m.id.to_string(),
        project_id: m.project_id.to_string(),
        user_id: m.user_id.to_string(),
        role: m.role.to_string(),
        created_at: m.created_at.to_rfc3339(),
        user: Some(u.into()),
      })
      .collect(),
  ))
}

#[derive(Deserialize)]
struct AddMemberRequest {
  user_id: String,
  role: String,
}

/// POST /api/projects/:id/members - Add a member to project
async fn api_add_project_member(
  State(state): State<AppState>,
  headers: HeaderMap,
  Path(id): Path<String>,
  Json(body): Json<AddMemberRequest>,
) -> Result<Json<ProjectMemberResponse>, AppError> {
  let project_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;
  let user_id: Uuid = body
    .user_id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid user ID".to_string()))?;
  let role: crate::types::ProjectRole = body
    .role
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid role".to_string()))?;

  // Check permission
  let token = extract_token_from_headers(&headers)
    .ok_or_else(|| AppError::Unauthorized("Missing auth token".to_string()))?;
  let session_token = token
    .strip_prefix("session_")
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  let session_hash = auth::hash_session_token(session_token);
  let (_, current_user) = state
    .backend
    .validate_admin_session(&session_hash)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;

  let current_role = state
    .backend
    .get_user_project_role(project_id, current_user.id)
    .await?;
  let can_manage = match current_role {
    Some(r) if r.can_manage_members() => true,
    _ if current_user.role == AdminRole::Owner => true,
    _ => false,
  };
  if !can_manage {
    return Err(AppError::Forbidden("Cannot manage members".to_string()));
  }

  let member = state
    .backend
    .add_project_member(project_id, user_id, role)
    .await?;
  Ok(Json(ProjectMemberResponse {
    id: member.id.to_string(),
    project_id: member.project_id.to_string(),
    user_id: member.user_id.to_string(),
    role: member.role.to_string(),
    created_at: member.created_at.to_rfc3339(),
    user: None,
  }))
}

#[derive(Deserialize)]
struct UpdateMemberRoleRequest {
  role: String,
}

/// PUT /api/projects/:id/members/:user_id - Update member role
async fn api_update_project_member_role(
  State(state): State<AppState>,
  headers: HeaderMap,
  Path((id, target_user_id)): Path<(String, String)>,
  Json(body): Json<UpdateMemberRoleRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  let project_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;
  let user_id: Uuid = target_user_id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid user ID".to_string()))?;
  let role: crate::types::ProjectRole = body
    .role
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid role".to_string()))?;

  // Check permission
  let token = extract_token_from_headers(&headers)
    .ok_or_else(|| AppError::Unauthorized("Missing auth token".to_string()))?;
  let session_token = token
    .strip_prefix("session_")
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  let session_hash = auth::hash_session_token(session_token);
  let (_, current_user) = state
    .backend
    .validate_admin_session(&session_hash)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;

  let current_role = state
    .backend
    .get_user_project_role(project_id, current_user.id)
    .await?;
  let can_manage = match current_role {
    Some(r) if r.can_manage_members() => true,
    _ if current_user.role == AdminRole::Owner => true,
    _ => false,
  };
  if !can_manage {
    return Err(AppError::Forbidden("Cannot manage members".to_string()));
  }

  let updated = state
    .backend
    .update_member_role(project_id, user_id, role)
    .await?;
  Ok(Json(serde_json::json!({"updated": updated})))
}

/// DELETE /api/projects/:id/members/:user_id - Remove a member
async fn api_remove_project_member(
  State(state): State<AppState>,
  headers: HeaderMap,
  Path((id, target_user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
  let project_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;
  let user_id: Uuid = target_user_id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid user ID".to_string()))?;

  // Check permission
  let token = extract_token_from_headers(&headers)
    .ok_or_else(|| AppError::Unauthorized("Missing auth token".to_string()))?;
  let session_token = token
    .strip_prefix("session_")
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;
  let session_hash = auth::hash_session_token(session_token);
  let (_, current_user) = state
    .backend
    .validate_admin_session(&session_hash)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".to_string()))?;

  let current_role = state
    .backend
    .get_user_project_role(project_id, current_user.id)
    .await?;
  let can_manage = match current_role {
    Some(r) if r.can_manage_members() => true,
    _ if current_user.role == AdminRole::Owner => true,
    _ => false,
  };
  if !can_manage {
    return Err(AppError::Forbidden("Cannot manage members".to_string()));
  }

  let removed = state
    .backend
    .remove_project_member(project_id, user_id)
    .await?;
  Ok(Json(serde_json::json!({"removed": removed})))
}

/// POST /api/projects/:id/select - Select a project as current
async fn api_select_project(
  State(state): State<AppState>,
  Path(id): Path<String>,
) -> Result<Json<ProjectResponse>, AppError> {
  let project_id: Uuid = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid project ID".to_string()))?;

  let project = state
    .backend
    .get_project(project_id)
    .await?
    .ok_or_else(|| AppError::NotFound("Project not found".to_string()))?;
  Ok(Json(project.into()))
}

// =============================================================================
// Storage Browser API
// =============================================================================

#[derive(Serialize)]
struct BrowserObjectInfo {
  key: String,
  is_folder: bool,
  size: Option<i64>,
  last_modified: Option<String>,
  etag: Option<String>,
}

#[derive(Serialize)]
struct ListObjectsResponse {
  objects: Vec<BrowserObjectInfo>,
  common_prefixes: Vec<String>,
  prefix: Option<String>,
  truncated: bool,
}

#[derive(Deserialize)]
struct ListObjectsQuery {
  prefix: Option<String>,
  delimiter: Option<String>,
  max_keys: Option<i32>,
  continuation_token: Option<String>,
}

async fn api_list_bucket_objects(
  State(state): State<AppState>,
  Path(bucket): Path<String>,
  Query(query): Query<ListObjectsQuery>,
) -> Result<Json<ListObjectsResponse>, AppError> {
  let prefix = query.prefix.unwrap_or_default();
  let delimiter = query.delimiter.unwrap_or_else(|| "/".to_string());
  let max_keys = query.max_keys.unwrap_or(1000);

  let (storage_objects, is_truncated, _next_token) = state
    .backend
    .list_storage_objects(
      &bucket,
      Some(&prefix),
      Some(&delimiter),
      max_keys,
      query.continuation_token.as_deref(),
    )
    .await?;

  let objects: Vec<BrowserObjectInfo> = storage_objects
    .into_iter()
    .map(|obj| BrowserObjectInfo {
      key: obj.key,
      is_folder: false,
      size: Some(obj.size),
      last_modified: Some(obj.created_at.to_rfc3339()),
      etag: Some(obj.etag),
    })
    .collect();

  let common_prefixes = state
    .backend
    .list_storage_common_prefixes(&bucket, Some(&prefix), Some(&delimiter))
    .await?;

  Ok(Json(ListObjectsResponse {
    objects,
    common_prefixes,
    prefix: Some(prefix),
    truncated: is_truncated,
  }))
}

async fn api_delete_bucket_object(
  State(state): State<AppState>,
  Path((bucket, key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
  // Get the object to verify it exists
  let _obj = state
    .backend
    .get_storage_object(&bucket, &key, None)
    .await?
    .ok_or_else(|| AppError::NotFound("Object not found".to_string()))?;

  // Delete from database
  state
    .backend
    .delete_storage_object(&bucket, &key, None)
    .await?;

  emit_log(
    "info",
    "squirreldb::admin",
    &format!("Object deleted: {}/{}", bucket, key),
  );

  Ok(Json(serde_json::json!({
    "bucket": bucket,
    "key": key,
    "deleted": true
  })))
}

async fn api_download_object(
  State(state): State<AppState>,
  Path((bucket, key)): Path<(String, String)>,
) -> Result<Response, AppError> {
  // Get object metadata
  let obj = state
    .backend
    .get_storage_object(&bucket, &key, None)
    .await?
    .ok_or_else(|| AppError::NotFound("Object not found".to_string()))?;

  // Read object data from storage
  let data = if let Some(feature) = state.feature_registry.get("storage") {
    if feature
      .as_any()
      .downcast_ref::<crate::storage::StorageFeature>()
      .is_some()
    {
      // Access storage directly via backend read
      // For now, read from filesystem path
      tokio::fs::read(&obj.storage_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read object: {}", e)))?
    } else {
      return Err(AppError::Internal(anyhow::anyhow!(
        "Storage feature not available"
      )));
    }
  } else {
    return Err(AppError::Internal(anyhow::anyhow!("Storage not running")));
  };

  // Determine content type
  let content_type = obj.content_type.clone();

  // Build response with file download headers
  let filename = key.split('/').next_back().unwrap_or(&key);
  let disposition = format!("attachment; filename=\"{}\"", filename);

  Ok(
    Response::builder()
      .status(StatusCode::OK)
      .header(header::CONTENT_TYPE, content_type)
      .header(header::CONTENT_DISPOSITION, disposition)
      .header(header::CONTENT_LENGTH, data.len())
      .header("ETag", format!("\"{}\"", obj.etag))
      .body(Body::from(data))
      .unwrap(),
  )
}

async fn api_upload_object(
  State(state): State<AppState>,
  Path(bucket): Path<String>,
  mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
  let mut uploaded = Vec::new();

  while let Some(field) = multipart
    .next_field()
    .await
    .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {}", e)))?
  {
    let name = field.name().unwrap_or("file").to_string();
    let filename = field.file_name().map(String::from);
    let content_type = field
      .content_type()
      .map(String::from)
      .unwrap_or_else(|| "application/octet-stream".to_string());

    let data = field
      .bytes()
      .await
      .map_err(|e| AppError::BadRequest(format!("Failed to read file data: {}", e)))?;

    // Use filename or field name as key
    let key = filename.unwrap_or_else(|| name.clone());

    // Get storage feature and write object
    if let Some(feature) = state.feature_registry.get("storage") {
      if let Some(_storage_feature) = feature
        .as_any()
        .downcast_ref::<crate::storage::StorageFeature>()
      {
        // Generate version ID
        let version_id = uuid::Uuid::new_v4();

        // Calculate ETag
        let etag = format!("{:x}", md5::compute(&data));

        // Write to filesystem storage path
        let storage_path_setting = state
          .backend
          .get_feature_settings("storage")
          .await
          .ok()
          .flatten()
          .and_then(|(_, s)| {
            s.get("storage_path")
              .and_then(|v| v.as_str())
              .map(String::from)
          })
          .unwrap_or_else(|| state.config.storage.storage_path.clone());

        // Create storage path
        let key_hash = format!("{:x}", sha2::Sha256::digest(key.as_bytes()));
        let storage_dir = std::path::PathBuf::from(&storage_path_setting)
          .join("buckets")
          .join(&bucket)
          .join("objects")
          .join(&key_hash[0..2])
          .join(&key_hash[2..4]);

        tokio::fs::create_dir_all(&storage_dir)
          .await
          .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create directory: {}", e)))?;

        let storage_path = storage_dir.join(format!("{}.data", version_id));

        tokio::fs::write(&storage_path, &data)
          .await
          .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to write file: {}", e)))?;

        // Create object record in database
        state
          .backend
          .create_storage_object(
            &bucket,
            &key,
            version_id,
            &etag,
            data.len() as i64,
            &content_type,
            storage_path.to_string_lossy().as_ref(),
            serde_json::json!({}),
          )
          .await?;

        uploaded.push(serde_json::json!({
          "key": key,
          "size": data.len(),
          "etag": etag
        }));

        emit_log(
          "info",
          "squirreldb::admin",
          &format!("Object uploaded: {}/{} ({} bytes)", bucket, key, data.len()),
        );
      }
    } else {
      return Err(AppError::Internal(anyhow::anyhow!("Storage not running")));
    }
  }

  Ok(Json(serde_json::json!({
    "uploaded": uploaded
  })))
}

// =============================================================================
// Proxy Connection Test API
// =============================================================================

#[derive(Deserialize)]
struct TestStorageConnectionRequest {
  endpoint: String,
  access_key_id: String,
  secret_access_key: String,
  region: String,
  force_path_style: bool,
}

async fn api_test_storage_connection(
  Json(req): Json<TestStorageConnectionRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  use crate::storage::backend::StorageBackend;
  use crate::storage::config::ProxyConfig;
  use crate::storage::S3ProxyClient;

  let config = ProxyConfig {
    endpoint: req.endpoint,
    access_key_id: req.access_key_id,
    secret_access_key: req.secret_access_key,
    region: req.region,
    bucket_prefix: None,
    force_path_style: req.force_path_style,
  };

  let client = S3ProxyClient::new(config)
    .await
    .map_err(|e| AppError::BadRequest(format!("Failed to create client: {}", e)))?;

  client
    .test_connection()
    .await
    .map_err(|e| AppError::BadRequest(format!("Connection failed: {}", e)))?;

  Ok(Json(serde_json::json!({
    "status": "connected",
    "message": "Successfully connected to S3 endpoint"
  })))
}

#[derive(Deserialize)]
struct TestCacheConnectionRequest {
  host: String,
  port: u16,
  password: Option<String>,
  database: u8,
  tls_enabled: bool,
}

async fn api_test_cache_connection(
  Json(req): Json<TestCacheConnectionRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
  use crate::cache::config::CacheProxyConfig;
  use crate::cache::RedisProxyClient;

  let config = CacheProxyConfig {
    host: req.host,
    port: req.port,
    password: req.password,
    database: req.database,
    tls_enabled: req.tls_enabled,
  };

  let client = RedisProxyClient::new(config)
    .await
    .map_err(|e| AppError::BadRequest(format!("Failed to connect: {}", e)))?;

  client
    .test_connection()
    .await
    .map_err(|e| AppError::BadRequest(format!("Connection failed: {}", e)))?;

  Ok(Json(serde_json::json!({
    "status": "connected",
    "message": "Successfully connected to Redis server"
  })))
}

// =============================================================================
// Error Handling
// =============================================================================

enum AppError {
  Internal(anyhow::Error),
  NotFound(String),
  BadRequest(String),
  Unauthorized(String),
  Forbidden(String),
}

impl From<anyhow::Error> for AppError {
  fn from(e: anyhow::Error) -> Self {
    Self::Internal(e)
  }
}

impl From<serde_json::Error> for AppError {
  fn from(e: serde_json::Error) -> Self {
    Self::Internal(e.into())
  }
}

impl IntoResponse for AppError {
  fn into_response(self) -> Response {
    let (status, msg) = match self {
      Self::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
      Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
      Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
      Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
      Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
    };
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
  }
}
