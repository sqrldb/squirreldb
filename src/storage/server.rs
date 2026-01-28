use async_trait::async_trait;
use parking_lot::RwLock;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};

use super::config::StorageConfig;
use super::routes::build_router;
use super::filesystem::LocalFileStorage;
use crate::db::DatabaseBackend;
use crate::features::{AppState, Feature};

/// S3 feature state shared across handlers
pub struct StorageState {
  pub backend: Arc<dyn DatabaseBackend>,
  pub storage: LocalFileStorage,
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
      "S3 feature config updated: port={}, storage_path={}",
      config.port,
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
    let config = if let Ok(Some((_, settings))) = state.backend.get_feature_settings("storage").await {
      let port = settings.get("port").and_then(|v| v.as_u64()).unwrap_or(self.config.read().port as u64) as u16;
      let storage_path = settings.get("storage_path").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| self.config.read().storage_path.clone());
      let region = settings.get("region").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| self.config.read().region.clone());
      let max_object_size = settings.get("max_object_size").and_then(|v| v.as_u64()).unwrap_or(self.config.read().max_object_size);
      let max_part_size = settings.get("max_part_size").and_then(|v| v.as_u64()).unwrap_or(self.config.read().max_part_size);
      let min_part_size = settings.get("min_part_size").and_then(|v| v.as_u64()).unwrap_or(self.config.read().min_part_size);
      StorageConfig {
        port,
        storage_path,
        region,
        max_object_size,
        max_part_size,
        min_part_size,
      }
    } else {
      self.config.read().clone()
    };

    // Update our stored config
    *self.config.write() = config.clone();

    // Initialize storage
    let storage = LocalFileStorage::new(&config.storage_path);
    storage
      .init()
      .await
      .map_err(|e| anyhow::anyhow!("Failed to initialize storage: {}", e))?;

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
}
