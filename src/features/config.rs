use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for all features
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureConfig {
  /// S3-compatible storage feature
  #[serde(default)]
  pub s3: FeatureState,
}

/// State and configuration for a single feature
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureState {
  /// Whether this feature is enabled at startup
  #[serde(default)]
  pub enabled: bool,

  /// Feature-specific configuration as key-value pairs
  #[serde(default)]
  pub config: HashMap<String, serde_json::Value>,
}
