mod config;

pub use config::{FeatureConfig, FeatureState};

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::db::DatabaseBackend;
use crate::query::QueryEnginePool;
use crate::server::ServerConfig;

/// Shared state available to features
pub struct AppState {
  pub backend: Arc<dyn DatabaseBackend>,
  pub engine_pool: Arc<QueryEnginePool>,
  pub config: ServerConfig,
}

/// Trait for runtime-toggleable features
#[async_trait]
pub trait Feature: Send + Sync {
  /// Unique name of this feature
  fn name(&self) -> &str;

  /// Human-readable description
  fn description(&self) -> &str {
    ""
  }

  /// Start the feature with given app state
  async fn start(&self, state: Arc<AppState>) -> Result<(), anyhow::Error>;

  /// Stop the feature gracefully
  async fn stop(&self) -> Result<(), anyhow::Error>;

  /// Check if the feature is currently running
  fn is_running(&self) -> bool;

  /// Get as Any for downcasting
  fn as_any(&self) -> &dyn std::any::Any;
}

/// Registry for managing runtime features
pub struct FeatureRegistry {
  features: RwLock<HashMap<String, Arc<dyn Feature>>>,
  states: RwLock<HashMap<String, bool>>,
}

impl Default for FeatureRegistry {
  fn default() -> Self {
    Self::new()
  }
}

impl FeatureRegistry {
  pub fn new() -> Self {
    Self {
      features: RwLock::new(HashMap::new()),
      states: RwLock::new(HashMap::new()),
    }
  }

  /// Register a feature with the registry
  pub fn register(&self, feature: Arc<dyn Feature>) {
    let name = feature.name().to_string();
    self.features.write().insert(name.clone(), feature);
    self.states.write().insert(name, false);
  }

  /// Get a feature by name
  pub fn get(&self, name: &str) -> Option<Arc<dyn Feature>> {
    self.features.read().get(name).cloned()
  }

  /// Start a feature by name
  pub async fn start(&self, name: &str, state: Arc<AppState>) -> Result<(), anyhow::Error> {
    let feature = self
      .features
      .read()
      .get(name)
      .cloned()
      .ok_or_else(|| anyhow::anyhow!("Feature '{}' not found", name))?;

    if feature.is_running() {
      return Ok(());
    }

    feature.start(state).await?;
    self.states.write().insert(name.to_string(), true);
    tracing::info!("Feature '{}' started", name);
    Ok(())
  }

  /// Stop a feature by name
  pub async fn stop(&self, name: &str) -> Result<(), anyhow::Error> {
    let feature = self
      .features
      .read()
      .get(name)
      .cloned()
      .ok_or_else(|| anyhow::anyhow!("Feature '{}' not found", name))?;

    if !feature.is_running() {
      return Ok(());
    }

    feature.stop().await?;
    self.states.write().insert(name.to_string(), false);
    tracing::info!("Feature '{}' stopped", name);
    Ok(())
  }

  /// Restart a feature (stop then start) - useful for applying new settings
  pub async fn restart(&self, name: &str, state: Arc<AppState>) -> Result<(), anyhow::Error> {
    let feature = self
      .features
      .read()
      .get(name)
      .cloned()
      .ok_or_else(|| anyhow::anyhow!("Feature '{}' not found", name))?;

    // Stop if running
    if feature.is_running() {
      feature.stop().await?;
      self.states.write().insert(name.to_string(), false);
      tracing::info!("Feature '{}' stopped for restart", name);
    }

    // Start with potentially new configuration
    feature.start(state).await?;
    self.states.write().insert(name.to_string(), true);
    tracing::info!("Feature '{}' restarted", name);
    Ok(())
  }

  /// Check if a feature is enabled
  pub fn is_enabled(&self, name: &str) -> bool {
    self.states.read().get(name).copied().unwrap_or(false)
  }

  /// List all registered features with their status
  pub fn list(&self) -> Vec<FeatureInfo> {
    let features = self.features.read();
    let states = self.states.read();

    features
      .iter()
      .map(|(name, f)| FeatureInfo {
        name: name.clone(),
        description: f.description().to_string(),
        enabled: states.get(name).copied().unwrap_or(false),
        running: f.is_running(),
      })
      .collect()
  }
}

/// Information about a registered feature
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeatureInfo {
  pub name: String,
  pub description: String,
  pub enabled: bool,
  pub running: bool,
}
