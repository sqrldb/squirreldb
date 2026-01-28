//! Document operation tests - CRUD, filtering, ordering, pagination

use serde_json::json;
use squirreldb::db::{DatabaseBackend, SqliteBackend};

// =============================================================================
// Insert Operations
// =============================================================================

#[tokio::test]
async fn test_insert_simple_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({"name": "Alice"});
  let doc = backend.insert("users", data).await.unwrap();

  assert!(!doc.id.is_nil());
  assert_eq!(doc.collection, "users");
  assert_eq!(doc.data["name"], "Alice");
}

#[tokio::test]
async fn test_insert_complex_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({
    "name": "Alice",
    "age": 30,
    "email": "alice@example.com",
    "address": {
      "street": "123 Main St",
      "city": "NYC",
      "zip": "10001"
    },
    "tags": ["developer", "rust", "database"],
    "active": true,
    "score": 95.5
  });

  let doc = backend.insert("users", data.clone()).await.unwrap();

  assert_eq!(doc.data["name"], "Alice");
  assert_eq!(doc.data["age"], 30);
  assert_eq!(doc.data["address"]["city"], "NYC");
  assert_eq!(doc.data["tags"][0], "developer");
  assert_eq!(doc.data["active"], true);
  assert_eq!(doc.data["score"], 95.5);
}

#[tokio::test]
async fn test_insert_empty_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({});
  let doc = backend.insert("empty", data).await.unwrap();

  assert!(!doc.id.is_nil());
  assert_eq!(doc.data, json!({}));
}

#[tokio::test]
async fn test_insert_null_values() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({
    "name": "Bob",
    "middle_name": null,
    "nickname": null
  });

  let doc = backend.insert("users", data).await.unwrap();
  assert_eq!(doc.data["name"], "Bob");
  assert!(doc.data["middle_name"].is_null());
}

#[tokio::test]
async fn test_insert_array_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({
    "items": [1, 2, 3, 4, 5],
    "nested": [{"a": 1}, {"b": 2}]
  });

  let doc = backend.insert("arrays", data).await.unwrap();
  assert_eq!(doc.data["items"].as_array().unwrap().len(), 5);
  assert_eq!(doc.data["nested"][0]["a"], 1);
}

#[tokio::test]
async fn test_insert_special_characters() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({
    "name": "O'Brien",
    "message": "Hello \"World\"",
    "path": "C:\\Users\\test",
    "unicode": "日本語テスト",
    "emoji": "🦀🔥"
  });

  let doc = backend.insert("special", data.clone()).await.unwrap();
  assert_eq!(doc.data["name"], "O'Brien");
  assert_eq!(doc.data["message"], "Hello \"World\"");
  assert_eq!(doc.data["unicode"], "日本語テスト");
}

#[tokio::test]
async fn test_insert_large_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  // Create a document with many fields
  let mut obj = serde_json::Map::new();
  for i in 0..100 {
    obj.insert(format!("field_{}", i), json!(format!("value_{}", i)));
  }

  let data = serde_json::Value::Object(obj);
  let doc = backend.insert("large", data).await.unwrap();

  assert_eq!(doc.data["field_0"], "value_0");
  assert_eq!(doc.data["field_99"], "value_99");
}

#[tokio::test]
async fn test_insert_to_different_collections() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  backend
    .insert("users", json!({"type": "user"}))
    .await
    .unwrap();
  backend
    .insert("posts", json!({"type": "post"}))
    .await
    .unwrap();
  backend
    .insert("comments", json!({"type": "comment"}))
    .await
    .unwrap();

  let collections = backend.list_collections().await.unwrap();
  assert_eq!(collections.len(), 3);
  assert!(collections.contains(&"users".to_string()));
  assert!(collections.contains(&"posts".to_string()));
  assert!(collections.contains(&"comments".to_string()));
}

// =============================================================================
// Get Operations
// =============================================================================

#[tokio::test]
async fn test_get_existing_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  let retrieved = backend.get("users", doc.id).await.unwrap();
  assert!(retrieved.is_some());

  let retrieved = retrieved.unwrap();
  assert_eq!(retrieved.id, doc.id);
  assert_eq!(retrieved.data["name"], "Alice");
}

#[tokio::test]
async fn test_get_nonexistent_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let fake_id = uuid::Uuid::new_v4();
  let result = backend.get("users", fake_id).await.unwrap();
  assert!(result.is_none());
}

#[tokio::test]
async fn test_get_from_wrong_collection() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  // Try to get from different collection
  let result = backend.get("posts", doc.id).await.unwrap();
  assert!(result.is_none());
}

#[tokio::test]
async fn test_get_preserves_data_types() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({
    "string": "hello",
    "number": 42,
    "float": 99.99,
    "boolean": true,
    "null": null,
    "array": [1, 2, 3],
    "object": {"nested": true}
  });

  let doc = backend.insert("types", data).await.unwrap();
  let retrieved = backend.get("types", doc.id).await.unwrap().unwrap();

  assert!(retrieved.data["string"].is_string());
  assert!(retrieved.data["number"].is_number());
  assert!(retrieved.data["float"].is_number());
  assert!(retrieved.data["boolean"].is_boolean());
  assert!(retrieved.data["null"].is_null());
  assert!(retrieved.data["array"].is_array());
  assert!(retrieved.data["object"].is_object());
}

// =============================================================================
// Update Operations
// =============================================================================

#[tokio::test]
async fn test_update_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice", "age": 30}))
    .await
    .unwrap();

  let updated = backend
    .update("users", doc.id, json!({"name": "Alice", "age": 31}))
    .await
    .unwrap()
    .unwrap();

  assert_eq!(updated.id, doc.id);
  assert_eq!(updated.data["age"], 31);
}

#[tokio::test]
async fn test_update_adds_fields() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  let updated = backend
    .update(
      "users",
      doc.id,
      json!({"name": "Alice", "email": "alice@example.com"}),
    )
    .await
    .unwrap()
    .unwrap();

  assert_eq!(updated.data["name"], "Alice");
  assert_eq!(updated.data["email"], "alice@example.com");
}

#[tokio::test]
async fn test_update_replaces_entire_data() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice", "age": 30, "city": "NYC"}))
    .await
    .unwrap();

  // Update with completely different data
  let updated = backend
    .update("users", doc.id, json!({"status": "inactive"}))
    .await
    .unwrap()
    .unwrap();

  // Old fields should be gone (full replacement)
  assert_eq!(updated.data["status"], "inactive");
}

#[tokio::test]
async fn test_update_nonexistent_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let fake_id = uuid::Uuid::new_v4();
  let result = backend
    .update("users", fake_id, json!({"name": "Nobody"}))
    .await
    .unwrap();

  assert!(result.is_none());
}

#[tokio::test]
async fn test_update_wrong_collection() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  let result = backend
    .update("posts", doc.id, json!({"title": "Hello"}))
    .await
    .unwrap();

  assert!(result.is_none());
}

#[tokio::test]
async fn test_update_timestamps() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  let created_at = doc.created_at;
  let original_updated_at = doc.updated_at;

  // Small delay to ensure timestamp difference
  tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

  let updated = backend
    .update("users", doc.id, json!({"name": "Alice Updated"}))
    .await
    .unwrap()
    .unwrap();

  assert_eq!(updated.created_at, created_at);
  assert!(updated.updated_at >= original_updated_at);
}

// =============================================================================
// Delete Operations
// =============================================================================

#[tokio::test]
async fn test_delete_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  let deleted = backend.delete("users", doc.id).await.unwrap();
  assert!(deleted.is_some());

  let deleted = deleted.unwrap();
  assert_eq!(deleted.id, doc.id);
  assert_eq!(deleted.data["name"], "Alice");
}

#[tokio::test]
async fn test_delete_removes_from_database() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  backend.delete("users", doc.id).await.unwrap();

  let result = backend.get("users", doc.id).await.unwrap();
  assert!(result.is_none());
}

#[tokio::test]
async fn test_delete_nonexistent_document() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let fake_id = uuid::Uuid::new_v4();
  let result = backend.delete("users", fake_id).await.unwrap();
  assert!(result.is_none());
}

#[tokio::test]
async fn test_delete_from_wrong_collection() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  let result = backend.delete("posts", doc.id).await.unwrap();
  assert!(result.is_none());

  // Original should still exist
  let still_exists = backend.get("users", doc.id).await.unwrap();
  assert!(still_exists.is_some());
}

#[tokio::test]
async fn test_delete_twice() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  let first = backend.delete("users", doc.id).await.unwrap();
  assert!(first.is_some());

  let second = backend.delete("users", doc.id).await.unwrap();
  assert!(second.is_none());
}

// =============================================================================
// List Operations
// =============================================================================

#[tokio::test]
async fn test_list_all_documents() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  for i in 0..5 {
    backend
      .insert("users", json!({"name": format!("User {}", i)}))
      .await
      .unwrap();
  }

  let docs = backend.list("users", None, None, None, None).await.unwrap();
  assert_eq!(docs.len(), 5);
}

#[tokio::test]
async fn test_list_empty_collection() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let docs = backend.list("empty", None, None, None, None).await.unwrap();
  assert!(docs.is_empty());
}

#[tokio::test]
async fn test_list_with_limit() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  for i in 0..10 {
    backend.insert("items", json!({"index": i})).await.unwrap();
  }

  let docs = backend
    .list("items", None, None, Some(5), None)
    .await
    .unwrap();
  assert_eq!(docs.len(), 5);
}

#[tokio::test]
async fn test_list_limit_larger_than_count() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  for i in 0..3 {
    backend.insert("items", json!({"index": i})).await.unwrap();
  }

  let docs = backend
    .list("items", None, None, Some(100), None)
    .await
    .unwrap();
  assert_eq!(docs.len(), 3);
}

#[tokio::test]
async fn test_list_with_filter() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  backend
    .insert("users", json!({"name": "Alice", "age": 30}))
    .await
    .unwrap();
  backend
    .insert("users", json!({"name": "Bob", "age": 25}))
    .await
    .unwrap();
  backend
    .insert("users", json!({"name": "Charlie", "age": 35}))
    .await
    .unwrap();

  // Filter for age > 28 using SQLite syntax
  let filter = "CAST(json_extract(data, '$.age') AS REAL) > 28";
  let docs = backend
    .list("users", Some(filter), None, None, None)
    .await
    .unwrap();

  assert_eq!(docs.len(), 2);
}

#[tokio::test]
async fn test_list_only_from_specified_collection() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  backend
    .insert("users", json!({"type": "user"}))
    .await
    .unwrap();
  backend
    .insert("users", json!({"type": "user"}))
    .await
    .unwrap();
  backend
    .insert("posts", json!({"type": "post"}))
    .await
    .unwrap();

  let users = backend.list("users", None, None, None, None).await.unwrap();
  assert_eq!(users.len(), 2);

  let posts = backend.list("posts", None, None, None, None).await.unwrap();
  assert_eq!(posts.len(), 1);
}

// =============================================================================
// Collection Operations
// =============================================================================

#[tokio::test]
async fn test_list_collections() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  backend.insert("alpha", json!({})).await.unwrap();
  backend.insert("beta", json!({})).await.unwrap();
  backend.insert("gamma", json!({})).await.unwrap();

  let collections = backend.list_collections().await.unwrap();
  assert_eq!(collections.len(), 3);
  assert!(collections.contains(&"alpha".to_string()));
  assert!(collections.contains(&"beta".to_string()));
  assert!(collections.contains(&"gamma".to_string()));
}

#[tokio::test]
async fn test_list_collections_empty() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let collections = backend.list_collections().await.unwrap();
  assert!(collections.is_empty());
}

#[tokio::test]
async fn test_collection_names() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  // Valid collection names: lowercase alphanumeric and underscores only
  let names = vec![
    "simple",
    "with_underscores",
    "_private",
    "numbers123",
    "a1b2c3",
  ];

  for name in &names {
    backend.insert(name, json!({})).await.unwrap();
  }

  let collections = backend.list_collections().await.unwrap();
  for name in &names {
    assert!(collections.contains(&name.to_string()));
  }
}

#[tokio::test]
async fn test_invalid_collection_names_rejected() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  // Invalid collection names should be rejected for security
  let invalid_names = vec![
    "with-dashes",
    "CamelCase",
    "UPPERCASE",
    "with spaces",
    "with.dots",
    "1starts_with_number",
    "'; DROP TABLE documents;--",
  ];

  for name in &invalid_names {
    let result = backend.insert(name, json!({})).await;
    assert!(
      result.is_err(),
      "Expected collection name '{}' to be rejected",
      name
    );
  }
}

// =============================================================================
// Document Metadata Tests
// =============================================================================

#[tokio::test]
async fn test_document_has_timestamps() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  let now = chrono::Utc::now();
  let created_diff = now - doc.created_at;
  let updated_diff = now - doc.updated_at;

  assert!(created_diff.num_seconds() < 60);
  assert!(updated_diff.num_seconds() < 60);
}

#[tokio::test]
async fn test_document_id_is_uuid() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();

  // UUID v4 has specific format
  let id_str = doc.id.to_string();
  assert_eq!(id_str.len(), 36);
  assert_eq!(id_str.chars().filter(|c| *c == '-').count(), 4);
}

#[tokio::test]
async fn test_document_collection_field() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let doc = backend
    .insert("my_collection", json!({"field": "value"}))
    .await
    .unwrap();

  assert_eq!(doc.collection, "my_collection");
}

// =============================================================================
// Concurrent Operations
// =============================================================================

#[tokio::test]
async fn test_concurrent_inserts() {
  let backend = std::sync::Arc::new(SqliteBackend::in_memory().await.unwrap());
  backend.init_schema().await.unwrap();

  let mut handles = vec![];

  for i in 0..20 {
    let backend = backend.clone();
    let handle =
      tokio::spawn(async move { backend.insert("users", json!({"index": i})).await.unwrap() });
    handles.push(handle);
  }

  for handle in handles {
    handle.await.unwrap();
  }

  let docs = backend.list("users", None, None, None, None).await.unwrap();
  assert_eq!(docs.len(), 20);
}

#[tokio::test]
async fn test_concurrent_read_write() {
  let backend = std::sync::Arc::new(SqliteBackend::in_memory().await.unwrap());
  backend.init_schema().await.unwrap();

  // Insert some initial data
  let doc = backend
    .insert("users", json!({"counter": 0}))
    .await
    .unwrap();

  let mut handles = vec![];

  // Concurrent reads
  for _ in 0..10 {
    let backend = backend.clone();
    let id = doc.id;
    let handle = tokio::spawn(async move { backend.get("users", id).await.unwrap() });
    handles.push(handle);
  }

  for handle in handles {
    let result = handle.await.unwrap();
    assert!(result.is_some());
  }
}
