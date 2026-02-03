//! Cache store implementation

use async_trait::async_trait;
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::broadcast;

use super::entry::{CacheEntry, CacheValue, SnapshotEntry};
use super::events::{CacheChange, CacheChangeOperation};

/// Eviction policy when memory limit is reached
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvictionPolicy {
  /// Least Recently Used
  #[default]
  Lru,
  /// Least Frequently Used
  Lfu,
  /// Random eviction
  Random,
  /// Don't evict, return error on memory limit
  NoEviction,
}

impl std::str::FromStr for EvictionPolicy {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "lru" => Ok(EvictionPolicy::Lru),
      "lfu" => Ok(EvictionPolicy::Lfu),
      "random" => Ok(EvictionPolicy::Random),
      "noeviction" | "no-eviction" | "no_eviction" => Ok(EvictionPolicy::NoEviction),
      _ => Err(format!("Unknown eviction policy: {}", s)),
    }
  }
}

impl std::fmt::Display for EvictionPolicy {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      EvictionPolicy::Lru => write!(f, "lru"),
      EvictionPolicy::Lfu => write!(f, "lfu"),
      EvictionPolicy::Random => write!(f, "random"),
      EvictionPolicy::NoEviction => write!(f, "noeviction"),
    }
  }
}

/// Cache statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
  pub keys: usize,
  pub memory_used: usize,
  pub memory_limit: usize,
  pub hits: u64,
  pub misses: u64,
  pub evictions: u64,
  pub expired: u64,
}

impl CacheStats {
  pub fn hit_rate(&self) -> f64 {
    let total = self.hits + self.misses;
    if total == 0 {
      0.0
    } else {
      self.hits as f64 / total as f64
    }
  }
}

/// Cache store trait
#[async_trait]
pub trait CacheStore: Send + Sync {
  async fn get(&self, key: &str) -> Option<CacheEntry>;
  async fn set(
    &self,
    key: &str,
    value: CacheValue,
    ttl: Option<Duration>,
  ) -> Result<(), CacheStoreError>;
  async fn delete(&self, key: &str) -> bool;
  async fn exists(&self, key: &str) -> bool;
  async fn expire(&self, key: &str, ttl: Duration) -> bool;
  async fn persist(&self, key: &str) -> bool;
  async fn ttl(&self, key: &str) -> Option<i64>;
  async fn keys(&self, pattern: &str) -> Vec<String>;
  async fn flush(&self);
  async fn info(&self) -> CacheStats;
  async fn dbsize(&self) -> usize;
}

/// Store operation error
#[derive(Debug, Clone)]
pub enum CacheStoreError {
  OutOfMemory,
  InvalidValue(String),
}

impl std::fmt::Display for CacheStoreError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CacheStoreError::OutOfMemory => write!(f, "OOM: out of memory"),
      CacheStoreError::InvalidValue(msg) => write!(f, "Invalid value: {}", msg),
    }
  }
}

impl std::error::Error for CacheStoreError {}

/// In-memory cache store implementation
pub struct InMemoryCacheStore {
  data: RwLock<HashMap<String, CacheEntry>>,
  memory_used: AtomicUsize,
  memory_limit: usize,
  eviction_policy: EvictionPolicy,
  default_ttl: Option<Duration>,
  hits: AtomicU64,
  misses: AtomicU64,
  evictions: AtomicU64,
  expired: AtomicU64,
  change_tx: broadcast::Sender<CacheChange>,
}

impl InMemoryCacheStore {
  pub fn new(
    memory_limit: usize,
    eviction_policy: EvictionPolicy,
    default_ttl: Option<Duration>,
  ) -> Self {
    let (change_tx, _) = broadcast::channel(1000);
    Self {
      data: RwLock::new(HashMap::new()),
      memory_used: AtomicUsize::new(0),
      memory_limit,
      eviction_policy,
      default_ttl,
      hits: AtomicU64::new(0),
      misses: AtomicU64::new(0),
      evictions: AtomicU64::new(0),
      expired: AtomicU64::new(0),
      change_tx,
    }
  }

  /// Subscribe to cache changes
  pub fn subscribe(&self) -> broadcast::Receiver<CacheChange> {
    self.change_tx.subscribe()
  }

  /// Emit a change event
  fn emit_change(&self, change: CacheChange) {
    let _ = self.change_tx.send(change);
  }

  /// Check and evict expired entries
  pub fn evict_expired(&self) -> usize {
    let mut data = self.data.write();
    let expired_keys: Vec<String> = data
      .iter()
      .filter(|(_, entry)| entry.is_expired())
      .map(|(k, _)| k.clone())
      .collect();

    let count = expired_keys.len();
    for key in expired_keys {
      if let Some(entry) = data.remove(&key) {
        self.memory_used.fetch_sub(entry.size, Ordering::Relaxed);
        self.expired.fetch_add(1, Ordering::Relaxed);
        self.emit_change(CacheChange::new(
          key,
          CacheChangeOperation::Expire,
          Some(entry.value),
          None,
          None,
        ));
      }
    }
    count
  }

  /// Evict entries to free memory (based on policy)
  fn evict_for_space(&self, needed: usize) -> Result<(), CacheStoreError> {
    if self.eviction_policy == EvictionPolicy::NoEviction {
      return Err(CacheStoreError::OutOfMemory);
    }

    let mut data = self.data.write();
    let current_used = self.memory_used.load(Ordering::Relaxed);

    if current_used + needed <= self.memory_limit {
      return Ok(());
    }

    let to_free = (current_used + needed).saturating_sub(self.memory_limit);
    let mut freed = 0usize;

    while freed < to_free && !data.is_empty() {
      let key_to_evict = match self.eviction_policy {
        EvictionPolicy::Lru => {
          // Find least recently accessed
          data
            .iter()
            .min_by_key(|(_, entry)| entry.accessed_at)
            .map(|(k, _)| k.clone())
        }
        EvictionPolicy::Lfu => {
          // Find least frequently used
          data
            .iter()
            .min_by_key(|(_, entry)| entry.access_count)
            .map(|(k, _)| k.clone())
        }
        EvictionPolicy::Random => {
          // Random selection
          let keys: Vec<_> = data.keys().cloned().collect();
          keys.choose(&mut rand::thread_rng()).cloned()
        }
        EvictionPolicy::NoEviction => None,
      };

      if let Some(key) = key_to_evict {
        if let Some(entry) = data.remove(&key) {
          freed += entry.size;
          self.evictions.fetch_add(1, Ordering::Relaxed);
          self.emit_change(CacheChange::new(
            key,
            CacheChangeOperation::Delete,
            Some(entry.value),
            None,
            None,
          ));
        }
      } else {
        break;
      }
    }

    self.memory_used.fetch_sub(freed, Ordering::Relaxed);

    if freed >= to_free {
      Ok(())
    } else {
      Err(CacheStoreError::OutOfMemory)
    }
  }

  /// Get all entries for snapshot
  pub fn snapshot_entries(&self) -> Vec<SnapshotEntry> {
    let data = self.data.read();
    data
      .values()
      .filter(|e| !e.is_expired())
      .map(SnapshotEntry::from)
      .collect()
  }

  /// Restore from snapshot
  pub fn restore_from_snapshot(&self, entries: Vec<SnapshotEntry>) {
    let mut data = self.data.write();
    let mut total_size = 0usize;

    for entry in entries {
      let ttl = entry.ttl_ms.map(Duration::from_millis);
      let cache_entry = CacheEntry::new(entry.key.clone(), entry.value, ttl);
      total_size += cache_entry.size;
      data.insert(entry.key, cache_entry);
    }

    self.memory_used.store(total_size, Ordering::Relaxed);
  }

  /// Increment a key's integer value
  pub async fn incr(&self, key: &str, delta: i64) -> Result<i64, CacheStoreError> {
    let mut data = self.data.write();

    if let Some(entry) = data.get_mut(key) {
      if entry.is_expired() {
        // Treat as new
        let new_entry = CacheEntry::new(key.to_string(), CacheValue::Integer(delta), None);
        self
          .memory_used
          .fetch_add(new_entry.size, Ordering::Relaxed);
        self.memory_used.fetch_sub(entry.size, Ordering::Relaxed);
        *entry = new_entry;
        return Ok(delta);
      }

      match &entry.value {
        CacheValue::Integer(i) => {
          let new_val = i + delta;
          entry.value = CacheValue::Integer(new_val);
          entry.touch();
          Ok(new_val)
        }
        CacheValue::String(s) => {
          if let Ok(i) = s.parse::<i64>() {
            let new_val = i + delta;
            entry.value = CacheValue::Integer(new_val);
            entry.touch();
            Ok(new_val)
          } else {
            Err(CacheStoreError::InvalidValue(
              "value is not an integer".to_string(),
            ))
          }
        }
        _ => Err(CacheStoreError::InvalidValue(
          "value is not an integer".to_string(),
        )),
      }
    } else {
      // Key doesn't exist, create with delta value
      let entry = CacheEntry::new(key.to_string(), CacheValue::Integer(delta), None);
      self.memory_used.fetch_add(entry.size, Ordering::Relaxed);
      data.insert(key.to_string(), entry);
      Ok(delta)
    }
  }
}

#[async_trait]
impl CacheStore for InMemoryCacheStore {
  async fn get(&self, key: &str) -> Option<CacheEntry> {
    let mut data = self.data.write();

    if let Some(entry) = data.get_mut(key) {
      if entry.is_expired() {
        let expired_entry = data.remove(key)?;
        self
          .memory_used
          .fetch_sub(expired_entry.size, Ordering::Relaxed);
        self.expired.fetch_add(1, Ordering::Relaxed);
        self.misses.fetch_add(1, Ordering::Relaxed);
        return None;
      }

      entry.touch();
      self.hits.fetch_add(1, Ordering::Relaxed);
      Some(entry.clone())
    } else {
      self.misses.fetch_add(1, Ordering::Relaxed);
      None
    }
  }

  async fn set(
    &self,
    key: &str,
    value: CacheValue,
    ttl: Option<Duration>,
  ) -> Result<(), CacheStoreError> {
    let effective_ttl = ttl.or(self.default_ttl);
    let new_entry = CacheEntry::new(key.to_string(), value.clone(), effective_ttl);
    let new_size = new_entry.size;

    // Check if we need to evict
    let current_used = self.memory_used.load(Ordering::Relaxed);
    let old_size = {
      let data = self.data.read();
      data.get(key).map(|e| e.size).unwrap_or(0)
    };

    let size_diff = new_size.saturating_sub(old_size);
    if current_used + size_diff > self.memory_limit {
      self.evict_for_space(size_diff)?;
    }

    let mut data = self.data.write();
    let old_value = data.insert(key.to_string(), new_entry);

    if let Some(old) = old_value {
      self.memory_used.fetch_sub(old.size, Ordering::Relaxed);
      self.emit_change(CacheChange::new(
        key.to_string(),
        CacheChangeOperation::Set,
        Some(old.value),
        Some(value),
        effective_ttl.map(|d| d.as_secs() as i64),
      ));
    } else {
      self.emit_change(CacheChange::new(
        key.to_string(),
        CacheChangeOperation::Set,
        None,
        Some(value),
        effective_ttl.map(|d| d.as_secs() as i64),
      ));
    }

    self.memory_used.fetch_add(new_size, Ordering::Relaxed);
    Ok(())
  }

  async fn delete(&self, key: &str) -> bool {
    let mut data = self.data.write();
    if let Some(entry) = data.remove(key) {
      self.memory_used.fetch_sub(entry.size, Ordering::Relaxed);
      self.emit_change(CacheChange::new(
        key.to_string(),
        CacheChangeOperation::Delete,
        Some(entry.value),
        None,
        None,
      ));
      true
    } else {
      false
    }
  }

  async fn exists(&self, key: &str) -> bool {
    let data = self.data.read();
    data.get(key).map(|e| !e.is_expired()).unwrap_or(false)
  }

  async fn expire(&self, key: &str, ttl: Duration) -> bool {
    let mut data = self.data.write();
    if let Some(entry) = data.get_mut(key) {
      if entry.is_expired() {
        return false;
      }
      entry.update_ttl(Some(ttl));
      true
    } else {
      false
    }
  }

  async fn persist(&self, key: &str) -> bool {
    let mut data = self.data.write();
    if let Some(entry) = data.get_mut(key) {
      if entry.ttl.is_some() {
        entry.update_ttl(None);
        true
      } else {
        false
      }
    } else {
      false
    }
  }

  async fn ttl(&self, key: &str) -> Option<i64> {
    let data = self.data.read();
    data.get(key).and_then(|entry| {
      if entry.is_expired() {
        Some(-2) // Key doesn't exist (expired)
      } else {
        entry
          .ttl_remaining()
          .map(|d| d.as_secs() as i64)
          .or(Some(-1))
      }
    })
  }

  async fn keys(&self, pattern: &str) -> Vec<String> {
    let data = self.data.read();

    // Simple glob pattern matching
    if pattern == "*" {
      return data
        .iter()
        .filter(|(_, e)| !e.is_expired())
        .map(|(k, _)| k.clone())
        .collect();
    }

    let regex = glob_to_regex(pattern);
    data
      .iter()
      .filter(|(k, e)| !e.is_expired() && regex.is_match(k))
      .map(|(k, _)| k.clone())
      .collect()
  }

  async fn flush(&self) {
    let mut data = self.data.write();
    let old_data: Vec<_> = data.drain().collect();
    self.memory_used.store(0, Ordering::Relaxed);

    // Emit flush event
    self.emit_change(CacheChange::new(
      "*".to_string(),
      CacheChangeOperation::Flush,
      None,
      None,
      None,
    ));

    drop(old_data);
  }

  async fn info(&self) -> CacheStats {
    let data = self.data.read();
    CacheStats {
      keys: data.iter().filter(|(_, e)| !e.is_expired()).count(),
      memory_used: self.memory_used.load(Ordering::Relaxed),
      memory_limit: self.memory_limit,
      hits: self.hits.load(Ordering::Relaxed),
      misses: self.misses.load(Ordering::Relaxed),
      evictions: self.evictions.load(Ordering::Relaxed),
      expired: self.expired.load(Ordering::Relaxed),
    }
  }

  async fn dbsize(&self) -> usize {
    let data = self.data.read();
    data.iter().filter(|(_, e)| !e.is_expired()).count()
  }
}

/// Convert a glob pattern to a regex
fn glob_to_regex(pattern: &str) -> regex::Regex {
  let mut regex_str = String::with_capacity(pattern.len() * 2);
  regex_str.push('^');

  for c in pattern.chars() {
    match c {
      '*' => regex_str.push_str(".*"),
      '?' => regex_str.push('.'),
      '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\' => {
        regex_str.push('\\');
        regex_str.push(c);
      }
      _ => regex_str.push(c),
    }
  }

  regex_str.push('$');
  regex::Regex::new(&regex_str).unwrap_or_else(|_| regex::Regex::new("^$").unwrap())
}
