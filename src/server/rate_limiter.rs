//! Rate limiting and connection management.
//!
//! Provides:
//! - Connection limits per IP address
//! - Request rate limiting using token bucket algorithm
//! - Concurrent query limiting per client

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use uuid::Uuid;

use super::config::LimitsSection;

/// Rate limiter for managing connections and request rates.
pub struct RateLimiter {
  config: LimitsSection,
  /// Connections per IP: IP -> count
  connections: RwLock<HashMap<IpAddr, u32>>,
  /// Token buckets per IP: IP -> TokenBucket
  buckets: RwLock<HashMap<IpAddr, TokenBucket>>,
  /// Concurrent queries per client: client_id -> count
  concurrent_queries: RwLock<HashMap<Uuid, Arc<AtomicU32>>>,
}

/// Token bucket for rate limiting.
struct TokenBucket {
  tokens: f64,
  last_update: Instant,
  rate: f64,     // tokens per second
  capacity: f64, // max tokens (burst size)
}

impl TokenBucket {
  fn new(rate: u32, capacity: u32) -> Self {
    Self {
      tokens: capacity as f64,
      last_update: Instant::now(),
      rate: rate as f64,
      capacity: capacity as f64,
    }
  }

  /// Try to consume a token. Returns true if successful.
  fn try_consume(&mut self) -> bool {
    self.refill();
    if self.tokens >= 1.0 {
      self.tokens -= 1.0;
      true
    } else {
      false
    }
  }

  /// Refill tokens based on elapsed time.
  fn refill(&mut self) {
    let now = Instant::now();
    let elapsed = now.duration_since(self.last_update).as_secs_f64();
    self.tokens = (self.tokens + elapsed * self.rate).min(self.capacity);
    self.last_update = now;
  }
}

impl RateLimiter {
  pub fn new(config: LimitsSection) -> Self {
    Self {
      config,
      connections: RwLock::new(HashMap::new()),
      buckets: RwLock::new(HashMap::new()),
      concurrent_queries: RwLock::new(HashMap::new()),
    }
  }

  /// Check if a new connection from this IP is allowed.
  /// If allowed, increments the connection count and returns Ok.
  /// If not allowed, returns Err with a message.
  pub fn check_connection(&self, ip: IpAddr) -> Result<(), RateLimitError> {
    if self.config.max_connections_per_ip == 0 {
      return Ok(()); // Unlimited
    }

    let mut conns = self.connections.write();
    let count = conns.entry(ip).or_insert(0);

    if *count >= self.config.max_connections_per_ip {
      return Err(RateLimitError::TooManyConnections {
        ip,
        limit: self.config.max_connections_per_ip,
      });
    }

    *count += 1;
    Ok(())
  }

  /// Release a connection slot for an IP.
  pub fn release_connection(&self, ip: IpAddr) {
    let mut conns = self.connections.write();
    if let Some(count) = conns.get_mut(&ip) {
      *count = count.saturating_sub(1);
      if *count == 0 {
        conns.remove(&ip);
      }
    }
  }

  /// Check if a request is allowed under rate limiting.
  /// Returns Ok if allowed, Err if rate limited.
  pub fn check_request(&self, ip: IpAddr) -> Result<(), RateLimitError> {
    if self.config.requests_per_second == 0 {
      return Ok(()); // Unlimited
    }

    let mut buckets = self.buckets.write();
    let bucket = buckets
      .entry(ip)
      .or_insert_with(|| TokenBucket::new(self.config.requests_per_second, self.config.burst_size));

    if bucket.try_consume() {
      Ok(())
    } else {
      Err(RateLimitError::RateLimited {
        ip,
        retry_after: Duration::from_secs_f64(1.0 / bucket.rate),
      })
    }
  }

  /// Get a query permit for a client. Returns a guard that releases the permit on drop.
  pub fn acquire_query_permit(&self, client_id: Uuid) -> Result<QueryPermit, RateLimitError> {
    if self.config.max_concurrent_queries == 0 {
      return Ok(QueryPermit {
        counter: None,
        client_id,
      }); // Unlimited
    }

    let counter = {
      let mut queries = self.concurrent_queries.write();
      queries
        .entry(client_id)
        .or_insert_with(|| Arc::new(AtomicU32::new(0)))
        .clone()
    };

    let current = counter.fetch_add(1, Ordering::SeqCst);
    if current >= self.config.max_concurrent_queries {
      counter.fetch_sub(1, Ordering::SeqCst);
      return Err(RateLimitError::TooManyConcurrentQueries {
        client_id,
        limit: self.config.max_concurrent_queries,
      });
    }

    Ok(QueryPermit {
      counter: Some(counter),
      client_id,
    })
  }

  /// Get the query timeout duration.
  pub fn query_timeout(&self) -> Option<Duration> {
    if self.config.query_timeout_ms == 0 {
      None
    } else {
      Some(Duration::from_millis(self.config.query_timeout_ms))
    }
  }

  /// Get the max message size.
  pub fn max_message_size(&self) -> usize {
    self.config.max_message_size
  }

  /// Clean up stale entries (call periodically).
  pub fn cleanup(&self) {
    // Remove stale token buckets (older than 1 minute with full tokens)
    let mut buckets = self.buckets.write();
    let now = Instant::now();
    buckets.retain(|_, bucket| {
      bucket.refill();
      now.duration_since(bucket.last_update) < Duration::from_secs(60)
        || bucket.tokens < bucket.capacity
    });

    // Remove empty connection entries (shouldn't happen, but just in case)
    let mut conns = self.connections.write();
    conns.retain(|_, count| *count > 0);

    // Remove stale query counters
    let mut queries = self.concurrent_queries.write();
    queries.retain(|_, counter| counter.load(Ordering::SeqCst) > 0);
  }
}

/// RAII guard for query permits.
pub struct QueryPermit {
  counter: Option<Arc<AtomicU32>>,
  #[allow(dead_code)]
  client_id: Uuid,
}

impl Drop for QueryPermit {
  fn drop(&mut self) {
    if let Some(ref counter) = self.counter {
      counter.fetch_sub(1, Ordering::SeqCst);
    }
  }
}

/// Rate limit errors.
#[derive(Debug, Clone)]
pub enum RateLimitError {
  TooManyConnections { ip: IpAddr, limit: u32 },
  RateLimited { ip: IpAddr, retry_after: Duration },
  TooManyConcurrentQueries { client_id: Uuid, limit: u32 },
  QueryTimeout,
}

impl std::fmt::Display for RateLimitError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::TooManyConnections { ip, limit } => {
        write!(
          f,
          "Too many connections from {}: limit is {} per IP",
          ip, limit
        )
      }
      Self::RateLimited { retry_after, .. } => {
        write!(f, "Rate limited, retry after {:?}", retry_after)
      }
      Self::TooManyConcurrentQueries { limit, .. } => {
        write!(
          f,
          "Too many concurrent queries: limit is {} per client",
          limit
        )
      }
      Self::QueryTimeout => write!(f, "Query execution timed out"),
    }
  }
}

impl std::error::Error for RateLimitError {}

#[cfg(test)]
mod tests {
  use super::*;
  use std::net::Ipv4Addr;

  fn test_config() -> LimitsSection {
    LimitsSection {
      max_connections_per_ip: 2,
      requests_per_second: 10,
      burst_size: 5,
      query_timeout_ms: 1000,
      max_concurrent_queries: 3,
      max_message_size: 1024,
    }
  }

  #[test]
  fn test_connection_limit() {
    let limiter = RateLimiter::new(test_config());
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // First two connections should succeed
    assert!(limiter.check_connection(ip).is_ok());
    assert!(limiter.check_connection(ip).is_ok());

    // Third should fail
    assert!(limiter.check_connection(ip).is_err());

    // Release one
    limiter.release_connection(ip);

    // Now should succeed
    assert!(limiter.check_connection(ip).is_ok());
  }

  #[test]
  fn test_rate_limiting() {
    let limiter = RateLimiter::new(test_config());
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // First 5 requests should succeed (burst)
    for _ in 0..5 {
      assert!(limiter.check_request(ip).is_ok());
    }

    // Next request should fail (bucket empty)
    assert!(limiter.check_request(ip).is_err());
  }

  #[test]
  fn test_concurrent_queries() {
    let limiter = RateLimiter::new(test_config());
    let client_id = Uuid::new_v4();

    // First 3 queries should succeed
    let _permit1 = limiter.acquire_query_permit(client_id).unwrap();
    let _permit2 = limiter.acquire_query_permit(client_id).unwrap();
    let _permit3 = limiter.acquire_query_permit(client_id).unwrap();

    // Fourth should fail
    assert!(limiter.acquire_query_permit(client_id).is_err());

    // Drop one permit
    drop(_permit1);

    // Now should succeed
    assert!(limiter.acquire_query_permit(client_id).is_ok());
  }

  #[test]
  fn test_unlimited() {
    let config = LimitsSection {
      max_connections_per_ip: 0,
      requests_per_second: 0,
      burst_size: 0,
      query_timeout_ms: 0,
      max_concurrent_queries: 0,
      max_message_size: 0,
    };
    let limiter = RateLimiter::new(config);
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let client_id = Uuid::new_v4();

    // All should succeed with unlimited config
    for _ in 0..1000 {
      assert!(limiter.check_connection(ip).is_ok());
      assert!(limiter.check_request(ip).is_ok());
      assert!(limiter.acquire_query_permit(client_id).is_ok());
    }
  }
}
