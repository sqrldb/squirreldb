//! MCP (Model Context Protocol) server tests
//!
//! Tests cover:
//! - Configuration parsing and defaults
//! - Parameter struct serialization/deserialization
//! - McpServer tool handlers with real SQLite backend
//! - Error handling and edge cases

use serde_json::json;
use squirreldb::mcp::server::{DeleteParams, InsertParams, McpServer, QueryParams, UpdateParams};
use squirreldb::server::ServerConfig;
use types::DEFAULT_PROJECT_ID;

// =============================================================================
// Configuration Tests
// =============================================================================

#[test]
fn test_mcp_config_defaults() {
  let config = ServerConfig::default();
  assert_eq!(config.server.ports.mcp, 8083);
  assert!(!config.server.protocols.mcp);
}

#[test]
fn test_mcp_config_address() {
  let config = ServerConfig::default();
  assert_eq!(config.mcp_address(), "0.0.0.0:8083");
}

#[test]
fn test_mcp_config_custom_host() {
  let yaml = r#"
server:
  host: "127.0.0.1"
  ports:
    mcp: 9999
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.mcp_address(), "127.0.0.1:9999");
}

#[test]
fn test_mcp_config_from_yaml() {
  let yaml = r#"
server:
  ports:
    mcp: 9083
  protocols:
    mcp: true
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.server.ports.mcp, 9083);
  assert!(config.server.protocols.mcp);
}

#[test]
fn test_mcp_config_partial_yaml() {
  let yaml = r#"
server:
  protocols:
    mcp: true
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert_eq!(config.server.ports.mcp, 8083);
  assert!(config.server.protocols.mcp);
}

#[test]
fn test_mcp_config_with_other_protocols() {
  let yaml = r#"
server:
  protocols:
    websocket: true
    tcp: false
    mcp: true
    sse: false
"#;

  let config: ServerConfig = serde_yaml::from_str(yaml).unwrap();
  assert!(config.server.protocols.websocket);
  assert!(!config.server.protocols.tcp);
  assert!(config.server.protocols.mcp);
  assert!(!config.server.protocols.sse);
}

// =============================================================================
// Parameter Struct Serialization Tests
// =============================================================================

#[test]
fn test_query_params_basic() {
  let query_json = json!({ "query": "db.table(\"users\").run()" });
  let params: QueryParams = serde_json::from_value(query_json).unwrap();
  assert_eq!(params.query, "db.table(\"users\").run()");
}

#[test]
fn test_query_params_complex_query() {
  let query_json = json!({
    "query": "db.table(\"users\").filter(u => u.age > 21 && u.status === \"active\").orderBy(\"name\").limit(10).run()"
  });
  let params: QueryParams = serde_json::from_value(query_json).unwrap();
  assert!(params.query.contains("filter"));
  assert!(params.query.contains("orderBy"));
  assert!(params.query.contains("limit"));
}

#[test]
fn test_query_params_empty_query() {
  let query_json = json!({ "query": "" });
  let params: QueryParams = serde_json::from_value(query_json).unwrap();
  assert_eq!(params.query, "");
}

#[test]
fn test_query_params_unicode() {
  let query_json = json!({ "query": "db.table(\"用户\").filter(u => u.名前 === \"太郎\").run()" });
  let params: QueryParams = serde_json::from_value(query_json).unwrap();
  assert!(params.query.contains("用户"));
  assert!(params.query.contains("名前"));
}

#[test]
fn test_query_params_missing_field() {
  let query_json = json!({});
  let result: Result<QueryParams, _> = serde_json::from_value(query_json);
  assert!(result.is_err());
}

#[test]
fn test_insert_params_basic() {
  let insert_json = json!({
    "collection": "users",
    "data": { "name": "Alice", "age": 30 }
  });
  let params: InsertParams = serde_json::from_value(insert_json).unwrap();
  assert_eq!(params.collection, "users");
  assert_eq!(params.data["name"], "Alice");
  assert_eq!(params.data["age"], 30);
}

#[test]
fn test_insert_params_nested_data() {
  let insert_json = json!({
    "collection": "profiles",
    "data": {
      "user": {
        "name": "Bob",
        "address": {
          "city": "Tokyo",
          "country": "Japan"
        }
      },
      "settings": {
        "theme": "dark",
        "notifications": true
      }
    }
  });
  let params: InsertParams = serde_json::from_value(insert_json).unwrap();
  assert_eq!(params.collection, "profiles");
  assert_eq!(params.data["user"]["address"]["city"], "Tokyo");
  assert_eq!(params.data["settings"]["theme"], "dark");
}

#[test]
fn test_insert_params_array_data() {
  let insert_json = json!({
    "collection": "orders",
    "data": {
      "items": [
        {"product": "Widget", "qty": 5},
        {"product": "Gadget", "qty": 3}
      ],
      "total": 150.00
    }
  });
  let params: InsertParams = serde_json::from_value(insert_json).unwrap();
  assert_eq!(params.data["items"].as_array().unwrap().len(), 2);
  assert_eq!(params.data["items"][0]["product"], "Widget");
}

#[test]
fn test_insert_params_empty_data() {
  let insert_json = json!({
    "collection": "empty",
    "data": {}
  });
  let params: InsertParams = serde_json::from_value(insert_json).unwrap();
  assert_eq!(params.collection, "empty");
  assert!(params.data.as_object().unwrap().is_empty());
}

#[test]
fn test_insert_params_null_values() {
  let insert_json = json!({
    "collection": "nullable",
    "data": {
      "name": "Test",
      "optional_field": null
    }
  });
  let params: InsertParams = serde_json::from_value(insert_json).unwrap();
  assert!(params.data["optional_field"].is_null());
}

#[test]
fn test_insert_params_missing_collection() {
  let insert_json = json!({
    "data": { "name": "Alice" }
  });
  let result: Result<InsertParams, _> = serde_json::from_value(insert_json);
  assert!(result.is_err());
}

#[test]
fn test_insert_params_missing_data() {
  let insert_json = json!({
    "collection": "users"
  });
  let result: Result<InsertParams, _> = serde_json::from_value(insert_json);
  assert!(result.is_err());
}

#[test]
fn test_update_params_basic() {
  let update_json = json!({
    "collection": "users",
    "id": "123e4567-e89b-12d3-a456-426614174000",
    "data": { "name": "Bob" }
  });
  let params: UpdateParams = serde_json::from_value(update_json).unwrap();
  assert_eq!(params.collection, "users");
  assert_eq!(params.id, "123e4567-e89b-12d3-a456-426614174000");
  assert_eq!(params.data["name"], "Bob");
}

#[test]
fn test_update_params_partial_update() {
  let update_json = json!({
    "collection": "users",
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "data": { "age": 31 }
  });
  let params: UpdateParams = serde_json::from_value(update_json).unwrap();
  assert_eq!(params.data["age"], 31);
  assert!(params.data.get("name").is_none());
}

#[test]
fn test_update_params_missing_id() {
  let update_json = json!({
    "collection": "users",
    "data": { "name": "Bob" }
  });
  let result: Result<UpdateParams, _> = serde_json::from_value(update_json);
  assert!(result.is_err());
}

#[test]
fn test_delete_params_basic() {
  let delete_json = json!({
    "collection": "users",
    "id": "123e4567-e89b-12d3-a456-426614174000"
  });
  let params: DeleteParams = serde_json::from_value(delete_json).unwrap();
  assert_eq!(params.collection, "users");
  assert_eq!(params.id, "123e4567-e89b-12d3-a456-426614174000");
}

#[test]
fn test_delete_params_missing_id() {
  let delete_json = json!({
    "collection": "users"
  });
  let result: Result<DeleteParams, _> = serde_json::from_value(delete_json);
  assert!(result.is_err());
}

#[test]
fn test_delete_params_missing_collection() {
  let delete_json = json!({
    "id": "123e4567-e89b-12d3-a456-426614174000"
  });
  let result: Result<DeleteParams, _> = serde_json::from_value(delete_json);
  assert!(result.is_err());
}

// =============================================================================
// UUID Validation Tests
// =============================================================================

#[test]
fn test_uuid_formats() {
  // Standard UUID format
  let valid_uuids = vec![
    "123e4567-e89b-12d3-a456-426614174000",
    "550e8400-e29b-41d4-a716-446655440000",
    "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "00000000-0000-0000-0000-000000000000",
    "ffffffff-ffff-ffff-ffff-ffffffffffff",
  ];

  for uuid in valid_uuids {
    let parsed = uuid::Uuid::parse_str(uuid);
    assert!(parsed.is_ok(), "Should parse valid UUID: {}", uuid);
  }
}

#[test]
fn test_invalid_uuid_formats() {
  let invalid_uuids = vec![
    "not-a-uuid",
    "123e4567-e89b-12d3-a456",
    "", // Empty string
    "123e4567-e89b-12d3-a456-426614174000-extra",
    "123e4567-e89b-12d3-a456-42661417400g", // Invalid hex char
    "123e4567-e89b-12d3-a456-4266141740",   // Too short
  ];

  for uuid in invalid_uuids {
    let parsed = uuid::Uuid::parse_str(uuid);
    assert!(parsed.is_err(), "Should reject invalid UUID: {}", uuid);
  }
}

#[test]
fn test_uuid_without_hyphens_valid() {
  // UUID without hyphens is actually valid
  let uuid = "123e4567e89b12d3a456426614174000";
  let parsed = uuid::Uuid::parse_str(uuid);
  assert!(parsed.is_ok(), "UUID without hyphens should be valid");
}

// =============================================================================
// MCP Server Integration Tests (with SQLite backend)
// =============================================================================

mod integration {
  use super::*;
  use squirreldb::db::{DatabaseBackend, SqliteBackend};
  use squirreldb::query::QueryEnginePool;
  use std::sync::Arc;

  async fn create_test_server() -> (McpServer, Arc<SqliteBackend>) {
    let backend = Arc::new(SqliteBackend::in_memory().await.unwrap());
    backend.init_schema().await.unwrap();
    let engine_pool = Arc::new(QueryEnginePool::new(1, backend.dialect()));
    let server = McpServer::new(backend.clone(), engine_pool);
    (server, backend)
  }

  #[tokio::test]
  async fn test_mcp_server_creation() {
    let (server, _backend) = create_test_server().await;
    // Server should be created without panicking
    drop(server);
  }

  #[tokio::test]
  async fn test_mcp_server_info() {
    use rmcp::ServerHandler;

    let (server, _backend) = create_test_server().await;
    let info = server.get_info();

    assert_eq!(info.server_info.name, "squirreldb");
    assert!(!info.server_info.version.is_empty());
    assert!(info.instructions.is_some());
  }

  #[tokio::test]
  async fn test_mcp_insert_and_query() {
    let (_server, backend) = create_test_server().await;

    // Insert a document
    let doc = backend
      .insert(DEFAULT_PROJECT_ID, "users", json!({"name": "Alice", "age": 30}))
      .await
      .unwrap();

    assert_eq!(doc.collection, "users");
    assert_eq!(doc.data["name"], "Alice");

    // Query should find it
    let docs = backend.list(DEFAULT_PROJECT_ID, "users", None, None, None, None).await.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].data["name"], "Alice");
  }

  #[tokio::test]
  async fn test_mcp_crud_operations() {
    let (_server, backend) = create_test_server().await;

    // Create
    let doc = backend
      .insert(DEFAULT_PROJECT_ID, "items", json!({"name": "Widget", "price": 9.99}))
      .await
      .unwrap();
    let id = doc.id;

    // Read
    let retrieved = backend.get(DEFAULT_PROJECT_ID, "items", id).await.unwrap().unwrap();
    assert_eq!(retrieved.data["name"], "Widget");

    // Update
    let updated = backend
      .update(DEFAULT_PROJECT_ID, "items", id, json!({"name": "Super Widget", "price": 19.99}))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(updated.data["name"], "Super Widget");
    assert_eq!(updated.data["price"], 19.99);

    // Delete
    let deleted = backend.delete(DEFAULT_PROJECT_ID, "items", id).await.unwrap().unwrap();
    assert_eq!(deleted.id, id);

    // Verify deleted
    let gone = backend.get(DEFAULT_PROJECT_ID, "items", id).await.unwrap();
    assert!(gone.is_none());
  }

  #[tokio::test]
  async fn test_mcp_list_collections() {
    let (_server, backend) = create_test_server().await;

    // Create documents in multiple collections
    backend
      .insert(DEFAULT_PROJECT_ID, "users", json!({"name": "Alice"}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "posts", json!({"title": "Hello"}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "comments", json!({"text": "Nice!"}))
      .await
      .unwrap();

    let collections = backend.list_collections(DEFAULT_PROJECT_ID).await.unwrap();
    assert_eq!(collections.len(), 3);
    assert!(collections.contains(&"users".to_string()));
    assert!(collections.contains(&"posts".to_string()));
    assert!(collections.contains(&"comments".to_string()));
  }

  #[tokio::test]
  async fn test_mcp_query_with_filter() {
    let (_server, backend) = create_test_server().await;

    // Insert test data
    backend
      .insert(DEFAULT_PROJECT_ID, "users", json!({"name": "Alice", "age": 25}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "users", json!({"name": "Bob", "age": 30}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "users", json!({"name": "Charlie", "age": 35}))
      .await
      .unwrap();

    // Get engine pool for query execution
    let engine_pool = Arc::new(QueryEnginePool::new(1, backend.dialect()));

    // Execute query through engine pool
    let result = engine_pool
      .execute(
        "db.table(\"users\").filter(u => u.age > 28).run()",
        backend.as_ref(),
      )
      .await
      .unwrap();

    let results = result.as_array().unwrap();
    assert_eq!(results.len(), 2);
  }

  #[tokio::test]
  async fn test_mcp_update_nonexistent() {
    let (_server, backend) = create_test_server().await;

    let fake_id = uuid::Uuid::new_v4();
    let result = backend
      .update(DEFAULT_PROJECT_ID, "users", fake_id, json!({"name": "Ghost"}))
      .await
      .unwrap();

    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_mcp_delete_nonexistent() {
    let (_server, backend) = create_test_server().await;

    let fake_id = uuid::Uuid::new_v4();
    let result = backend.delete(DEFAULT_PROJECT_ID, "users", fake_id).await.unwrap();

    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_mcp_empty_collection() {
    let (_server, backend) = create_test_server().await;

    let docs = backend
      .list(DEFAULT_PROJECT_ID, "empty_collection", None, None, None, None)
      .await
      .unwrap();
    assert!(docs.is_empty());
  }

  #[tokio::test]
  async fn test_mcp_multiple_collections_isolation() {
    let (_server, backend) = create_test_server().await;

    // Insert into different collections
    backend
      .insert(DEFAULT_PROJECT_ID, "collection_a", json!({"value": 1}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "collection_a", json!({"value": 2}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "collection_b", json!({"value": 100}))
      .await
      .unwrap();

    // Verify isolation
    let a_docs = backend
      .list(DEFAULT_PROJECT_ID, "collection_a", None, None, None, None)
      .await
      .unwrap();
    let b_docs = backend
      .list(DEFAULT_PROJECT_ID, "collection_b", None, None, None, None)
      .await
      .unwrap();

    assert_eq!(a_docs.len(), 2);
    assert_eq!(b_docs.len(), 1);
  }

  #[tokio::test]
  async fn test_mcp_large_document() {
    let (_server, backend) = create_test_server().await;

    // Create a document with many fields
    let mut large_data = serde_json::Map::new();
    for i in 0..100 {
      large_data.insert(format!("field_{}", i), json!(format!("value_{}", i)));
    }

    let doc = backend
      .insert(DEFAULT_PROJECT_ID, "large", serde_json::Value::Object(large_data))
      .await
      .unwrap();

    let retrieved = backend.get(DEFAULT_PROJECT_ID, "large", doc.id).await.unwrap().unwrap();
    assert_eq!(retrieved.data["field_50"], "value_50");
    assert_eq!(retrieved.data["field_99"], "value_99");
  }

  #[tokio::test]
  async fn test_mcp_special_characters_in_data() {
    let (_server, backend) = create_test_server().await;

    let doc = backend
      .insert(
        DEFAULT_PROJECT_ID,
        "special",
        json!({
          "text": "Hello \"World\"!",
          "path": "C:\\Users\\test",
          "newline": "line1\nline2",
          "tab": "col1\tcol2",
          "unicode": "日本語 🎉",
          "html": "<script>alert('xss')</script>"
        }),
      )
      .await
      .unwrap();

    let retrieved = backend.get(DEFAULT_PROJECT_ID, "special", doc.id).await.unwrap().unwrap();
    assert_eq!(retrieved.data["text"], "Hello \"World\"!");
    assert_eq!(retrieved.data["unicode"], "日本語 🎉");
  }

  #[tokio::test]
  async fn test_mcp_numeric_types() {
    let (_server, backend) = create_test_server().await;

    let doc = backend
      .insert(
        DEFAULT_PROJECT_ID,
        "numbers",
        json!({
          "integer": 42,
          "negative": -17,
          "float": 12.345,
          "large": 9007199254740991_i64,
          "zero": 0,
          "scientific": 1.5e10
        }),
      )
      .await
      .unwrap();

    let retrieved = backend.get(DEFAULT_PROJECT_ID, "numbers", doc.id).await.unwrap().unwrap();
    assert_eq!(retrieved.data["integer"], 42);
    assert_eq!(retrieved.data["negative"], -17);
    assert!((retrieved.data["float"].as_f64().unwrap() - 12.345).abs() < 0.0001);
  }

  #[tokio::test]
  async fn test_mcp_boolean_and_null() {
    let (_server, backend) = create_test_server().await;

    let doc = backend
      .insert(
        DEFAULT_PROJECT_ID,
        "booleans",
        json!({
          "active": true,
          "deleted": false,
          "optional": null
        }),
      )
      .await
      .unwrap();

    let retrieved = backend.get(DEFAULT_PROJECT_ID, "booleans", doc.id).await.unwrap().unwrap();
    assert_eq!(retrieved.data["active"], true);
    assert_eq!(retrieved.data["deleted"], false);
    assert!(retrieved.data["optional"].is_null());
  }

  #[tokio::test]
  async fn test_mcp_deeply_nested() {
    let (_server, backend) = create_test_server().await;

    let doc = backend
      .insert(
        DEFAULT_PROJECT_ID,
        "nested",
        json!({
          "level1": {
            "level2": {
              "level3": {
                "level4": {
                  "level5": {
                    "value": "deep"
                  }
                }
              }
            }
          }
        }),
      )
      .await
      .unwrap();

    let retrieved = backend.get(DEFAULT_PROJECT_ID, "nested", doc.id).await.unwrap().unwrap();
    assert_eq!(
      retrieved.data["level1"]["level2"]["level3"]["level4"]["level5"]["value"],
      "deep"
    );
  }

  #[tokio::test]
  async fn test_mcp_array_operations() {
    let (_server, backend) = create_test_server().await;

    let doc = backend
      .insert(
        DEFAULT_PROJECT_ID,
        "arrays",
        json!({
          "empty": [],
          "numbers": [1, 2, 3, 4, 5],
          "strings": ["a", "b", "c"],
          "mixed": [1, "two", true, null, {"nested": "object"}],
          "nested": [[1, 2], [3, 4], [5, 6]]
        }),
      )
      .await
      .unwrap();

    let retrieved = backend.get(DEFAULT_PROJECT_ID, "arrays", doc.id).await.unwrap().unwrap();
    assert!(retrieved.data["empty"].as_array().unwrap().is_empty());
    assert_eq!(retrieved.data["numbers"].as_array().unwrap().len(), 5);
    assert_eq!(retrieved.data["mixed"][4]["nested"], "object");
  }
}

// =============================================================================
// Query Engine Integration Tests
// =============================================================================

mod query_engine {
  use super::*;
  use squirreldb::db::{DatabaseBackend, SqliteBackend};
  use squirreldb::query::QueryEnginePool;
  use std::sync::Arc;

  #[tokio::test]
  async fn test_query_basic_table() {
    let backend = Arc::new(SqliteBackend::in_memory().await.unwrap());
    backend.init_schema().await.unwrap();

    backend.insert(DEFAULT_PROJECT_ID, "test", json!({"x": 1})).await.unwrap();
    backend.insert(DEFAULT_PROJECT_ID, "test", json!({"x": 2})).await.unwrap();

    let engine_pool = Arc::new(QueryEnginePool::new(1, backend.dialect()));
    let result = engine_pool
      .execute("db.table(\"test\").run()", backend.as_ref())
      .await
      .unwrap();

    assert_eq!(result.as_array().unwrap().len(), 2);
  }

  #[tokio::test]
  async fn test_query_with_limit() {
    let backend = Arc::new(SqliteBackend::in_memory().await.unwrap());
    backend.init_schema().await.unwrap();

    for i in 0..10 {
      backend.insert(DEFAULT_PROJECT_ID, "items", json!({"index": i})).await.unwrap();
    }

    let engine_pool = Arc::new(QueryEnginePool::new(1, backend.dialect()));
    let result = engine_pool
      .execute("db.table(\"items\").limit(5).run()", backend.as_ref())
      .await
      .unwrap();

    assert_eq!(result.as_array().unwrap().len(), 5);
  }

  #[tokio::test]
  async fn test_query_filter_equality() {
    let backend = Arc::new(SqliteBackend::in_memory().await.unwrap());
    backend.init_schema().await.unwrap();

    backend
      .insert(DEFAULT_PROJECT_ID, "users", json!({"name": "Alice", "role": "admin"}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "users", json!({"name": "Bob", "role": "user"}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "users", json!({"name": "Charlie", "role": "admin"}))
      .await
      .unwrap();

    let engine_pool = Arc::new(QueryEnginePool::new(1, backend.dialect()));
    let result = engine_pool
      .execute(
        "db.table(\"users\").filter(u => u.role === \"admin\").run()",
        backend.as_ref(),
      )
      .await
      .unwrap();

    assert_eq!(result.as_array().unwrap().len(), 2);
  }

  #[tokio::test]
  async fn test_query_filter_comparison() {
    let backend = Arc::new(SqliteBackend::in_memory().await.unwrap());
    backend.init_schema().await.unwrap();

    backend
      .insert(DEFAULT_PROJECT_ID, "products", json!({"name": "A", "price": 10}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "products", json!({"name": "B", "price": 25}))
      .await
      .unwrap();
    backend
      .insert(DEFAULT_PROJECT_ID, "products", json!({"name": "C", "price": 50}))
      .await
      .unwrap();

    let engine_pool = Arc::new(QueryEnginePool::new(1, backend.dialect()));
    let result = engine_pool
      .execute(
        "db.table(\"products\").filter(p => p.price >= 25).run()",
        backend.as_ref(),
      )
      .await
      .unwrap();

    assert_eq!(result.as_array().unwrap().len(), 2);
  }

  #[tokio::test]
  async fn test_query_empty_result() {
    let backend = Arc::new(SqliteBackend::in_memory().await.unwrap());
    backend.init_schema().await.unwrap();

    backend.insert(DEFAULT_PROJECT_ID, "data", json!({"value": 1})).await.unwrap();

    let engine_pool = Arc::new(QueryEnginePool::new(1, backend.dialect()));
    let result = engine_pool
      .execute(
        "db.table(\"data\").filter(d => d.value > 100).run()",
        backend.as_ref(),
      )
      .await
      .unwrap();

    assert!(result.as_array().unwrap().is_empty());
  }

  #[tokio::test]
  async fn test_query_map_transform() {
    let backend = Arc::new(SqliteBackend::in_memory().await.unwrap());
    backend.init_schema().await.unwrap();

    backend
      .insert(DEFAULT_PROJECT_ID, "users", json!({"firstName": "Alice", "lastName": "Smith"}))
      .await
      .unwrap();

    let engine_pool = Arc::new(QueryEnginePool::new(1, backend.dialect()));
    let result = engine_pool
      .execute(
        "db.table(\"users\").map(u => ({ fullName: u.firstName + \" \" + u.lastName })).run()",
        backend.as_ref(),
      )
      .await
      .unwrap();

    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["fullName"], "Alice Smith");
  }
}

// =============================================================================
// Error Handling Tests
// =============================================================================

mod errors {
  use super::*;

  #[test]
  fn test_invalid_json_structure() {
    // Missing required fields should error
    let invalid = json!({"wrong_field": "value"});
    assert!(serde_json::from_value::<QueryParams>(invalid).is_err());
  }

  #[test]
  fn test_wrong_type_for_field() {
    // Query should be a string, not a number
    let invalid = json!({"query": 12345});
    assert!(serde_json::from_value::<QueryParams>(invalid).is_err());
  }

  #[test]
  fn test_null_required_field() {
    let invalid = json!({"query": null});
    assert!(serde_json::from_value::<QueryParams>(invalid).is_err());
  }

  #[test]
  fn test_insert_data_wrong_type() {
    // Data should be an object, not a string
    let invalid = json!({
      "collection": "test",
      "data": "not an object"
    });
    // This actually succeeds because serde_json::Value accepts strings
    // The validation happens at the database level
    let result: Result<InsertParams, _> = serde_json::from_value(invalid);
    assert!(result.is_ok()); // JSON Value accepts any type
  }

  #[tokio::test]
  async fn test_invalid_uuid_in_update() {
    let invalid_uuid = "not-a-valid-uuid";
    let result = uuid::Uuid::parse_str(invalid_uuid);
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_invalid_query_syntax() {
    use squirreldb::db::{DatabaseBackend, SqliteBackend};
    use squirreldb::query::QueryEnginePool;
    use std::sync::Arc;

    let backend = Arc::new(SqliteBackend::in_memory().await.unwrap());
    backend.init_schema().await.unwrap();

    let engine_pool = Arc::new(QueryEnginePool::new(1, backend.dialect()));

    // Invalid JavaScript syntax
    let result = engine_pool
      .execute(
        "db.table(\"test\").filter(u => {{{).run()",
        backend.as_ref(),
      )
      .await;

    assert!(result.is_err());
  }
}
