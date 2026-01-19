use squirreldb::server::{BackendType, ServerConfig};

#[test]
fn test_default_config() {
  let config = ServerConfig::default();
  assert_eq!(config.server.host, "0.0.0.0");
  assert_eq!(config.server.ports.http, 8080);
  assert_eq!(config.server.ports.admin, 8081);
  assert_eq!(config.server.ports.tcp, 8082);
  assert_eq!(config.backend, BackendType::Postgres);
  assert_eq!(config.postgres.url, "postgres://localhost/squirreldb");
  assert_eq!(config.postgres.max_connections, 20);
  assert_eq!(config.sqlite.path, "squirreldb.db");
  assert_eq!(config.logging.level, "info");
}

#[test]
fn test_config_address() {
  let config = ServerConfig::default();
  assert_eq!(config.address(), "0.0.0.0:8080");
}

#[test]
fn test_config_from_yaml() {
  let yaml = r#"
server:
  host: "127.0.0.1"
  ports:
    http: 9000
    admin: 9001
    tcp: 9002

postgres:
  url: "postgres://user:pass@localhost/mydb"
  max_connections: 50

logging:
  level: "debug"
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.server.host, "127.0.0.1");
  assert_eq!(config.server.ports.http, 9000);
  assert_eq!(config.server.ports.admin, 9001);
  assert_eq!(config.server.ports.tcp, 9002);
  assert_eq!(config.postgres.url, "postgres://user:pass@localhost/mydb");
  assert_eq!(config.postgres.max_connections, 50);
  assert_eq!(config.logging.level, "debug");
}

#[test]
fn test_config_partial_yaml() {
  let yaml = r#"
server:
  ports:
    http: 3000
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.server.ports.http, 3000);
  // Defaults should be applied
  assert_eq!(config.server.host, "0.0.0.0");
  assert_eq!(config.server.ports.admin, 8081);
  assert_eq!(config.server.ports.tcp, 8082);
  assert_eq!(config.postgres.url, "postgres://localhost/squirreldb");
}

#[test]
fn test_config_sqlite_backend() {
  let yaml = r#"
backend: sqlite

sqlite:
  path: "/tmp/test.db"
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.backend, BackendType::Sqlite);
  assert_eq!(config.sqlite.path, "/tmp/test.db");
}

#[test]
fn test_config_empty_yaml() {
  let yaml = "";
  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.server.ports.http, 8080);
  assert_eq!(config.backend, BackendType::Postgres);
}
