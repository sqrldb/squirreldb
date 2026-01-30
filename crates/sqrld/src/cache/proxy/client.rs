//! Redis proxy client for connecting to external Redis servers

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client, RedisError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::cache::config::CacheProxyConfig;
use crate::cache::entry::{CacheEntry, CacheValue};
use crate::cache::store::{CacheStats, CacheStore, CacheStoreError};

/// Redis proxy client that connects to external Redis servers
pub struct RedisProxyClient {
  connection: ConnectionManager,
  #[allow(dead_code)]
  config: CacheProxyConfig,
  hits: AtomicU64,
  misses: AtomicU64,
}

impl RedisProxyClient {
  /// Create a new Redis proxy client from configuration
  pub async fn new(config: CacheProxyConfig) -> Result<Self, RedisError> {
    let url = config.connection_url();
    let client = Client::open(url)?;
    let connection = ConnectionManager::new(client).await?;

    Ok(Self {
      connection,
      config,
      hits: AtomicU64::new(0),
      misses: AtomicU64::new(0),
    })
  }

  /// Test the connection to Redis
  pub async fn test_connection(&self) -> Result<(), RedisError> {
    let mut conn = self.connection.clone();
    redis::cmd("PING").query_async::<()>(&mut conn).await?;
    Ok(())
  }

  /// Parse a Redis value into our CacheValue enum
  fn parse_value(value: redis::Value) -> Option<CacheValue> {
    match value {
      redis::Value::Nil => None,
      redis::Value::Int(i) => Some(CacheValue::Integer(i)),
      redis::Value::BulkString(bytes) => {
        // Try to parse as string first
        if let Ok(s) = String::from_utf8(bytes.clone()) {
          // Check if it looks like a JSON structure
          if (s.starts_with('[') && s.ends_with(']')) || (s.starts_with('{') && s.ends_with('}')) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&s) {
              return Some(CacheValue::Json(json));
            }
          }
          Some(CacheValue::String(s))
        } else {
          Some(CacheValue::String(format!("{:?}", bytes)))
        }
      }
      redis::Value::Array(items) => {
        let list: Vec<serde_json::Value> = items
          .into_iter()
          .filter_map(|v| match v {
            redis::Value::BulkString(bytes) => {
              String::from_utf8(bytes).ok().map(serde_json::Value::String)
            }
            redis::Value::Int(i) => Some(serde_json::Value::Number(i.into())),
            _ => None,
          })
          .collect();
        Some(CacheValue::Json(serde_json::Value::Array(list)))
      }
      redis::Value::SimpleString(s) => Some(CacheValue::String(s)),
      _ => None,
    }
  }

  /// Serialize a CacheValue for Redis
  fn serialize_value(value: &CacheValue) -> Vec<u8> {
    match value {
      CacheValue::Null => Vec::new(),
      CacheValue::String(s) => s.as_bytes().to_vec(),
      CacheValue::Integer(i) => i.to_string().as_bytes().to_vec(),
      CacheValue::Json(json) => serde_json::to_vec(json).unwrap_or_default(),
    }
  }
}

#[async_trait]
impl CacheStore for RedisProxyClient {
  async fn get(&self, key: &str) -> Option<CacheEntry> {
    let mut conn = self.connection.clone();

    let result: Result<redis::Value, _> = conn.get(key).await;
    match result {
      Ok(value) => {
        if let Some(cache_value) = Self::parse_value(value) {
          self.hits.fetch_add(1, Ordering::Relaxed);
          Some(CacheEntry::new(key.to_string(), cache_value, None))
        } else {
          self.misses.fetch_add(1, Ordering::Relaxed);
          None
        }
      }
      Err(_) => {
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
      }
    }
  }

  async fn set(
    &self,
    key: &str,
    value: CacheValue,
    ttl: Option<Duration>,
  ) -> Result<(), CacheStoreError> {
    let mut conn = self.connection.clone();
    let serialized = Self::serialize_value(&value);

    let result: Result<(), RedisError> = if let Some(duration) = ttl {
      conn.set_ex(key, serialized, duration.as_secs()).await
    } else {
      conn.set(key, serialized).await
    };

    result.map_err(|e| CacheStoreError::InvalidValue(e.to_string()))
  }

  async fn delete(&self, key: &str) -> bool {
    let mut conn = self.connection.clone();
    let result: Result<i64, _> = conn.del(key).await;
    result.map(|n| n > 0).unwrap_or(false)
  }

  async fn exists(&self, key: &str) -> bool {
    let mut conn = self.connection.clone();
    let result: Result<bool, _> = conn.exists(key).await;
    result.unwrap_or(false)
  }

  async fn expire(&self, key: &str, ttl: Duration) -> bool {
    let mut conn = self.connection.clone();
    let result: Result<bool, _> = conn.expire(key, ttl.as_secs() as i64).await;
    result.unwrap_or(false)
  }

  async fn persist(&self, key: &str) -> bool {
    let mut conn = self.connection.clone();
    let result: Result<bool, _> = conn.persist(key).await;
    result.unwrap_or(false)
  }

  async fn ttl(&self, key: &str) -> Option<i64> {
    let mut conn = self.connection.clone();
    let result: Result<i64, _> = conn.ttl(key).await;
    result.ok()
  }

  async fn keys(&self, pattern: &str) -> Vec<String> {
    let mut conn = self.connection.clone();
    let result: Result<Vec<String>, _> = conn.keys(pattern).await;
    result.unwrap_or_default()
  }

  async fn flush(&self) {
    let mut conn = self.connection.clone();
    let _: Result<(), _> = redis::cmd("FLUSHDB").query_async(&mut conn).await;
  }

  async fn info(&self) -> CacheStats {
    let mut conn = self.connection.clone();

    // Get info from Redis
    let info: Result<String, _> = redis::cmd("INFO").query_async(&mut conn).await;
    let dbsize: Result<usize, _> = redis::cmd("DBSIZE").query_async(&mut conn).await;

    let mut memory_used = 0usize;
    let mut memory_limit = 0usize;

    if let Ok(info_str) = info {
      for line in info_str.lines() {
        if let Some(val) = line.strip_prefix("used_memory:") {
          memory_used = val.trim().parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("maxmemory:") {
          memory_limit = val.trim().parse().unwrap_or(0);
        }
      }
    }

    CacheStats {
      keys: dbsize.unwrap_or(0),
      memory_used,
      memory_limit,
      hits: self.hits.load(Ordering::Relaxed),
      misses: self.misses.load(Ordering::Relaxed),
      evictions: 0, // Not tracked locally for proxy
      expired: 0,   // Not tracked locally for proxy
    }
  }

  async fn dbsize(&self) -> usize {
    let mut conn = self.connection.clone();
    let result: Result<usize, _> = redis::cmd("DBSIZE").query_async(&mut conn).await;
    result.unwrap_or(0)
  }
}
