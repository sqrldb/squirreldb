use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

use super::{RateLimiter, ServerConfig, TcpServer, WebSocketServer};
use crate::admin::{emit_log, AdminServer};
use crate::db::DatabaseBackend;
use crate::mcp::McpServer;
use crate::query::QueryEnginePool;
use crate::subscriptions::SubscriptionManager;

pub struct Daemon {
  config: ServerConfig,
  backend: Arc<dyn DatabaseBackend>,
  subs: Arc<SubscriptionManager>,
  engine_pool: Arc<QueryEnginePool>,
  rate_limiter: Arc<RateLimiter>,
  shutdown_tx: broadcast::Sender<()>,
}

impl Daemon {
  pub fn new(config: ServerConfig, backend: Arc<dyn DatabaseBackend>) -> Self {
    let (shutdown_tx, _) = broadcast::channel(1);
    // Create engine pool with number of CPU cores
    let pool_size = std::thread::available_parallelism()
      .map(|n| n.get())
      .unwrap_or(4);
    let engine_pool = Arc::new(QueryEnginePool::new(pool_size, backend.dialect()));
    tracing::info!("QueryEngine pool created with {} engines", pool_size);

    // Create rate limiter
    let rate_limiter = Arc::new(RateLimiter::new(config.limits.clone()));
    tracing::info!(
      "Rate limiter created: {} conn/IP, {} req/s, {}ms query timeout",
      config.limits.max_connections_per_ip,
      config.limits.requests_per_second,
      config.limits.query_timeout_ms
    );

    Self {
      config,
      backend: backend.clone(),
      subs: Arc::new(SubscriptionManager::with_backend(backend)),
      engine_pool,
      rate_limiter,
      shutdown_tx,
    }
  }

  /// Trigger graceful shutdown of all servers
  pub fn shutdown(&self) {
    tracing::info!("Initiating graceful shutdown...");
    let _ = self.shutdown_tx.send(());
  }

  pub async fn run(&self) -> Result<(), anyhow::Error> {
    emit_log(
      "info",
      "squirreldb::daemon",
      "Initializing database schema...",
    );
    self.backend.init_schema().await?;
    emit_log("info", "squirreldb::daemon", "Database schema initialized");

    emit_log("info", "squirreldb::daemon", "Starting change listener...");
    self.backend.start_change_listener().await?;
    emit_log("info", "squirreldb::daemon", "Change listener started");

    let change_rx = self.backend.subscribe_changes();
    let subs = self.subs.clone();
    tokio::spawn(async move {
      subs.process_changes(change_rx).await;
    });

    // Start rate limiter cleanup task
    let cleanup_limiter = self.rate_limiter.clone();
    tokio::spawn(async move {
      loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        cleanup_limiter.cleanup();
      }
    });

    // Start admin UI server with shutdown signal
    let admin = AdminServer::new(
      self.backend.clone(),
      self.subs.clone(),
      self.engine_pool.clone(),
      self.shutdown_tx.subscribe(),
      self.config.clone(),
    );
    let admin_addr = self.config.admin_address();
    emit_log(
      "info",
      "squirreldb::admin",
      &format!("Starting admin UI on {}", admin_addr),
    );
    tokio::spawn(async move {
      if let Err(e) = admin.run(&admin_addr).await {
        tracing::error!("Admin server error: {}", e);
      }
    });

    // Start TCP wire protocol server if enabled
    if self.config.server.protocols.tcp {
      let tcp = TcpServer::new(
        self.backend.clone(),
        self.subs.clone(),
        self.engine_pool.clone(),
        self.rate_limiter.clone(),
        self.shutdown_tx.subscribe(),
        self.config.clone(),
      );
      let tcp_addr = self.config.tcp_address();
      emit_log(
        "info",
        "squirreldb::tcp",
        &format!("Starting TCP wire protocol server on {}", tcp_addr),
      );
      tracing::info!("SquirrelDB TCP on {}", tcp_addr);
      tokio::spawn(async move {
        if let Err(e) = tcp.run(&tcp_addr).await {
          tracing::error!("TCP server error: {}", e);
        }
      });
    } else {
      emit_log(
        "warn",
        "squirreldb::tcp",
        "TCP wire protocol server disabled",
      );
      tracing::info!("TCP wire protocol server disabled");
    }

    // Start MCP SSE server if enabled
    if self.config.server.protocols.mcp {
      let mcp_addr = self.config.mcp_address();
      let backend = self.backend.clone();
      let engine_pool = self.engine_pool.clone();
      emit_log(
        "info",
        "squirreldb::mcp",
        &format!("Starting MCP SSE server on {}", mcp_addr),
      );
      tracing::info!("SquirrelDB MCP SSE on {}", mcp_addr);
      tokio::spawn(async move {
        if let Err(e) = McpServer::run_sse(&mcp_addr, backend, engine_pool).await {
          tracing::error!("MCP server error: {}", e);
        }
      });
    } else {
      emit_log("warn", "squirreldb::mcp", "MCP SSE server disabled");
      tracing::info!("MCP SSE server disabled");
    }

    // Start WebSocket server only if enabled
    if self.config.server.protocols.websocket {
      let ws = WebSocketServer::new(
        self.backend.clone(),
        self.subs.clone(),
        self.engine_pool.clone(),
        self.rate_limiter.clone(),
        self.shutdown_tx.subscribe(),
      );
      emit_log(
        "info",
        "squirreldb::websocket",
        &format!("Starting WebSocket server on {}", self.config.address()),
      );
      tracing::info!("SquirrelDB WebSocket on {}", self.config.address());
      ws.run(&self.config.address()).await
    } else {
      emit_log("warn", "squirreldb::websocket", "WebSocket server disabled");
      tracing::info!("WebSocket server disabled");
      // Keep the daemon running
      loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
      }
    }
  }
}
