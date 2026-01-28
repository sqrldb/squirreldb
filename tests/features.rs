use squirreldb::features::{FeatureInfo, FeatureRegistry};

// =============================================================================
// Feature Registry Tests
// =============================================================================

#[test]
fn test_feature_registry_new() {
  let registry = FeatureRegistry::new();
  let features = registry.list();
  assert!(features.is_empty());
}

#[test]
fn test_feature_registry_is_enabled_nonexistent() {
  let registry = FeatureRegistry::new();
  assert!(!registry.is_enabled("nonexistent"));
}

#[test]
fn test_feature_info_fields() {
  let info = FeatureInfo {
    name: "my-feature".to_string(),
    description: "My awesome feature".to_string(),
    enabled: true,
    running: false,
  };

  assert_eq!(info.name, "my-feature");
  assert_eq!(info.description, "My awesome feature");
  assert!(info.enabled);
  assert!(!info.running);
}

#[test]
fn test_feature_info_serialize() {
  let info = FeatureInfo {
    name: "test".to_string(),
    description: "Test feature".to_string(),
    enabled: true,
    running: true,
  };

  let json = serde_json::to_string(&info).unwrap();
  assert!(json.contains("\"name\":\"test\""));
  assert!(json.contains("\"description\":\"Test feature\""));
  assert!(json.contains("\"enabled\":true"));
  assert!(json.contains("\"running\":true"));
}

#[test]
fn test_feature_info_clone() {
  let info = FeatureInfo {
    name: "clone-test".to_string(),
    description: "Testing clone".to_string(),
    enabled: false,
    running: false,
  };

  let cloned = info.clone();
  assert_eq!(cloned.name, info.name);
  assert_eq!(cloned.description, info.description);
  assert_eq!(cloned.enabled, info.enabled);
  assert_eq!(cloned.running, info.running);
}
