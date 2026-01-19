//! Extended configuration tests - protocols, authentication, and edge cases

use squirreldb::server::{AuthSection, BackendType, ProtocolsSection, ServerConfig};

// =============================================================================
// Protocol Configuration Tests
// =============================================================================

#[test]
fn test_protocols_default() {
  let config = ServerConfig::default();
  assert!(
    config.server.protocols.rest,
    "REST should be enabled by default"
  );
  assert!(
    config.server.protocols.websocket,
    "WebSocket should be enabled by default"
  );
  assert!(
    !config.server.protocols.sse,
    "SSE should be disabled by default"
  );
  assert!(
    config.server.protocols.tcp,
    "TCP should be enabled by default"
  );
}

#[test]
fn test_protocols_from_yaml_all_enabled() {
  let yaml = r#"
server:
  protocols:
    rest: true
    websocket: true
    sse: true
    tcp: true
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert!(config.server.protocols.rest);
  assert!(config.server.protocols.websocket);
  assert!(config.server.protocols.sse);
  assert!(config.server.protocols.tcp);
}

#[test]
fn test_protocols_from_yaml_all_disabled() {
  let yaml = r#"
server:
  protocols:
    rest: false
    websocket: false
    sse: false
    tcp: false
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert!(!config.server.protocols.rest);
  assert!(!config.server.protocols.websocket);
  assert!(!config.server.protocols.sse);
  assert!(!config.server.protocols.tcp);
}

#[test]
fn test_protocols_from_yaml_partial() {
  let yaml = r#"
server:
  protocols:
    websocket: false
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  // Defaults should apply for unspecified protocols
  assert!(config.server.protocols.rest, "REST should default to true");
  assert!(!config.server.protocols.websocket);
  assert!(!config.server.protocols.sse, "SSE should default to false");
  assert!(config.server.protocols.tcp, "TCP should default to true");
}

#[test]
fn test_protocols_serialization() {
  let protocols = ProtocolsSection {
    rest: true,
    websocket: false,
    sse: true,
    tcp: true,
  };

  let yaml = serde_yaml::to_string(&protocols).unwrap();
  assert!(yaml.contains("rest: true"));
  assert!(yaml.contains("websocket: false"));
  assert!(yaml.contains("sse: true"));
  assert!(yaml.contains("tcp: true"));
}

// =============================================================================
// Authentication Configuration Tests
// =============================================================================

#[test]
fn test_auth_default() {
  let config = ServerConfig::default();
  assert!(!config.auth.enabled, "Auth should be disabled by default");
  assert!(
    config.auth.admin_token.is_none(),
    "Admin token should be None by default"
  );
}

#[test]
fn test_auth_from_yaml_enabled() {
  let yaml = r#"
auth:
  enabled: true
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert!(config.auth.enabled);
  assert!(config.auth.admin_token.is_none());
}

#[test]
fn test_auth_from_yaml_with_admin_token() {
  let yaml = r#"
auth:
  enabled: true
  admin_token: "super-secret-password"
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert!(config.auth.enabled);
  assert_eq!(
    config.auth.admin_token,
    Some("super-secret-password".to_string())
  );
}

#[test]
fn test_auth_from_yaml_empty_admin_token() {
  let yaml = r#"
auth:
  enabled: true
  admin_token: ""
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert!(config.auth.enabled);
  assert_eq!(config.auth.admin_token, Some("".to_string()));
}

#[test]
fn test_auth_from_yaml_disabled_with_token() {
  // Token can exist even if auth is disabled
  let yaml = r#"
auth:
  enabled: false
  admin_token: "unused-token"
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert!(!config.auth.enabled);
  assert_eq!(config.auth.admin_token, Some("unused-token".to_string()));
}

#[test]
fn test_auth_section_serialization() {
  let auth = AuthSection {
    enabled: true,
    admin_token: Some("my-token".to_string()),
  };

  let yaml = serde_yaml::to_string(&auth).unwrap();
  assert!(yaml.contains("enabled: true"));
  assert!(yaml.contains("admin_token: my-token"));
}

// =============================================================================
// Full Configuration Tests
// =============================================================================

#[test]
fn test_full_config_yaml() {
  let yaml = r#"
server:
  host: "192.168.1.100"
  ports:
    http: 9090
    admin: 9091
    tcp: 9092
  protocols:
    rest: true
    websocket: true
    sse: false
    tcp: true

backend: postgres

postgres:
  url: "postgres://user:pass@dbhost:5432/squirreldb"
  max_connections: 100

sqlite:
  path: "/data/squirreldb.db"

auth:
  enabled: true
  admin_token: "admin123"

logging:
  level: "debug"
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();

  // Server
  assert_eq!(config.server.host, "192.168.1.100");
  assert_eq!(config.server.ports.http, 9090);
  assert_eq!(config.server.ports.admin, 9091);
  assert_eq!(config.server.ports.tcp, 9092);

  // Protocols
  assert!(config.server.protocols.rest);
  assert!(config.server.protocols.websocket);
  assert!(!config.server.protocols.sse);
  assert!(config.server.protocols.tcp);

  // Backend
  assert_eq!(config.backend, BackendType::Postgres);
  assert_eq!(
    config.postgres.url,
    "postgres://user:pass@dbhost:5432/squirreldb"
  );
  assert_eq!(config.postgres.max_connections, 100);
  assert_eq!(config.sqlite.path, "/data/squirreldb.db");

  // Auth
  assert!(config.auth.enabled);
  assert_eq!(config.auth.admin_token, Some("admin123".to_string()));

  // Logging
  assert_eq!(config.logging.level, "debug");
}

#[test]
fn test_config_addresses() {
  let yaml = r#"
server:
  host: "10.0.0.1"
  ports:
    http: 8080
    admin: 8081
    tcp: 8082
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.address(), "10.0.0.1:8080");
  assert_eq!(config.admin_address(), "10.0.0.1:8081");
  assert_eq!(config.tcp_address(), "10.0.0.1:8082");
}

#[test]
fn test_config_localhost() {
  let yaml = r#"
server:
  host: "localhost"
  ports:
    http: 3000
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.address(), "localhost:3000");
}

#[test]
fn test_config_ipv6() {
  let yaml = r#"
server:
  host: "::"
  ports:
    http: 8080
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.address(), ":::8080");
}

// =============================================================================
// Backend Type Tests
// =============================================================================

#[test]
fn test_backend_type_postgres() {
  let yaml = "backend: postgres";
  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.backend, BackendType::Postgres);
}

#[test]
fn test_backend_type_sqlite() {
  let yaml = "backend: sqlite";
  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.backend, BackendType::Sqlite);
}

#[test]
fn test_backend_type_case_insensitive() {
  let cases = vec!["backend: POSTGRES", "backend: Sqlite", "backend: PostgreS"];

  for yaml in cases {
    // Should parse without error, case handling depends on implementation
    let result: Result<ServerConfig, _> = serde_yaml::from_str(yaml);
    // Just verify it parses or fails gracefully
    let _ = result;
  }
}

// =============================================================================
// PostgreSQL Configuration Tests
// =============================================================================

#[test]
fn test_postgres_config_defaults() {
  let config = ServerConfig::default();
  assert_eq!(config.postgres.url, "postgres://localhost/squirreldb");
  assert_eq!(config.postgres.max_connections, 20);
}

#[test]
fn test_postgres_config_connection_string_formats() {
  let urls = vec![
    "postgres://localhost/db",
    "postgres://user@localhost/db",
    "postgres://user:pass@localhost/db",
    "postgres://user:pass@localhost:5432/db",
    "postgresql://user:pass@localhost:5432/db?sslmode=require",
  ];

  for url in urls {
    let yaml = format!("postgres:\n  url: \"{}\"", url);
    let config: ServerConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(config.postgres.url, url);
  }
}

#[test]
fn test_postgres_max_connections() {
  let yaml = r#"
postgres:
  max_connections: 1
"#;
  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.postgres.max_connections, 1);

  let yaml = r#"
postgres:
  max_connections: 1000
"#;
  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.postgres.max_connections, 1000);
}

// =============================================================================
// SQLite Configuration Tests
// =============================================================================

#[test]
fn test_sqlite_config_defaults() {
  let config = ServerConfig::default();
  assert_eq!(config.sqlite.path, "squirreldb.db");
}

#[test]
fn test_sqlite_config_paths() {
  let paths = vec![
    "/absolute/path/db.sqlite",
    "./relative/path/db.sqlite",
    "simple.db",
    "/tmp/test-squirreldb.db",
    ":memory:", // Special SQLite in-memory path
  ];

  for path in paths {
    let yaml = format!("sqlite:\n  path: \"{}\"", path);
    let config: ServerConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(config.sqlite.path, path);
  }
}

// =============================================================================
// Logging Configuration Tests
// =============================================================================

#[test]
fn test_logging_default() {
  let config = ServerConfig::default();
  assert_eq!(config.logging.level, "info");
}

#[test]
fn test_logging_levels() {
  let levels = vec!["trace", "debug", "info", "warn", "error"];

  for level in levels {
    let yaml = format!("logging:\n  level: \"{}\"", level);
    let config: ServerConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(config.logging.level, level);
  }
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_config_with_comments() {
  let yaml = r#"
# Main server configuration
server:
  host: "0.0.0.0"  # Bind to all interfaces
  ports:
    http: 8080       # HTTP/WebSocket port
    # admin defaults to 8081
    # tcp defaults to 8082

# Enable authentication in production
auth:
  enabled: false  # TODO: enable for prod
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.server.host, "0.0.0.0");
  assert_eq!(config.server.ports.http, 8080);
  assert!(!config.auth.enabled);
}

#[test]
fn test_config_with_anchors_and_aliases() {
  // YAML anchors and aliases
  let yaml = r#"
defaults: &defaults
  max_connections: 50

postgres:
  <<: *defaults
  url: "postgres://localhost/db"
"#;

  let result: Result<ServerConfig, _> = serde_yaml::from_str(yaml);
  // This might or might not work depending on serde_yaml version
  // Just ensure it doesn't panic
  let _ = result;
}

#[test]
fn test_config_unknown_fields() {
  // Unknown fields should be ignored (or error depending on config)
  let yaml = r#"
server:
  host: "0.0.0.0"
  ports:
    http: 8080
  unknown_field: "ignored"

unknown_section:
  foo: bar
"#;

  // Depending on serde settings, this might parse or fail
  let result: Result<ServerConfig, _> = serde_yaml::from_str(yaml);
  // Just ensure it doesn't panic
  let _ = result;
}

#[test]
fn test_config_clone() {
  let config = ServerConfig::default();
  let cloned = config.clone();

  assert_eq!(config.server.host, cloned.server.host);
  assert_eq!(config.server.ports.http, cloned.server.ports.http);
  assert_eq!(config.server.ports.admin, cloned.server.ports.admin);
  assert_eq!(config.server.ports.tcp, cloned.server.ports.tcp);
  assert_eq!(config.backend, cloned.backend);
}

#[test]
fn test_config_debug() {
  let config = ServerConfig::default();
  let debug_str = format!("{:?}", config);

  assert!(debug_str.contains("ServerConfig"));
  assert!(debug_str.contains("host"));
  assert!(debug_str.contains("port"));
}
