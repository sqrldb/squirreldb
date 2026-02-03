//! Cache module tests

use squirreldb::cache::config::parse_memory_size;
use squirreldb::cache::resp::{extract_command, parse_resp, RespParser};
use squirreldb::cache::{
  CacheChange, CacheChangeOperation, CacheConfig, CacheEntry, CacheStore, CacheValue,
  EvictionPolicy, InMemoryCacheStore, RespValue,
};
use std::time::Duration;

// =============================================================================
// Config Tests
// =============================================================================

#[test]
fn test_cache_config_defaults() {
  let config = CacheConfig::default();
  assert_eq!(config.port, 6379);
  assert_eq!(config.max_memory, "256mb");
  assert_eq!(config.eviction, EvictionPolicy::Lru);
  assert_eq!(config.default_ttl, 0);
  assert!(!config.snapshot.enabled);
}

#[test]
fn test_parse_memory_size() {
  assert_eq!(parse_memory_size("256mb"), Some(256 * 1024 * 1024));
  assert_eq!(parse_memory_size("1gb"), Some(1024 * 1024 * 1024));
  assert_eq!(parse_memory_size("512kb"), Some(512 * 1024));
  assert_eq!(parse_memory_size("1024b"), Some(1024));
  assert_eq!(parse_memory_size("1024"), Some(1024));
  assert_eq!(parse_memory_size("256 MB"), Some(256 * 1024 * 1024));
  assert_eq!(parse_memory_size("invalid"), None);
}

#[test]
fn test_eviction_policy_parse() {
  assert_eq!(
    "lru".parse::<EvictionPolicy>().unwrap(),
    EvictionPolicy::Lru
  );
  assert_eq!(
    "lfu".parse::<EvictionPolicy>().unwrap(),
    EvictionPolicy::Lfu
  );
  assert_eq!(
    "random".parse::<EvictionPolicy>().unwrap(),
    EvictionPolicy::Random
  );
  assert_eq!(
    "noeviction".parse::<EvictionPolicy>().unwrap(),
    EvictionPolicy::NoEviction
  );
}

// =============================================================================
// RESP Protocol Tests
// =============================================================================

#[test]
fn test_resp_parse_simple_string() {
  let data = b"+OK\r\n";
  let result = parse_resp(data).unwrap();
  assert_eq!(result, RespValue::SimpleString("OK".to_string()));
}

#[test]
fn test_resp_parse_error() {
  let data = b"-ERR unknown command\r\n";
  let result = parse_resp(data).unwrap();
  assert_eq!(result, RespValue::Error("ERR unknown command".to_string()));
}

#[test]
fn test_resp_parse_integer() {
  let data = b":42\r\n";
  let result = parse_resp(data).unwrap();
  assert_eq!(result, RespValue::Integer(42));
}

#[test]
fn test_resp_parse_bulk_string() {
  let data = b"$5\r\nhello\r\n";
  let result = parse_resp(data).unwrap();
  assert_eq!(result, RespValue::BulkString(Some("hello".to_string())));
}

#[test]
fn test_resp_parse_null_bulk() {
  let data = b"$-1\r\n";
  let result = parse_resp(data).unwrap();
  assert_eq!(result, RespValue::BulkString(None));
}

#[test]
fn test_resp_parse_array() {
  let data = b"*2\r\n$3\r\nGET\r\n$3\r\nfoo\r\n";
  let result = parse_resp(data).unwrap();
  assert_eq!(
    result,
    RespValue::Array(Some(vec![
      RespValue::BulkString(Some("GET".to_string())),
      RespValue::BulkString(Some("foo".to_string())),
    ]))
  );
}

#[test]
fn test_resp_encode_roundtrip() {
  let values = vec![
    RespValue::ok(),
    RespValue::error("ERR test"),
    RespValue::integer(123),
    RespValue::bulk("hello"),
    RespValue::null_bulk(),
    RespValue::array(vec![
      RespValue::bulk("SET"),
      RespValue::bulk("key"),
      RespValue::bulk("value"),
    ]),
  ];

  for original in values {
    let encoded = original.encode();
    let parsed = parse_resp(&encoded).unwrap();
    assert_eq!(original, parsed);
  }
}

#[test]
fn test_extract_command() {
  let value = RespValue::Array(Some(vec![
    RespValue::BulkString(Some("set".to_string())),
    RespValue::BulkString(Some("key".to_string())),
    RespValue::BulkString(Some("value".to_string())),
  ]));

  let (cmd, args) = extract_command(&value).unwrap();
  assert_eq!(cmd, "SET");
  assert_eq!(args, vec!["key", "value"]);
}

#[test]
fn test_resp_parser_incremental() {
  let mut parser = RespParser::new();

  // Feed partial data
  parser.feed(b"+O");
  assert!(parser.parse().unwrap().is_none());

  // Feed remaining data
  parser.feed(b"K\r\n");
  let result = parser.parse().unwrap().unwrap();
  assert_eq!(result, RespValue::SimpleString("OK".to_string()));
}

// =============================================================================
// CacheValue Tests
// =============================================================================

#[test]
fn test_cache_value_from_string() {
  let v: CacheValue = "hello".into();
  assert_eq!(v, CacheValue::String("hello".to_string()));

  let v: CacheValue = "123".into();
  assert_eq!(v, CacheValue::Integer(123));

  let v: CacheValue = r#"{"foo": "bar"}"#.into();
  assert!(matches!(v, CacheValue::Json(_)));
}

#[test]
fn test_cache_value_to_resp_string() {
  assert_eq!(
    CacheValue::String("hello".to_string()).to_resp_string(),
    "hello"
  );
  assert_eq!(CacheValue::Integer(42).to_resp_string(), "42");
  assert_eq!(CacheValue::Null.to_resp_string(), "");
}

// =============================================================================
// Store Tests
// =============================================================================

#[tokio::test]
async fn test_store_set_get() {
  let store = InMemoryCacheStore::new(1024 * 1024, EvictionPolicy::Lru, None);

  store
    .set("key1", CacheValue::String("value1".to_string()), None)
    .await
    .unwrap();

  let entry = store.get("key1").await.unwrap();
  assert_eq!(entry.value, CacheValue::String("value1".to_string()));
}

#[tokio::test]
async fn test_store_del_exists() {
  let store = InMemoryCacheStore::new(1024 * 1024, EvictionPolicy::Lru, None);

  store
    .set("key1", CacheValue::String("value1".to_string()), None)
    .await
    .unwrap();
  assert!(store.exists("key1").await);

  assert!(store.delete("key1").await);
  assert!(!store.exists("key1").await);
}

#[tokio::test]
async fn test_store_expire_ttl() {
  let store = InMemoryCacheStore::new(1024 * 1024, EvictionPolicy::Lru, None);

  store
    .set(
      "key1",
      CacheValue::String("value1".to_string()),
      Some(Duration::from_secs(100)),
    )
    .await
    .unwrap();

  let ttl = store.ttl("key1").await.unwrap();
  assert!(ttl > 0 && ttl <= 100);

  // Test persist
  assert!(store.persist("key1").await);
  let ttl = store.ttl("key1").await.unwrap();
  assert_eq!(ttl, -1); // No TTL
}

#[tokio::test]
async fn test_store_incr_decr() {
  let store = InMemoryCacheStore::new(1024 * 1024, EvictionPolicy::Lru, None);

  // Increment non-existent key
  let val = store.incr("counter", 1).await.unwrap();
  assert_eq!(val, 1);

  // Increment again
  let val = store.incr("counter", 5).await.unwrap();
  assert_eq!(val, 6);

  // Decrement
  let val = store.incr("counter", -2).await.unwrap();
  assert_eq!(val, 4);
}

#[tokio::test]
async fn test_store_keys_pattern() {
  let store = InMemoryCacheStore::new(1024 * 1024, EvictionPolicy::Lru, None);

  store
    .set("user:1", CacheValue::String("alice".to_string()), None)
    .await
    .unwrap();
  store
    .set("user:2", CacheValue::String("bob".to_string()), None)
    .await
    .unwrap();
  store
    .set("order:1", CacheValue::String("order1".to_string()), None)
    .await
    .unwrap();

  let keys = store.keys("user:*").await;
  assert_eq!(keys.len(), 2);
  assert!(keys.contains(&"user:1".to_string()));
  assert!(keys.contains(&"user:2".to_string()));

  let keys = store.keys("*").await;
  assert_eq!(keys.len(), 3);
}

#[tokio::test]
async fn test_store_memory_tracking() {
  let store = InMemoryCacheStore::new(1024 * 1024, EvictionPolicy::Lru, None);

  store
    .set("key1", CacheValue::String("value1".to_string()), None)
    .await
    .unwrap();

  let stats = store.info().await;
  assert!(stats.memory_used > 0);
  assert_eq!(stats.keys, 1);
}

#[tokio::test]
async fn test_store_lru_eviction() {
  // Small memory limit to force eviction (50 bytes)
  let store = InMemoryCacheStore::new(50, EvictionPolicy::Lru, None);

  // Insert multiple items with values that are bigger than limit combined
  for i in 0..10 {
    let value = format!("this_is_a_longer_value_for_key_{}", i);
    store
      .set(&format!("key{}", i), CacheValue::String(value), None)
      .await
      .ok();
  }

  let stats = store.info().await;
  // Memory used should be within limit
  assert!(
    stats.memory_used <= 50,
    "memory_used {} > limit 50",
    stats.memory_used
  );
  // Some keys should have been evicted (we can't fit 10 keys with ~40+ byte values in 50 bytes)
  assert!(
    stats.evictions > 0 || stats.keys < 10,
    "evictions={} keys={}",
    stats.evictions,
    stats.keys
  );
}

#[tokio::test]
async fn test_store_ttl_expiration() {
  let store = InMemoryCacheStore::new(1024 * 1024, EvictionPolicy::Lru, None);

  store
    .set(
      "key1",
      CacheValue::String("value1".to_string()),
      Some(Duration::from_millis(50)),
    )
    .await
    .unwrap();

  // Key exists initially
  assert!(store.exists("key1").await);

  // Wait for expiration
  tokio::time::sleep(Duration::from_millis(100)).await;

  // Key should be expired now
  assert!(!store.exists("key1").await);
}

#[tokio::test]
async fn test_store_flush() {
  let store = InMemoryCacheStore::new(1024 * 1024, EvictionPolicy::Lru, None);

  store
    .set("key1", CacheValue::String("value1".to_string()), None)
    .await
    .unwrap();
  store
    .set("key2", CacheValue::String("value2".to_string()), None)
    .await
    .unwrap();

  assert_eq!(store.dbsize().await, 2);

  store.flush().await;

  assert_eq!(store.dbsize().await, 0);
}

// =============================================================================
// Cache Entry Tests
// =============================================================================

#[test]
fn test_cache_entry_ttl() {
  let entry = CacheEntry::new(
    "key".to_string(),
    CacheValue::String("value".to_string()),
    Some(Duration::from_secs(100)),
  );

  assert!(!entry.is_expired());
  assert!(entry.ttl_remaining().is_some());
}

#[test]
fn test_cache_entry_no_ttl() {
  let entry = CacheEntry::new(
    "key".to_string(),
    CacheValue::String("value".to_string()),
    None,
  );

  assert!(!entry.is_expired());
  assert!(entry.ttl_remaining().is_none());
}

// =============================================================================
// Cache Change Tests
// =============================================================================

#[test]
fn test_cache_change_event() {
  let change = CacheChange::new(
    "key".to_string(),
    CacheChangeOperation::Set,
    None,
    Some(CacheValue::String("value".to_string())),
    Some(100),
  );

  assert_eq!(change.key, "key");
  assert_eq!(change.operation, CacheChangeOperation::Set);
  assert!(change.new_value.is_some());
}

#[tokio::test]
async fn test_cache_change_subscription() {
  use squirreldb::cache::CacheSubscriptionManager;
  use uuid::Uuid;

  let manager = CacheSubscriptionManager::new();
  let client_id = Uuid::new_v4();

  // Subscribe to a pattern
  let count = manager.psubscribe(client_id, "user:*");
  assert_eq!(count, 1);

  // Check subscribers
  let subs = manager.get_subscribers("user:123");
  assert_eq!(subs.len(), 1);
  assert_eq!(subs[0].0, client_id);

  // Unsubscribe
  manager.remove_client(client_id);
  let subs = manager.get_subscribers("user:123");
  assert_eq!(subs.len(), 0);
}
