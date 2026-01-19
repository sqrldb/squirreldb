use axum::{
  extract::{
    ws::{Message, WebSocket, WebSocketUpgrade},
    Path, Query, Request, State,
  },
  http::{header, StatusCode},
  middleware::Next,
  response::{Html, IntoResponse, Response},
  routing::{delete, get, post, put},
  Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use leptos::ssr::render_to_string;
use parking_lot::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

use super::app::App;
use crate::db::{ApiTokenInfo, DatabaseBackend, SqlDialect};
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
}

impl AdminServer {
  pub fn new(
    backend: Backend,
    subs: Arc<SubscriptionManager>,
    engine_pool: Arc<QueryEnginePool>,
    shutdown_rx: broadcast::Receiver<()>,
    config: ServerConfig,
  ) -> Self {
    Self {
      backend,
      subs,
      engine_pool,
      shutdown_rx,
      config,
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
            .route("/client.js", get(serve_js))
            // Auth pages - always public
            .route("/setup", get(serve_setup_page))
            .route("/login", get(serve_login_page))
            // Setup API - only works when no tokens exist
            .route("/api/setup", post(api_setup_token));

    // Admin API routes (protected by admin auth)
    let admin_routes = Router::new()
      .route("/api/settings", get(api_get_settings))
      .route("/api/settings", put(api_update_settings))
      .route("/api/tokens", get(api_list_tokens))
      .route("/api/tokens", post(api_create_token))
      .route("/api/tokens/{id}", delete(api_delete_token))
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

    // Leptos SSR for admin UI (protected by admin auth when enabled)
    let app = app
      .fallback(get(serve_leptos_app_with_auth))
      .layer(CorsLayer::permissive())
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

/// Serve JavaScript
async fn serve_js() -> impl IntoResponse {
  (
    [(header::CONTENT_TYPE, "application/javascript")],
    include_str!("client.js"),
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

/// Serve Leptos app
/// The HTML page itself doesn't require auth - auth is handled client-side
/// and enforced on all API endpoints. This avoids the chicken-and-egg problem
/// of needing a token to load the page that will store the token.
async fn serve_leptos_app_with_auth(State(state): State<AppState>, _req: Request) -> Response {
  let auth_enabled = state.config.auth.enabled;

  // If setup is needed, redirect to setup page
  let setup_needed = needs_setup(&state).await;

  let html = render_to_string(App);
  Html(format!(
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SquirrelDB Admin</title>
    <link rel="stylesheet" href="/style.css">
</head>
<body>
    {}
    <script>
        // Auth configuration from server
        window.SQRL_AUTH_ENABLED = {};
        window.SQRL_SETUP_NEEDED = {};
    </script>
    <script src="/client.js"></script>
</body>
</html>"#,
    html, auth_enabled, setup_needed
  ))
  .into_response()
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
    let docs = state.backend.list(&name, None, None, None).await?;
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
  let docs = state.backend.list(&name, None, None, q.limit).await?;
  let offset = q.offset.unwrap_or(0);
  let docs: Vec<_> = docs.into_iter().skip(offset).collect();
  Ok(Json(serde_json::to_value(docs)?))
}

async fn api_drop_collection(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
  let docs = state.backend.list(&name, None, None, None).await?;
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
    None => Err(AppError::NotFound),
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
    None => Err(AppError::NotFound),
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
    None => Err(AppError::NotFound),
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
    .list(&spec.table, sql_filter, spec.order_by.as_ref(), spec.limit)
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
  // Try Authorization header first
  if let Some(token) = req
    .headers()
    .get("Authorization")
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.strip_prefix("Bearer "))
  {
    return Some(token.to_string());
  }

  // Try query parameter
  if let Some(query) = req.uri().query() {
    for pair in query.split('&') {
      if let Some(token) = pair.strip_prefix("token=") {
        return Some(token.to_string());
      }
    }
  }

  None
}

/// Auth middleware for admin UI routes
/// Allows access if: auth disabled, admin_token matches, or valid API token
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
    Err(AppError::NotFound)
  }
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
  state.subs.remove_client(client_id);
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
  NotFound,
  BadRequest(String),
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
      Self::NotFound => (StatusCode::NOT_FOUND, "Not found".into()),
      Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
    };
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
  }
}
