//! Cache feature server implementation

use async_trait::async_trait;
use parking_lot::RwLock;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use uuid::Uuid;

use super::commands::{execute_command, CommandContext};
use super::config::{CacheConfig, CacheMode, CacheProxyConfig};
use super::events::CacheSubscriptionManager;
use super::proxy::RedisProxyClient;
use super::resp::{extract_command, RespParser, RespValue};
use super::snapshot::{run_expiration_task, run_snapshot_task, SnapshotManager};
use super::store::{CacheStore, InMemoryCacheStore};
use crate::features::{AppState, Feature};

/// Cache feature implementation
pub struct CacheFeature {
  config: RwLock<CacheConfig>,
  store: RwLock<Option<Arc<InMemoryCacheStore>>>,
  proxy_store: RwLock<Option<Arc<RedisProxyClient>>>,
  subscriptions: RwLock<Option<Arc<CacheSubscriptionManager>>>,
  shutdown_tx: RwLock<Option<oneshot::Sender<()>>>,
  running: RwLock<bool>,
}

impl CacheFeature {
  pub fn new(config: CacheConfig) -> Self {
    Self {
      config: RwLock::new(config),
      store: RwLock::new(None),
      proxy_store: RwLock::new(None),
      subscriptions: RwLock::new(None),
      shutdown_tx: RwLock::new(None),
      running: RwLock::new(false),
    }
  }

  /// Update the feature's configuration
  pub fn update_config(&self, config: CacheConfig) {
    tracing::info!(
      "Cache config updated: port={}, mode={}, max_memory={}",
      config.port,
      config.mode,
      config.max_memory
    );
    *self.config.write() = config;
  }

  /// Get the current configuration
  pub fn get_config(&self) -> CacheConfig {
    self.config.read().clone()
  }

  /// Get cache store (if running in builtin mode)
  pub fn get_store(&self) -> Option<Arc<InMemoryCacheStore>> {
    self.store.read().clone()
  }

  /// Get proxy store (if running in proxy mode)
  pub fn get_proxy_store(&self) -> Option<Arc<RedisProxyClient>> {
    self.proxy_store.read().clone()
  }

  /// Get the active cache store as a trait object
  pub fn get_active_store(&self) -> Option<Arc<dyn CacheStore>> {
    if let Some(store) = self.store.read().clone() {
      Some(store as Arc<dyn CacheStore>)
    } else if let Some(proxy) = self.proxy_store.read().clone() {
      Some(proxy as Arc<dyn CacheStore>)
    } else {
      None
    }
  }
}

#[async_trait]
impl Feature for CacheFeature {
  fn name(&self) -> &str {
    "caching"
  }

  fn description(&self) -> &str {
    "Redis-compatible in-memory caching"
  }

  async fn start(&self, state: Arc<AppState>) -> Result<(), anyhow::Error> {
    if *self.running.read() {
      return Ok(());
    }

    // Load config from database if available
    let config = if let Ok(Some((_, settings))) = state.backend.get_feature_settings("caching").await
    {
      let port = settings
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(self.config.read().port as u64) as u16;
      let max_memory = settings
        .get("max_memory")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| self.config.read().max_memory.clone());
      let eviction = settings
        .get("eviction")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(self.config.read().eviction);
      let default_ttl = settings
        .get("default_ttl")
        .and_then(|v| v.as_u64())
        .unwrap_or(self.config.read().default_ttl);
      let snapshot_enabled = settings
        .get("snapshot_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(self.config.read().snapshot.enabled);
      let snapshot_path = settings
        .get("snapshot_path")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| self.config.read().snapshot.path.clone());
      let snapshot_interval = settings
        .get("snapshot_interval")
        .and_then(|v| v.as_u64())
        .unwrap_or(self.config.read().snapshot.interval);

      // Parse cache mode
      let mode = settings
        .get("mode")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(self.config.read().mode);

      // Parse proxy config
      let proxy = if mode == CacheMode::Proxy {
        CacheProxyConfig {
          host: settings
            .get("proxy_host")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| "localhost".to_string()),
          port: settings
            .get("proxy_port")
            .and_then(|v| v.as_u64())
            .unwrap_or(6379) as u16,
          password: settings
            .get("proxy_password")
            .and_then(|v| v.as_str())
            .map(String::from),
          database: settings
            .get("proxy_database")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8,
          tls_enabled: settings
            .get("proxy_tls_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        }
      } else {
        self.config.read().proxy.clone()
      };

      CacheConfig {
        port,
        max_memory,
        eviction,
        default_ttl,
        snapshot: super::config::CacheSnapshotConfig {
          enabled: snapshot_enabled,
          path: snapshot_path,
          interval: snapshot_interval,
        },
        mode,
        proxy,
      }
    } else {
      self.config.read().clone()
    };

    // Update stored config
    *self.config.write() = config.clone();

    match config.mode {
      CacheMode::Builtin => {
        self.start_builtin_mode(config).await?;
      }
      CacheMode::Proxy => {
        self.start_proxy_mode(config).await?;
      }
    }

    *self.running.write() = true;
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

    // Save final snapshot if enabled (builtin mode only)
    let config = self.config.read().clone();
    if config.mode == CacheMode::Builtin && config.snapshot.enabled {
      let store = self.store.read().clone();
      if let Some(store) = store {
        let snapshot_manager = SnapshotManager::new(&config.snapshot.path);
        if let Err(e) = snapshot_manager.save(&store).await {
          tracing::error!("Failed to save final snapshot: {}", e);
        }
      }
    }

    // Clear state
    *self.store.write() = None;
    *self.proxy_store.write() = None;
    *self.subscriptions.write() = None;
    *self.running.write() = false;

    tracing::info!("Cache server stopped");
    Ok(())
  }

  fn is_running(&self) -> bool {
    *self.running.read()
  }

  fn as_any(&self) -> &dyn std::any::Any {
    self
  }
}

impl CacheFeature {
  /// Start in builtin (in-memory) mode with RESP server
  async fn start_builtin_mode(&self, config: CacheConfig) -> Result<(), anyhow::Error> {
    // Create store
    let memory_limit = config.max_memory_bytes();
    let default_ttl = if config.default_ttl > 0 {
      Some(Duration::from_secs(config.default_ttl))
    } else {
      None
    };

    let store = Arc::new(InMemoryCacheStore::new(
      memory_limit,
      config.eviction,
      default_ttl,
    ));

    // Load snapshot if enabled
    if config.snapshot.enabled {
      let snapshot_manager = SnapshotManager::new(&config.snapshot.path);
      match snapshot_manager.load(&store).await {
        Ok(count) => {
          if count > 0 {
            tracing::info!("Loaded {} entries from cache snapshot", count);
          }
        }
        Err(e) => {
          tracing::warn!("Failed to load cache snapshot: {}", e);
        }
      }
    }

    // Create subscription manager
    let subscriptions = Arc::new(CacheSubscriptionManager::new());

    // Store references
    *self.store.write() = Some(store.clone());
    *self.subscriptions.write() = Some(subscriptions.clone());

    // Bind TCP listener
    let addr: SocketAddr = format!("0.0.0.0:{}", config.port)
      .parse()
      .map_err(|e| anyhow::anyhow!("Invalid cache address: {}", e))?;

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Cache server listening on {} (builtin mode)", addr);

    // Create shutdown channel
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    *self.shutdown_tx.write() = Some(shutdown_tx);

    // Spawn expiration task
    let expiration_store = store.clone();
    tokio::spawn(async move {
      run_expiration_task(expiration_store, 1).await;
    });

    // Spawn snapshot task if enabled
    if config.snapshot.enabled {
      let snapshot_store = store.clone();
      let snapshot_path = config.snapshot.path.clone();
      let snapshot_interval = config.snapshot.interval;
      tokio::spawn(async move {
        run_snapshot_task(snapshot_store, snapshot_path, snapshot_interval).await;
      });
    }

    // Spawn accept loop
    let accept_store = store.clone();
    let accept_subs = subscriptions.clone();
    tokio::spawn(async move {
      loop {
        tokio::select! {
          result = listener.accept() => {
            match result {
              Ok((socket, addr)) => {
                let client_store = accept_store.clone();
                let client_subs = accept_subs.clone();
                tokio::spawn(async move {
                  if let Err(e) = handle_client(socket, addr, client_store, client_subs).await {
                    tracing::debug!("Client {} error: {}", addr, e);
                  }
                });
              }
              Err(e) => {
                tracing::error!("Accept error: {}", e);
              }
            }
          }
          _ = &mut shutdown_rx => {
            tracing::info!("Cache server shutting down");
            break;
          }
        }
      }
    });

    Ok(())
  }

  /// Start in proxy mode (connect to external Redis)
  async fn start_proxy_mode(&self, config: CacheConfig) -> Result<(), anyhow::Error> {
    if !config.proxy.is_configured() {
      return Err(anyhow::anyhow!("Proxy mode requires host configuration"));
    }

    let proxy = RedisProxyClient::new(config.proxy.clone())
      .await
      .map_err(|e| anyhow::anyhow!("Failed to connect to Redis: {}", e))?;

    // Test connection
    proxy
      .test_connection()
      .await
      .map_err(|e| anyhow::anyhow!("Redis connection test failed: {}", e))?;

    tracing::info!(
      "Cache proxy connected to {}:{} (proxy mode)",
      config.proxy.host,
      config.proxy.port
    );

    *self.proxy_store.write() = Some(Arc::new(proxy));

    // Create shutdown channel (no TCP server in proxy mode)
    let (shutdown_tx, _) = oneshot::channel();
    *self.shutdown_tx.write() = Some(shutdown_tx);

    Ok(())
  }
}

/// Handle a single client connection (builtin mode only)
async fn handle_client(
  mut socket: TcpStream,
  addr: SocketAddr,
  store: Arc<InMemoryCacheStore>,
  subscriptions: Arc<CacheSubscriptionManager>,
) -> Result<(), anyhow::Error> {
  tracing::debug!("Cache client connected: {}", addr);

  let client_id = Uuid::new_v4();
  let mut parser = RespParser::new();
  let mut buf = [0u8; 4096];

  let ctx = CommandContext {
    store,
    subscriptions: subscriptions.clone(),
    client_id,
  };

  loop {
    let n = socket.read(&mut buf).await?;
    if n == 0 {
      break; // Connection closed
    }

    parser.feed(&buf[..n]);

    // Process all complete commands in buffer
    while let Some(value) = parser.parse()? {
      let response = if let Some((cmd, args)) = extract_command(&value) {
        if cmd == "QUIT" {
          socket.write_all(&RespValue::ok().encode()).await?;
          subscriptions.remove_client(client_id);
          return Ok(());
        }
        execute_command(&ctx, &cmd, &args).await
      } else {
        RespValue::error("ERR invalid command format")
      };

      socket.write_all(&response.encode()).await?;
    }
  }

  // Cleanup subscriptions on disconnect
  subscriptions.remove_client(client_id);
  tracing::debug!("Cache client disconnected: {}", addr);

  Ok(())
}
