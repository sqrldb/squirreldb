//! Cache entry types

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// A cached entry with metadata
#[derive(Debug, Clone)]
pub struct CacheEntry {
  pub key: String,
  pub value: CacheValue,
  pub ttl: Option<Duration>,
  pub created_at: Instant,
  pub accessed_at: Instant,
  pub expires_at: Option<Instant>,
  pub access_count: u64,
  /// Size in bytes (approximate)
  pub size: usize,
}

impl CacheEntry {
  pub fn new(key: String, value: CacheValue, ttl: Option<Duration>) -> Self {
    let now = Instant::now();
    let size = value.approximate_size() + key.len();
    let expires_at = ttl.map(|d| now + d);
    Self {
      key,
      value,
      ttl,
      created_at: now,
      accessed_at: now,
      expires_at,
      access_count: 0,
      size,
    }
  }

  pub fn is_expired(&self) -> bool {
    self
      .expires_at
      .map(|exp| Instant::now() > exp)
      .unwrap_or(false)
  }

  pub fn ttl_remaining(&self) -> Option<Duration> {
    self.expires_at.and_then(|exp| {
      let now = Instant::now();
      if now > exp {
        None
      } else {
        Some(exp - now)
      }
    })
  }

  pub fn touch(&mut self) {
    self.accessed_at = Instant::now();
    self.access_count += 1;
  }

  pub fn update_ttl(&mut self, ttl: Option<Duration>) {
    self.ttl = ttl;
    self.expires_at = ttl.map(|d| Instant::now() + d);
  }
}

/// Cache value types (JSON-compatible)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CacheValue {
  #[default]
  Null,
  String(String),
  Integer(i64),
  Json(serde_json::Value),
}

impl CacheValue {
  pub fn approximate_size(&self) -> usize {
    match self {
      CacheValue::Null => 0,
      CacheValue::String(s) => s.len(),
      CacheValue::Integer(_) => 8,
      CacheValue::Json(v) => estimate_json_size(v),
    }
  }

  pub fn as_str(&self) -> Option<&str> {
    match self {
      CacheValue::String(s) => Some(s),
      _ => None,
    }
  }

  pub fn as_i64(&self) -> Option<i64> {
    match self {
      CacheValue::Integer(i) => Some(*i),
      CacheValue::String(s) => s.parse().ok(),
      _ => None,
    }
  }

  pub fn to_resp_string(&self) -> String {
    match self {
      CacheValue::Null => "".to_string(),
      CacheValue::String(s) => s.clone(),
      CacheValue::Integer(i) => i.to_string(),
      CacheValue::Json(v) => v.to_string(),
    }
  }
}

impl From<String> for CacheValue {
  fn from(s: String) -> Self {
    // Try to parse as JSON first
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
      match v {
        serde_json::Value::Null => CacheValue::Null,
        serde_json::Value::Number(n) => {
          if let Some(i) = n.as_i64() {
            CacheValue::Integer(i)
          } else {
            CacheValue::Json(serde_json::Value::Number(n))
          }
        }
        serde_json::Value::String(inner) => CacheValue::String(inner),
        other => CacheValue::Json(other),
      }
    } else {
      CacheValue::String(s)
    }
  }
}

impl From<&str> for CacheValue {
  fn from(s: &str) -> Self {
    CacheValue::from(s.to_string())
  }
}

impl From<i64> for CacheValue {
  fn from(i: i64) -> Self {
    CacheValue::Integer(i)
  }
}

impl From<serde_json::Value> for CacheValue {
  fn from(v: serde_json::Value) -> Self {
    match v {
      serde_json::Value::Null => CacheValue::Null,
      serde_json::Value::Number(n) => {
        if let Some(i) = n.as_i64() {
          CacheValue::Integer(i)
        } else {
          CacheValue::Json(serde_json::Value::Number(n))
        }
      }
      serde_json::Value::String(s) => CacheValue::String(s),
      other => CacheValue::Json(other),
    }
  }
}

fn estimate_json_size(v: &serde_json::Value) -> usize {
  match v {
    serde_json::Value::Null => 4,
    serde_json::Value::Bool(_) => 5,
    serde_json::Value::Number(_) => 8,
    serde_json::Value::String(s) => s.len() + 2,
    serde_json::Value::Array(arr) => arr.iter().map(estimate_json_size).sum::<usize>() + arr.len(),
    serde_json::Value::Object(map) => map
      .iter()
      .map(|(k, v)| k.len() + estimate_json_size(v) + 4)
      .sum(),
  }
}

/// Snapshot-serializable entry (for persistence)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotEntry {
  pub key: String,
  pub value: CacheValue,
  /// TTL remaining in milliseconds (None = no expiry)
  pub ttl_ms: Option<u64>,
}

impl From<&CacheEntry> for SnapshotEntry {
  fn from(entry: &CacheEntry) -> Self {
    Self {
      key: entry.key.clone(),
      value: entry.value.clone(),
      ttl_ms: entry.ttl_remaining().map(|d| d.as_millis() as u64),
    }
  }
}
