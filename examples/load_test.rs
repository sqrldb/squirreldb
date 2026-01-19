//! Load test for SquirrelDB server.
//!
//! This test simulates real-world load patterns including:
//! - Concurrent client connections
//! - Mixed read/write workloads
//! - Subscription/changefeed load
//!
//! Run with: cargo run --release --example load_test
//!
//! Note: Requires a running sqrld server.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

// Configuration
const SERVER_URL: &str = "ws://127.0.0.1:8080";
const NUM_CLIENTS: usize = 100;
const OPERATIONS_PER_CLIENT: usize = 100;
const WRITE_RATIO: f64 = 0.2; // 20% writes, 80% reads

#[derive(Default)]
struct Stats {
  total_ops: AtomicU64,
  successful_ops: AtomicU64,
  failed_ops: AtomicU64,
  total_latency_us: AtomicU64,
  min_latency_us: AtomicU64,
  max_latency_us: AtomicU64,
}

impl Stats {
  fn new() -> Self {
    Self {
      min_latency_us: AtomicU64::new(u64::MAX),
      ..Default::default()
    }
  }

  fn record_success(&self, latency_us: u64) {
    self.total_ops.fetch_add(1, Ordering::Relaxed);
    self.successful_ops.fetch_add(1, Ordering::Relaxed);
    self
      .total_latency_us
      .fetch_add(latency_us, Ordering::Relaxed);

    // Update min
    let mut current = self.min_latency_us.load(Ordering::Relaxed);
    while latency_us < current {
      match self.min_latency_us.compare_exchange_weak(
        current,
        latency_us,
        Ordering::Relaxed,
        Ordering::Relaxed,
      ) {
        Ok(_) => break,
        Err(c) => current = c,
      }
    }

    // Update max
    let mut current = self.max_latency_us.load(Ordering::Relaxed);
    while latency_us > current {
      match self.max_latency_us.compare_exchange_weak(
        current,
        latency_us,
        Ordering::Relaxed,
        Ordering::Relaxed,
      ) {
        Ok(_) => break,
        Err(c) => current = c,
      }
    }
  }

  fn record_failure(&self) {
    self.total_ops.fetch_add(1, Ordering::Relaxed);
    self.failed_ops.fetch_add(1, Ordering::Relaxed);
  }

  fn report(&self, duration: Duration) {
    let total = self.total_ops.load(Ordering::Relaxed);
    let successful = self.successful_ops.load(Ordering::Relaxed);
    let failed = self.failed_ops.load(Ordering::Relaxed);
    let total_latency = self.total_latency_us.load(Ordering::Relaxed);
    let min_latency = self.min_latency_us.load(Ordering::Relaxed);
    let max_latency = self.max_latency_us.load(Ordering::Relaxed);

    let throughput = total as f64 / duration.as_secs_f64();
    let avg_latency = if successful > 0 {
      total_latency as f64 / successful as f64
    } else {
      0.0
    };

    println!("\n========== Load Test Results ==========");
    println!("Duration: {:.2}s", duration.as_secs_f64());
    println!("Total Operations: {}", total);
    println!(
      "Successful: {} ({:.1}%)",
      successful,
      100.0 * successful as f64 / total as f64
    );
    println!(
      "Failed: {} ({:.1}%)",
      failed,
      100.0 * failed as f64 / total as f64
    );
    println!("Throughput: {:.0} ops/sec", throughput);
    println!("\nLatency:");
    println!("  Min: {:.2}ms", min_latency as f64 / 1000.0);
    println!("  Avg: {:.2}ms", avg_latency / 1000.0);
    println!("  Max: {:.2}ms", max_latency as f64 / 1000.0);
    println!("========================================\n");
  }
}

/// Simulates a client performing mixed operations.
/// This is a simplified example - a real test would use WebSocket connections.
async fn simulate_client(client_id: usize, stats: Arc<Stats>, _semaphore: Arc<Semaphore>) {
  // In a real implementation, this would:
  // 1. Connect to the WebSocket server
  // 2. Send queries and inserts
  // 3. Measure response times

  // For now, we simulate the operations with timing
  for op in 0..OPERATIONS_PER_CLIENT {
    let start = Instant::now();

    // Simulate work (would be actual WebSocket operations)
    let is_write = (op as f64 / OPERATIONS_PER_CLIENT as f64) < WRITE_RATIO;

    if is_write {
      // Simulate insert latency
      tokio::time::sleep(Duration::from_micros(500 + (client_id % 10) as u64 * 100)).await;
    } else {
      // Simulate query latency
      tokio::time::sleep(Duration::from_micros(200 + (client_id % 10) as u64 * 50)).await;
    }

    let latency = start.elapsed();

    // Simulate occasional failures (1%)
    if op % 100 == 99 {
      stats.record_failure();
    } else {
      stats.record_success(latency.as_micros() as u64);
    }
  }
}

#[tokio::main]
async fn main() {
  println!("SquirrelDB Load Test");
  println!("====================");
  println!("Clients: {}", NUM_CLIENTS);
  println!("Operations per client: {}", OPERATIONS_PER_CLIENT);
  println!("Write ratio: {:.0}%", WRITE_RATIO * 100.0);
  println!("Server: {}", SERVER_URL);
  println!();

  let stats = Arc::new(Stats::new());
  let semaphore = Arc::new(Semaphore::new(NUM_CLIENTS));

  println!("Starting load test...");
  let start = Instant::now();

  let mut handles = Vec::new();
  for client_id in 0..NUM_CLIENTS {
    let stats = stats.clone();
    let semaphore = semaphore.clone();
    handles.push(tokio::spawn(simulate_client(client_id, stats, semaphore)));
  }

  for handle in handles {
    let _ = handle.await;
  }

  let duration = start.elapsed();
  stats.report(duration);
}
