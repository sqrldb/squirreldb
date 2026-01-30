use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

use super::{RateLimiter, ServerConfig, TcpServer, WebSocketServer};
use crate::admin::{emit_log, AdminServer};
use crate::backup::BackupFeature;
use crate::cache::{CacheConfig, CacheFeature};
use crate::db::DatabaseBackend;
use crate::features::{AppState, FeatureRegistry};
use crate::mcp::McpServer;
use crate::query::QueryEnginePool;
use crate::storage::{StorageConfig, StorageFeature};
use crate::subscriptions::SubscriptionManager;

pub struct Daemon {
  config: ServerConfig,
  backend: Arc<dyn DatabaseBackend>,
  subs: Arc<SubscriptionManager>,
  engine_pool: Arc<QueryEnginePool>,
  rate_limiter: Arc<RateLimiter>,
  shutdown_tx: broadcast::Sender<()>,
  feature_registry: Arc<FeatureRegistry>,
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

    // Create feature registry
    let feature_registry = Arc::new(FeatureRegistry::new());

    // Register S3 feature
    let s3_config = StorageConfig::from(&config.storage);
    let s3_feature = Arc::new(StorageFeature::new(s3_config));
    feature_registry.register(s3_feature);

    // Register cache feature
    let cache_config = CacheConfig::from(&config.caching);
    let cache_feature = Arc::new(CacheFeature::new(cache_config));
    feature_registry.register(cache_feature);

    // Register backup feature
    let backup_feature = Arc::new(BackupFeature::new());
    feature_registry.register(backup_feature);

    Self {
      config,
      backend: backend.clone(),
      subs: Arc::new(SubscriptionManager::with_backend(backend)),
      engine_pool,
      rate_limiter,
      shutdown_tx,
      feature_registry,
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

    // Start admin UI server with shutdown signal (if enabled)
    if self.config.server.admin {
      let admin = AdminServer::new(
        self.backend.clone(),
        self.subs.clone(),
        self.engine_pool.clone(),
        self.shutdown_tx.subscribe(),
        self.shutdown_tx.clone(),
        self.config.clone(),
        self.feature_registry.clone(),
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
    } else {
      emit_log("warn", "squirreldb::admin", "Admin UI disabled");
      tracing::info!("Admin UI disabled");
    }

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

    // Start S3 feature if enabled
    if self.config.features.storage {
      let app_state = Arc::new(AppState {
        backend: self.backend.clone(),
        engine_pool: self.engine_pool.clone(),
        config: self.config.clone(),
      });
      let s3_addr = self.config.storage_address();
      emit_log(
        "info",
        "squirreldb::s3",
        &format!("Starting S3 server on {}", s3_addr),
      );
      if let Err(e) = self.feature_registry.start("storage", app_state).await {
        tracing::error!("Failed to start S3 feature: {}", e);
      } else {
        tracing::info!("SquirrelDB S3 on {}", s3_addr);
      }
    } else {
      emit_log("warn", "squirreldb::s3", "S3 feature disabled");
      tracing::info!("S3 feature disabled");
    }

    // Start cache feature if enabled
    if self.config.features.caching {
      let app_state = Arc::new(AppState {
        backend: self.backend.clone(),
        engine_pool: self.engine_pool.clone(),
        config: self.config.clone(),
      });
      let cache_addr = self.config.cache_address();
      emit_log(
        "info",
        "squirreldb::cache",
        &format!("Starting cache server on {}", cache_addr),
      );
      if let Err(e) = self.feature_registry.start("caching", app_state).await {
        tracing::error!("Failed to start cache feature: {}", e);
      } else {
        tracing::info!("SquirrelDB Cache on {}", cache_addr);
      }
    } else {
      emit_log("warn", "squirreldb::cache", "Cache feature disabled");
      tracing::info!("Cache feature disabled");
    }

    // Start backup feature if enabled
    if self.config.features.backup {
      let app_state = Arc::new(AppState {
        backend: self.backend.clone(),
        engine_pool: self.engine_pool.clone(),
        config: self.config.clone(),
      });

      // If storage is enabled, set the storage backend for backup
      if self.config.features.storage {
        if let Some(storage_feature) = self.feature_registry.get("storage") {
          if let Some(sf) = storage_feature.as_any().downcast_ref::<StorageFeature>() {
            if let Some(backend) = sf.get_backend() {
              if let Some(backup_feature) = self.feature_registry.get("backup") {
                if let Some(bf) = backup_feature.as_any().downcast_ref::<BackupFeature>() {
                  bf.set_storage_backend(backend);
                  emit_log(
                    "info",
                    "squirreldb::backup",
                    "Backup will store to S3 storage",
                  );
                }
              }
            }
          }
        }
      }

      emit_log("info", "squirreldb::backup", "Starting backup service");
      if let Err(e) = self.feature_registry.start("backup", app_state).await {
        tracing::error!("Failed to start backup feature: {}", e);
      } else {
        let location = if self.config.features.storage {
          format!("S3: /{}", self.config.backup.storage_path)
        } else {
          self.config.backup.local_path.clone()
        };
        tracing::info!(
          "SquirrelDB Backup enabled (interval: {}s, retention: {}, storage: {})",
          self.config.backup.interval,
          self.config.backup.retention,
          location
        );
      }
    } else {
      emit_log("warn", "squirreldb::backup", "Backup feature disabled");
      tracing::info!("Backup feature disabled");
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
