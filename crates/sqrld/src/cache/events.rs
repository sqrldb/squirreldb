//! Cache event system for subscriptions

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

use super::entry::CacheValue;

/// Cache change operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheChangeOperation {
  Set,
  Delete,
  Expire,
  Flush,
}

impl std::fmt::Display for CacheChangeOperation {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CacheChangeOperation::Set => write!(f, "set"),
      CacheChangeOperation::Delete => write!(f, "del"),
      CacheChangeOperation::Expire => write!(f, "expired"),
      CacheChangeOperation::Flush => write!(f, "flushdb"),
    }
  }
}

/// A cache change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheChange {
  pub key: String,
  pub operation: CacheChangeOperation,
  pub old_value: Option<CacheValue>,
  pub new_value: Option<CacheValue>,
  pub ttl: Option<i64>,
  pub changed_at: DateTime<Utc>,
}

impl CacheChange {
  pub fn new(
    key: String,
    operation: CacheChangeOperation,
    old_value: Option<CacheValue>,
    new_value: Option<CacheValue>,
    ttl: Option<i64>,
  ) -> Self {
    Self {
      key,
      operation,
      old_value,
      new_value,
      ttl,
      changed_at: Utc::now(),
    }
  }

  /// Format as Redis RESP pubsub message
  pub fn to_pubsub_message(&self, channel: &str) -> String {
    // Format: ["message", channel, "operation key [value]"]
    let payload = match self.operation {
      CacheChangeOperation::Set => {
        format!(
          "set {} {}",
          self.key,
          self
            .new_value
            .as_ref()
            .map(|v| v.to_resp_string())
            .unwrap_or_default()
        )
      }
      CacheChangeOperation::Delete => format!("del {}", self.key),
      CacheChangeOperation::Expire => format!("expired {}", self.key),
      CacheChangeOperation::Flush => "flushdb".to_string(),
    };
    format!(
      "*3\r\n$7\r\nmessage\r\n${}\r\n{}\r\n${}\r\n{}\r\n",
      channel.len(),
      channel,
      payload.len(),
      payload
    )
  }
}

/// Subscription entry
struct Subscription {
  client_id: Uuid,
  pattern: String,
  is_pattern: bool, // PSUBSCRIBE vs SUBSCRIBE
}

/// Manager for cache change subscriptions
pub struct CacheSubscriptionManager {
  subscriptions: RwLock<Vec<Subscription>>,
  client_channels: RwLock<HashMap<Uuid, Vec<String>>>,
  event_tx: broadcast::Sender<(Uuid, CacheChange)>,
}

impl Default for CacheSubscriptionManager {
  fn default() -> Self {
    Self::new()
  }
}

impl CacheSubscriptionManager {
  pub fn new() -> Self {
    let (event_tx, _) = broadcast::channel(1000);
    Self {
      subscriptions: RwLock::new(Vec::new()),
      client_channels: RwLock::new(HashMap::new()),
      event_tx,
    }
  }

  /// Subscribe to a channel (exact match)
  pub fn subscribe(&self, client_id: Uuid, channel: &str) -> usize {
    let mut subs = self.subscriptions.write();
    let mut channels = self.client_channels.write();

    subs.push(Subscription {
      client_id,
      pattern: channel.to_string(),
      is_pattern: false,
    });

    let client_chans = channels.entry(client_id).or_default();
    if !client_chans.contains(&channel.to_string()) {
      client_chans.push(channel.to_string());
    }

    client_chans.len()
  }

  /// Subscribe to a pattern (glob matching)
  pub fn psubscribe(&self, client_id: Uuid, pattern: &str) -> usize {
    let mut subs = self.subscriptions.write();
    let mut channels = self.client_channels.write();

    subs.push(Subscription {
      client_id,
      pattern: pattern.to_string(),
      is_pattern: true,
    });

    let client_chans = channels.entry(client_id).or_default();
    if !client_chans.contains(&pattern.to_string()) {
      client_chans.push(pattern.to_string());
    }

    client_chans.len()
  }

  /// Unsubscribe from a channel
  pub fn unsubscribe(&self, client_id: Uuid, channel: &str) -> usize {
    let mut subs = self.subscriptions.write();
    let mut channels = self.client_channels.write();

    subs.retain(|s| !(s.client_id == client_id && s.pattern == channel && !s.is_pattern));

    if let Some(client_chans) = channels.get_mut(&client_id) {
      client_chans.retain(|c| c != channel);
      client_chans.len()
    } else {
      0
    }
  }

  /// Unsubscribe from a pattern
  pub fn punsubscribe(&self, client_id: Uuid, pattern: &str) -> usize {
    let mut subs = self.subscriptions.write();
    let mut channels = self.client_channels.write();

    subs.retain(|s| !(s.client_id == client_id && s.pattern == pattern && s.is_pattern));

    if let Some(client_chans) = channels.get_mut(&client_id) {
      client_chans.retain(|c| c != pattern);
      client_chans.len()
    } else {
      0
    }
  }

  /// Remove all subscriptions for a client
  pub fn remove_client(&self, client_id: Uuid) {
    let mut subs = self.subscriptions.write();
    let mut channels = self.client_channels.write();

    subs.retain(|s| s.client_id != client_id);
    channels.remove(&client_id);
  }

  /// Get clients subscribed to a key
  pub fn get_subscribers(&self, key: &str) -> Vec<(Uuid, String)> {
    let subs = self.subscriptions.read();
    let mut result = Vec::new();

    for sub in subs.iter() {
      let matches = if sub.is_pattern {
        glob_match(&sub.pattern, key)
      } else {
        // For exact channel subscriptions, check if key starts with channel
        // This allows subscribing to "__keyspace@0__:*" type patterns
        key == sub.pattern || key.starts_with(&format!("{}:", sub.pattern))
      };

      if matches {
        result.push((sub.client_id, sub.pattern.clone()));
      }
    }

    result
  }

  /// Broadcast a change to all matching subscribers
  pub fn broadcast(&self, change: CacheChange) {
    let subscribers = self.get_subscribers(&change.key);
    for (client_id, _pattern) in subscribers {
      let _ = self.event_tx.send((client_id, change.clone()));
    }
  }

  /// Subscribe to event broadcasts
  pub fn subscribe_events(&self) -> broadcast::Receiver<(Uuid, CacheChange)> {
    self.event_tx.subscribe()
  }

  /// Get subscription count for a client
  pub fn client_subscription_count(&self, client_id: Uuid) -> usize {
    let channels = self.client_channels.read();
    channels.get(&client_id).map(|c| c.len()).unwrap_or(0)
  }
}

/// Simple glob pattern matching
fn glob_match(pattern: &str, text: &str) -> bool {
  let mut p_chars = pattern.chars().peekable();
  let mut t_chars = text.chars().peekable();

  while let Some(p) = p_chars.next() {
    match p {
      '*' => {
        // Skip consecutive *
        while p_chars.peek() == Some(&'*') {
          p_chars.next();
        }

        // If * is at end, match everything
        if p_chars.peek().is_none() {
          return true;
        }

        // Try matching rest of pattern at each position
        let remaining_pattern: String = p_chars.collect();
        while t_chars.peek().is_some() {
          let remaining_text: String = t_chars.clone().collect();
          if glob_match(&remaining_pattern, &remaining_text) {
            return true;
          }
          t_chars.next();
        }

        // Try matching empty string
        return glob_match(&remaining_pattern, "");
      }
      '?' => {
        if t_chars.next().is_none() {
          return false;
        }
      }
      c => {
        if t_chars.next() != Some(c) {
          return false;
        }
      }
    }
  }

  t_chars.next().is_none()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_glob_match() {
    assert!(glob_match("*", "anything"));
    assert!(glob_match("foo*", "foobar"));
    assert!(glob_match("*bar", "foobar"));
    assert!(glob_match("foo*bar", "fooXXXbar"));
    assert!(glob_match("f?o", "foo"));
    assert!(!glob_match("f?o", "fooo"));
    assert!(glob_match("user:*", "user:123"));
    assert!(!glob_match("user:*", "order:123"));
  }

  #[test]
  fn test_subscription_manager() {
    let manager = CacheSubscriptionManager::new();
    let client_id = Uuid::new_v4();

    // Test subscribe
    let count = manager.subscribe(client_id, "channel1");
    assert_eq!(count, 1);

    let count = manager.subscribe(client_id, "channel2");
    assert_eq!(count, 2);

    // Test unsubscribe
    let count = manager.unsubscribe(client_id, "channel1");
    assert_eq!(count, 1);

    // Test pattern subscribe
    let count = manager.psubscribe(client_id, "user:*");
    assert_eq!(count, 2);

    // Test get_subscribers
    let subs = manager.get_subscribers("user:123");
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].1, "user:*");
  }
}
