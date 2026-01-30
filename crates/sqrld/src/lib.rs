// Admin module (server API or CSR components depending on feature)
pub mod admin;

// Server-side modules (only compiled with server feature)
#[cfg(feature = "server")]
pub mod backup;
#[cfg(feature = "server")]
pub mod cache;
#[cfg(feature = "server")]
pub mod db;
#[cfg(feature = "server")]
pub mod features;
#[cfg(feature = "server")]
pub mod mcp;
#[cfg(feature = "server")]
pub mod query;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod storage;
#[cfg(feature = "server")]
pub mod subscriptions;

// Re-export types from the types crate for convenience
pub use types;
