//! Redis-compatible in-memory caching module
//!
//! Provides a lightweight caching layer with:
//! - JSON-compatible values (not Redis data structures)
//! - Event system mirroring SquirrelDB's changefeed pattern
//! - Optional snapshot persistence
//! - RESP protocol for redis-cli compatibility

mod commands;
pub mod config;
mod entry;
mod events;
pub mod proxy;
pub mod resp;
mod server;
mod snapshot;
mod store;

pub use config::{CacheConfig, CacheMode, CacheProxyConfig};
pub use entry::{CacheEntry, CacheValue};
pub use events::{CacheChange, CacheChangeOperation, CacheSubscriptionManager};
pub use proxy::RedisProxyClient;
pub use resp::{RespError, RespValue};
pub use server::CacheFeature;
pub use snapshot::SnapshotManager;
pub use store::{CacheStats, CacheStore, EvictionPolicy, InMemoryCacheStore};
