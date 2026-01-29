use axum::extract::Request;
use axum::{
  extract::{
    ws::{Message, WebSocket, WebSocketUpgrade},
    Path, Query, State,
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
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

use super::auth;
use crate::db::{AdminRole, AdminUser, ApiTokenInfo, DatabaseBackend, SqlDialect};
use crate::features::{FeatureInfo, FeatureRegistry};
use crate::query::{QueryEngine, QueryEnginePool};
use crate::server::{MessageHandler, ServerConfig};
use crate::subscriptions::SubscriptionManager;
use crate::types::{ClientMessage, ServerMessage};

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
  config: ServerConfig,
  feature_registry: Arc<FeatureRegistry>,
}

impl AdminServer {
  pub fn new(
    backend: Backend,
    subs: Arc<SubscriptionManager>,
    engine_pool: Arc<QueryEnginePool>,
    shutdown_rx: broadcast::Receiver<()>,
    config: ServerConfig,
    feature_registry: Arc<FeatureRegistry>,
  ) -> Self {
    Self {
      backend,
      subs,
      engine_pool,
      shutdown_rx,
      config,
      feature_registry,
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
            .route("/api/auth/logout", post(api_auth_logout));

    // Admin API routes (protected by admin auth)
    let admin_routes = Router::new()
      .route("/api/settings", get(api_get_settings))
      .route("/api/settings", put(api_update_settings))
      .route("/api/tokens", get(api_list_tokens))
      .route("/api/tokens", post(api_create_token))
      .route("/api/tokens/{id}", delete(api_delete_token))
      // Feature management
      .route("/api/features", get(api_list_features))
      .route("/api/features/{name}", put(api_toggle_feature))
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
      // User management (owner only)
      .route("/api/users", get(api_list_users))
      .route("/api/users", post(api_create_user))
      .route("/api/users/{id}", delete(api_delete_user))
      .route("/api/users/{id}/role", put(api_update_user_role))
      .layer(axum::middleware::from_fn_with_state(
        state.clone(),
        admin_auth_middleware,
      ));
    app = app.merge(admin_routes);

    // REST API routes (conditional, public - no auth required)
    if self.config.server.protocols.rest {
      app = app
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
        .route("/api/query", post(api_query));
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
  // Check if any API tokens exist
  match state.backend.list_tokens().await {
    Ok(tokens) => tokens.is_empty(),
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
  match state.backend.list_collections().await {
    Ok(_) => StatusCode::OK,
    Err(_) => StatusCode::SERVICE_UNAVAILABLE,
  }
}

async fn api_collections(
  State(state): State<AppState>,
) -> Result<Json<Vec<CollectionInfo>>, AppError> {
  let names = state.backend.list_collections().await?;
  let mut collections = Vec::with_capacity(names.len());
  for name in names {
    let docs = state.backend.list(&name, None, None, None, None).await?;
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
    .list(&name, None, None, q.limit, q.offset)
    .await?;
  Ok(Json(serde_json::to_value(docs)?))
}

async fn api_drop_collection(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
  let docs = state.backend.list(&name, None, None, None, None).await?;
  let mut deleted = 0;
  for doc in docs {
    state.backend.delete(&name, doc.id).await?;
    deleted += 1;
  }
  Ok(Json(serde_json::json!({ "deleted": deleted })))
}

async fn api_insert_doc(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Json(data): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
  let doc = state.backend.insert(&name, data).await?;
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
  let doc = state.backend.get(&name, id).await?;
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
  let doc = state.backend.update(&name, id, data).await?;
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
  let doc = state.backend.delete(&name, id).await?;
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
  let docs = state
    .backend
    .list(
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
      if let Some(ref admin_token) = state.config.auth.admin_token {
        if !admin_token.is_empty() && t == *admin_token {
          return next.run(req).await;
        }
      }

      // Otherwise validate as API token
      let token_hash = hash_token(&t);
      match state.backend.validate_token(&token_hash).await {
        Ok(true) => next.run(req).await,
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
) -> Result<Json<Vec<ApiTokenInfo>>, AppError> {
  let tokens = state.backend.list_tokens().await?;
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
      "Setup already completed. Use /api/tokens to create additional tokens.".into(),
    ));
  }

  if req.name.is_empty() {
    return Err(AppError::BadRequest("Token name is required".into()));
  }

  // Generate new token
  let token = generate_token();
  let token_hash = hash_token(&token);

  // Store in database
  let info = state.backend.create_token(&req.name, &token_hash).await?;

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
  Json(req): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, AppError> {
  if req.name.is_empty() {
    return Err(AppError::BadRequest("Token name is required".into()));
  }

  // Generate new token
  let token = generate_token();
  let token_hash = hash_token(&token);

  // Store in database
  let info = state.backend.create_token(&req.name, &token_hash).await?;

  // Return full token only once
  Ok(Json(CreateTokenResponse { token, info }))
}

async fn api_delete_token(
  State(state): State<AppState>,
  Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
  let id = id
    .parse()
    .map_err(|_| AppError::BadRequest("Invalid UUID".into()))?;
  let deleted = state.backend.delete_token(id).await?;
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

  Json(S3SettingsResponse {
    enabled: enabled || s3_running,
    port,
    storage_path,
    max_object_size,
    max_part_size,
    region,
  })
}

#[derive(Deserialize)]
struct UpdateS3SettingsRequest {
  port: Option<u16>,
  storage_path: Option<String>,
  region: Option<String>,
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
            .unwrap_or(false);
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
