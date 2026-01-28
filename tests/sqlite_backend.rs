use serde_json::json;
use squirreldb::db::{DatabaseBackend, SqlDialect, SqliteBackend};

#[tokio::test]
async fn test_sqlite_backend_init_schema() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();
  // Should not panic on re-init
  backend.init_schema().await.unwrap();
}

#[tokio::test]
async fn test_sqlite_backend_dialect() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  assert_eq!(backend.dialect(), SqlDialect::Sqlite);
}

#[tokio::test]
async fn test_sqlite_backend_insert_and_get() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({"name": "Alice", "age": 30});
  let doc = backend.insert("users", data.clone()).await.unwrap();

  assert_eq!(doc.collection, "users");
  assert_eq!(doc.data["name"], "Alice");
  assert_eq!(doc.data["age"], 30);

  let retrieved = backend.get("users", doc.id).await.unwrap().unwrap();
  assert_eq!(retrieved.id, doc.id);
  assert_eq!(retrieved.data["name"], "Alice");
}

#[tokio::test]
async fn test_sqlite_backend_update() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({"name": "Bob", "age": 25});
  let doc = backend.insert("users", data).await.unwrap();

  let updated_data = json!({"name": "Bob", "age": 26});
  let updated = backend
    .update("users", doc.id, updated_data)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(updated.data["age"], 26);
}

#[tokio::test]
async fn test_sqlite_backend_delete() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let data = json!({"name": "Charlie"});
  let doc = backend.insert("users", data).await.unwrap();

  let deleted = backend.delete("users", doc.id).await.unwrap().unwrap();
  assert_eq!(deleted.id, doc.id);

  let retrieved = backend.get("users", doc.id).await.unwrap();
  assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_sqlite_backend_list() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();
  backend
    .insert("users", json!({"name": "Bob"}))
    .await
    .unwrap();
  backend
    .insert("posts", json!({"title": "Hello"}))
    .await
    .unwrap();

  let users = backend.list("users", None, None, None, None).await.unwrap();
  assert_eq!(users.len(), 2);

  let posts = backend.list("posts", None, None, None, None).await.unwrap();
  assert_eq!(posts.len(), 1);
}

#[tokio::test]
async fn test_sqlite_backend_list_with_limit() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  for i in 0..10 {
    backend.insert("items", json!({"index": i})).await.unwrap();
  }

  let items = backend.list("items", None, None, Some(5), None).await.unwrap();
  assert_eq!(items.len(), 5);
}

#[tokio::test]
async fn test_sqlite_backend_list_collections() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  backend
    .insert("users", json!({"name": "Alice"}))
    .await
    .unwrap();
  backend
    .insert("posts", json!({"title": "Hello"}))
    .await
    .unwrap();
  backend
    .insert("comments", json!({"text": "Nice!"}))
    .await
    .unwrap();

  let collections = backend.list_collections().await.unwrap();
  assert_eq!(collections.len(), 3);
  assert!(collections.contains(&"users".to_string()));
  assert!(collections.contains(&"posts".to_string()));
  assert!(collections.contains(&"comments".to_string()));
}

#[tokio::test]
async fn test_sqlite_backend_filter() {
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

  // Filter by age > 28 using SQLite JSON syntax
  let filter = "CAST(json_extract(data, '$.age') AS REAL) > 28";
  let users = backend
    .list("users", Some(filter), None, None, None)
    .await
    .unwrap();
  assert_eq!(users.len(), 2);
}

// =============================================================================
// Token Management Tests
// =============================================================================

#[tokio::test]
async fn test_sqlite_backend_create_token() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let token_hash = "abc123def456";
  let token_info = backend
    .create_token("test-token", token_hash)
    .await
    .unwrap();

  assert_eq!(token_info.name, "test-token");
  assert!(!token_info.id.is_nil());
}

#[tokio::test]
async fn test_sqlite_backend_list_tokens() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  // Create some tokens
  backend.create_token("token-1", "hash1").await.unwrap();
  backend.create_token("token-2", "hash2").await.unwrap();
  backend.create_token("token-3", "hash3").await.unwrap();

  let tokens = backend.list_tokens().await.unwrap();
  assert_eq!(tokens.len(), 3);

  let names: Vec<&str> = tokens.iter().map(|t| t.name.as_str()).collect();
  assert!(names.contains(&"token-1"));
  assert!(names.contains(&"token-2"));
  assert!(names.contains(&"token-3"));
}

#[tokio::test]
async fn test_sqlite_backend_delete_token() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let token_info = backend.create_token("to-delete", "hash123").await.unwrap();

  // Verify it exists
  let tokens = backend.list_tokens().await.unwrap();
  assert_eq!(tokens.len(), 1);

  // Delete it
  let deleted = backend.delete_token(token_info.id).await.unwrap();
  assert!(deleted);

  // Verify it's gone
  let tokens = backend.list_tokens().await.unwrap();
  assert_eq!(tokens.len(), 0);

  // Delete non-existent should return false
  let deleted_again = backend.delete_token(token_info.id).await.unwrap();
  assert!(!deleted_again);
}

#[tokio::test]
async fn test_sqlite_backend_validate_token() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let valid_hash = "valid_token_hash_123";
  backend
    .create_token("valid-token", valid_hash)
    .await
    .unwrap();

  // Valid token should return true
  let is_valid = backend.validate_token(valid_hash).await.unwrap();
  assert!(is_valid);

  // Invalid token should return false
  let is_invalid = backend.validate_token("invalid_hash").await.unwrap();
  assert!(!is_invalid);
}

#[tokio::test]
async fn test_sqlite_backend_token_name_unique() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  // Create first token
  backend.create_token("unique-name", "hash1").await.unwrap();

  // Creating token with same name should fail
  let result = backend.create_token("unique-name", "hash2").await;
  assert!(result.is_err());
}
