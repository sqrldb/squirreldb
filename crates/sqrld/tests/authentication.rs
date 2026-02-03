//! Authentication tests - token hashing, validation, and management

use serde_json::json;
use sha2::{Digest, Sha256};
use squirreldb::db::{DatabaseBackend, SqliteBackend};
use types::DEFAULT_PROJECT_ID;

/// Hash a token using SHA-256 (same implementation as server)
fn hash_token(token: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(token.as_bytes());
  format!("{:x}", hasher.finalize())
}

// =============================================================================
// Token Hashing Tests
// =============================================================================

#[test]
fn test_token_hash_consistency() {
  let token = "sqrl_test1234567890abcdefghij";
  let hash1 = hash_token(token);
  let hash2 = hash_token(token);
  assert_eq!(hash1, hash2, "Same token should produce same hash");
}

#[test]
fn test_token_hash_uniqueness() {
  let token1 = "sqrl_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  let token2 = "sqrl_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
  let hash1 = hash_token(token1);
  let hash2 = hash_token(token2);
  assert_ne!(
    hash1, hash2,
    "Different tokens should produce different hashes"
  );
}

#[test]
fn test_token_hash_length() {
  let token = "sqrl_test1234567890abcdefghij";
  let hash = hash_token(token);
  assert_eq!(hash.len(), 64, "SHA-256 hash should be 64 hex chars");
}

#[test]
fn test_token_hash_is_hex() {
  let token = "sqrl_test1234567890abcdefghij";
  let hash = hash_token(token);
  assert!(
    hash.chars().all(|c| c.is_ascii_hexdigit()),
    "Hash should only contain hex characters"
  );
}

#[test]
fn test_token_hash_case_sensitive() {
  let token1 = "sqrl_ABC123";
  let token2 = "sqrl_abc123";
  let hash1 = hash_token(token1);
  let hash2 = hash_token(token2);
  assert_ne!(hash1, hash2, "Token hashing should be case sensitive");
}

#[test]
fn test_token_hash_empty() {
  let hash = hash_token("");
  assert!(!hash.is_empty(), "Empty token should still produce hash");
  assert_eq!(hash.len(), 64);
}

#[test]
fn test_token_hash_with_special_chars() {
  let token = "sqrl_test-with_special.chars!";
  let hash = hash_token(token);
  assert_eq!(hash.len(), 64);
}

#[test]
fn test_token_hash_unicode() {
  let token = "sqrl_テスト日本語";
  let hash = hash_token(token);
  assert_eq!(hash.len(), 64);
}

// =============================================================================
// Token CRUD Operations
// =============================================================================

#[tokio::test]
async fn test_create_token_generates_uuid() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let token_hash = hash_token("sqrl_testtoken123");
  let info = backend
    .create_token(DEFAULT_PROJECT_ID, "test-token", &token_hash)
    .await
    .unwrap();

  assert!(!info.id.is_nil());
  assert_eq!(info.name, "test-token");
}

#[tokio::test]
async fn test_create_token_with_various_names() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let names = vec![
    "simple",
    "with-dashes",
    "with_underscores",
    "MixedCase",
    "with.dots",
    "numbers123",
    "a",
    "very-long-token-name-that-describes-its-purpose-in-detail",
  ];

  for name in names {
    let hash = hash_token(&format!("sqrl_{}", name));
    let info = backend
      .create_token(DEFAULT_PROJECT_ID, name, &hash)
      .await
      .unwrap();
    assert_eq!(info.name, name);
  }
}

#[tokio::test]
async fn test_create_token_duplicate_name_fails() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let hash1 = hash_token("sqrl_token1");
  let hash2 = hash_token("sqrl_token2");

  backend
    .create_token(DEFAULT_PROJECT_ID, "duplicate", &hash1)
    .await
    .unwrap();
  let result = backend
    .create_token(DEFAULT_PROJECT_ID, "duplicate", &hash2)
    .await;

  assert!(result.is_err(), "Duplicate name should fail");
}

#[tokio::test]
async fn test_create_multiple_tokens() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  for i in 0..10 {
    let name = format!("token-{}", i);
    let hash = hash_token(&format!("sqrl_hash{}", i));
    backend
      .create_token(DEFAULT_PROJECT_ID, &name, &hash)
      .await
      .unwrap();
  }

  let tokens = backend.list_tokens(DEFAULT_PROJECT_ID).await.unwrap();
  assert_eq!(tokens.len(), 10);
}

#[tokio::test]
async fn test_list_tokens_returns_all() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let expected_names: Vec<String> = (0..5).map(|i| format!("token-{}", i)).collect();

  for name in &expected_names {
    let hash = hash_token(&format!("sqrl_{}", name));
    backend
      .create_token(DEFAULT_PROJECT_ID, name, &hash)
      .await
      .unwrap();
  }

  let tokens = backend.list_tokens(DEFAULT_PROJECT_ID).await.unwrap();
  let actual_names: Vec<String> = tokens.iter().map(|t| t.name.clone()).collect();

  for name in &expected_names {
    assert!(actual_names.contains(name));
  }
}

#[tokio::test]
async fn test_list_tokens_empty() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let tokens = backend.list_tokens(DEFAULT_PROJECT_ID).await.unwrap();
  assert!(tokens.is_empty());
}

#[tokio::test]
async fn test_delete_token_removes_from_list() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let hash = hash_token("sqrl_todelete");
  let info = backend
    .create_token(DEFAULT_PROJECT_ID, "to-delete", &hash)
    .await
    .unwrap();

  assert_eq!(
    backend.list_tokens(DEFAULT_PROJECT_ID).await.unwrap().len(),
    1
  );

  let deleted = backend
    .delete_token(DEFAULT_PROJECT_ID, info.id)
    .await
    .unwrap();
  assert!(deleted);

  assert_eq!(
    backend.list_tokens(DEFAULT_PROJECT_ID).await.unwrap().len(),
    0
  );
}

#[tokio::test]
async fn test_delete_nonexistent_token() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let fake_id = uuid::Uuid::new_v4();
  let deleted = backend
    .delete_token(DEFAULT_PROJECT_ID, fake_id)
    .await
    .unwrap();
  assert!(!deleted);
}

#[tokio::test]
async fn test_delete_token_twice() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let hash = hash_token("sqrl_once");
  let info = backend
    .create_token(DEFAULT_PROJECT_ID, "once", &hash)
    .await
    .unwrap();

  let first = backend
    .delete_token(DEFAULT_PROJECT_ID, info.id)
    .await
    .unwrap();
  assert!(first);

  let second = backend
    .delete_token(DEFAULT_PROJECT_ID, info.id)
    .await
    .unwrap();
  assert!(!second);
}

// =============================================================================
// Token Validation Tests
// =============================================================================

#[tokio::test]
async fn test_validate_token_correct_hash() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let token = "sqrl_validtoken123";
  let hash = hash_token(token);

  backend
    .create_token(DEFAULT_PROJECT_ID, "valid", &hash)
    .await
    .unwrap();

  let result = backend.validate_token(&hash).await.unwrap();
  assert!(result.is_some());
}

#[tokio::test]
async fn test_validate_token_wrong_hash() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let token = "sqrl_validtoken123";
  let hash = hash_token(token);

  backend
    .create_token(DEFAULT_PROJECT_ID, "valid", &hash)
    .await
    .unwrap();

  let wrong_hash = hash_token("sqrl_wrongtoken");
  let result = backend.validate_token(&wrong_hash).await.unwrap();
  assert!(result.is_none());
}

#[tokio::test]
async fn test_validate_token_no_tokens_exist() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let hash = hash_token("sqrl_nonexistent");
  let result = backend.validate_token(&hash).await.unwrap();
  assert!(result.is_none());
}

#[tokio::test]
async fn test_validate_token_after_deletion() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let token = "sqrl_temporary";
  let hash = hash_token(token);

  let info = backend
    .create_token(DEFAULT_PROJECT_ID, "temp", &hash)
    .await
    .unwrap();

  // Valid before deletion
  assert!(backend.validate_token(&hash).await.unwrap().is_some());

  // Delete
  backend
    .delete_token(DEFAULT_PROJECT_ID, info.id)
    .await
    .unwrap();

  // Invalid after deletion
  assert!(backend.validate_token(&hash).await.unwrap().is_none());
}

#[tokio::test]
async fn test_validate_multiple_tokens() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let hashes: Vec<String> = (0..5)
    .map(|i| {
      let token = format!("sqrl_token{}", i);
      hash_token(&token)
    })
    .collect();

  for (i, hash) in hashes.iter().enumerate() {
    let name = format!("token-{}", i);
    backend
      .create_token(DEFAULT_PROJECT_ID, &name, hash)
      .await
      .unwrap();
  }

  // All should be valid
  for hash in &hashes {
    assert!(backend.validate_token(hash).await.unwrap().is_some());
  }

  // Random hash should be invalid
  let fake_hash = hash_token("sqrl_fake");
  assert!(backend.validate_token(&fake_hash).await.unwrap().is_none());
}

#[tokio::test]
async fn test_validate_empty_hash() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let result = backend.validate_token("").await.unwrap();
  assert!(result.is_none());
}

// =============================================================================
// Token Info Tests
// =============================================================================

#[tokio::test]
async fn test_token_info_has_created_at() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let hash = hash_token("sqrl_withdate");
  let info = backend
    .create_token(DEFAULT_PROJECT_ID, "dated", &hash)
    .await
    .unwrap();

  // created_at should be recent (within last minute)
  let now = chrono::Utc::now();
  let diff = now - info.created_at;
  assert!(diff.num_seconds() < 60);
}

#[tokio::test]
async fn test_token_info_preserves_name() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let name = "my-special-token-name";
  let hash = hash_token("sqrl_special");
  let info = backend
    .create_token(DEFAULT_PROJECT_ID, name, &hash)
    .await
    .unwrap();

  assert_eq!(info.name, name);

  let tokens = backend.list_tokens(DEFAULT_PROJECT_ID).await.unwrap();
  assert_eq!(tokens[0].name, name);
}

// =============================================================================
// Edge Cases
// =============================================================================

#[tokio::test]
async fn test_token_with_sql_injection_attempt() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  // Attempt SQL injection in name
  let malicious_name = "token'; DROP TABLE api_tokens; --";
  let hash = hash_token("sqrl_safe");

  // Should either fail gracefully or store safely
  let result = backend
    .create_token(DEFAULT_PROJECT_ID, malicious_name, &hash)
    .await;
  if result.is_ok() {
    // If it succeeded, the token should be stored safely
    let tokens = backend.list_tokens(DEFAULT_PROJECT_ID).await.unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name, malicious_name);
  }
}

#[tokio::test]
async fn test_token_concurrent_creation() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let backend = std::sync::Arc::new(backend);

  let mut handles = vec![];

  for i in 0..10 {
    let backend = backend.clone();
    let handle = tokio::spawn(async move {
      let name = format!("concurrent-{}", i);
      let hash = hash_token(&format!("sqrl_concurrent{}", i));
      backend.create_token(DEFAULT_PROJECT_ID, &name, &hash).await
    });
    handles.push(handle);
  }

  for handle in handles {
    handle.await.unwrap().unwrap();
  }

  let tokens = backend.list_tokens(DEFAULT_PROJECT_ID).await.unwrap();
  assert_eq!(tokens.len(), 10);
}

#[tokio::test]
async fn test_token_hash_not_stored_as_plaintext() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let original_token = "sqrl_secrettoken12345";
  let hash = hash_token(original_token);

  backend
    .create_token(DEFAULT_PROJECT_ID, "secret", &hash)
    .await
    .unwrap();

  // The original token should not be recoverable
  // Only the hash is stored and can be validated
  assert!(backend.validate_token(&hash).await.unwrap().is_some());
  assert!(backend
    .validate_token(original_token)
    .await
    .unwrap()
    .is_none());
}

// =============================================================================
// Document Operations with Tokens
// =============================================================================

#[tokio::test]
async fn test_document_ops_work_with_tokens_present() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  // Create some tokens
  backend
    .create_token(DEFAULT_PROJECT_ID, "api-token", &hash_token("sqrl_api"))
    .await
    .unwrap();

  // Document operations should still work
  let doc = backend
    .insert(DEFAULT_PROJECT_ID, "users", json!({"name": "Alice"}))
    .await
    .unwrap();
  assert_eq!(doc.data["name"], "Alice");

  let retrieved = backend
    .get(DEFAULT_PROJECT_ID, "users", doc.id)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(retrieved.id, doc.id);
}
