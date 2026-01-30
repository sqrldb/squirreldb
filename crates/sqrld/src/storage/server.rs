use async_trait::async_trait;
use parking_lot::RwLock;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};

use super::backend::StorageBackend;
use super::config::{ProxyConfig, StorageConfig, StorageMode};
use super::filesystem::LocalFileStorage;
use super::proxy::S3ProxyClient;
use super::routes::build_router;
use crate::db::DatabaseBackend;
use crate::features::{AppState, Feature};

/// S3 feature state shared across handlers
pub struct StorageState {
  pub backend: Arc<dyn DatabaseBackend>,
  pub storage: Arc<dyn StorageBackend>,
  pub config: StorageConfig,
}

/// S3-compatible storage feature
pub struct StorageFeature {
  config: RwLock<StorageConfig>,
  shutdown_tx: RwLock<Option<oneshot::Sender<()>>>,
  running: RwLock<bool>,
}

impl StorageFeature {
  pub fn new(config: StorageConfig) -> Self {
    Self {
      config: RwLock::new(config),
      shutdown_tx: RwLock::new(None),
      running: RwLock::new(false),
    }
  }

  /// Update the feature's configuration (call before start/restart)
  pub fn update_config(&self, config: StorageConfig) {
    tracing::info!(
      "S3 feature config updated: port={}, mode={}, storage_path={}",
      config.port,
      config.mode,
      config.storage_path
    );
    *self.config.write() = config;
  }

  /// Get the current configuration
  pub fn get_config(&self) -> StorageConfig {
    self.config.read().clone()
  }
}

#[async_trait]
impl Feature for StorageFeature {
  fn name(&self) -> &str {
    "storage"
  }

  fn description(&self) -> &str {
    "S3-compatible object storage"
  }

  async fn start(&self, state: Arc<AppState>) -> Result<(), anyhow::Error> {
    if *self.running.read() {
      return Ok(());
    }

    // Load config from database if available, otherwise use current config
    let config =
      if let Ok(Some((_, settings))) = state.backend.get_feature_settings("storage").await {
        let port = settings
          .get("port")
          .and_then(|v| v.as_u64())
          .unwrap_or(self.config.read().port as u64) as u16;
        let storage_path = settings
          .get("storage_path")
          .and_then(|v| v.as_str())
          .map(String::from)
          .unwrap_or_else(|| self.config.read().storage_path.clone());
        let region = settings
          .get("region")
          .and_then(|v| v.as_str())
          .map(String::from)
          .unwrap_or_else(|| self.config.read().region.clone());
        let max_object_size = settings
          .get("max_object_size")
          .and_then(|v| v.as_u64())
          .unwrap_or(self.config.read().max_object_size);
        let max_part_size = settings
          .get("max_part_size")
          .and_then(|v| v.as_u64())
          .unwrap_or(self.config.read().max_part_size);
        let min_part_size = settings
          .get("min_part_size")
          .and_then(|v| v.as_u64())
          .unwrap_or(self.config.read().min_part_size);

        // Parse storage mode
        let mode = settings
          .get("mode")
          .and_then(|v| v.as_str())
          .and_then(|s| s.parse().ok())
          .unwrap_or(self.config.read().mode);

        // Parse proxy config
        let proxy = if mode == StorageMode::Proxy {
          ProxyConfig {
            endpoint: settings
              .get("proxy_endpoint")
              .and_then(|v| v.as_str())
              .map(String::from)
              .unwrap_or_default(),
            access_key_id: settings
              .get("proxy_access_key_id")
              .and_then(|v| v.as_str())
              .map(String::from)
              .unwrap_or_default(),
            secret_access_key: settings
              .get("proxy_secret_access_key")
              .and_then(|v| v.as_str())
              .map(String::from)
              .unwrap_or_default(),
            region: settings
              .get("proxy_region")
              .and_then(|v| v.as_str())
              .map(String::from)
              .unwrap_or_else(|| "us-east-1".to_string()),
            bucket_prefix: settings
              .get("proxy_bucket_prefix")
              .and_then(|v| v.as_str())
              .map(String::from),
            force_path_style: settings
              .get("proxy_force_path_style")
              .and_then(|v| v.as_bool())
              .unwrap_or(false),
          }
        } else {
          self.config.read().proxy.clone()
        };

        StorageConfig {
          port,
          storage_path,
          region,
          max_object_size,
          max_part_size,
          min_part_size,
          mode,
          proxy,
        }
      } else {
        self.config.read().clone()
      };

    // Update our stored config
    *self.config.write() = config.clone();

    // Create storage backend based on mode
    let storage: Arc<dyn StorageBackend> = match config.mode {
      StorageMode::Builtin => {
        let local = LocalFileStorage::new(&config.storage_path);
        local
          .init()
          .await
          .map_err(|e| anyhow::anyhow!("Failed to initialize local storage: {}", e))?;
        Arc::new(local)
      }
      StorageMode::Proxy => {
        if !config.proxy.is_configured() {
          return Err(anyhow::anyhow!(
            "Proxy mode requires access_key_id and secret_access_key"
          ));
        }
        let proxy = S3ProxyClient::new(config.proxy.clone())
          .await
          .map_err(|e| anyhow::anyhow!("Failed to initialize S3 proxy: {}", e))?;
        proxy
          .init()
          .await
          .map_err(|e| anyhow::anyhow!("Failed to connect to S3 proxy: {}", e))?;
        Arc::new(proxy)
      }
    };

    tracing::info!(
      "Storage backend initialized: {} (mode: {})",
      storage.name(),
      config.mode
    );

    // Create S3 state
    let s3_state = Arc::new(StorageState {
      backend: state.backend.clone(),
      storage,
      config: config.clone(),
    });

    // Build router with CORS
    let cors = CorsLayer::new()
      .allow_origin(Any)
      .allow_methods(Any)
      .allow_headers(Any)
      .expose_headers(Any);

    let app = build_router(s3_state).layer(cors);

    // Bind to address
    let addr: SocketAddr = format!("0.0.0.0:{}", config.port)
      .parse()
      .map_err(|e| anyhow::anyhow!("Invalid S3 address: {}", e))?;

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("S3 server listening on {}", addr);

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    *self.shutdown_tx.write() = Some(shutdown_tx);
    *self.running.write() = true;

    // Spawn server task
    tokio::spawn(async move {
      axum::serve(listener, app)
        .with_graceful_shutdown(async {
          let _ = shutdown_rx.await;
        })
        .await
        .ok();
    });

    Ok(())
  }

  async fn stop(&self) -> Result<(), anyhow::Error> {
    if !*self.running.read() {
      return Ok(());
    }

    // Send shutdown signal
    if let Some(tx) = self.shutdown_tx.write().take() {
      let _ = tx.send(());
    }

    *self.running.write() = false;
    tracing::info!("S3 server stopped");
    Ok(())
  }

  fn is_running(&self) -> bool {
    *self.running.read()
  }

  fn as_any(&self) -> &dyn std::any::Any {
    self
  }
}
