// Admin module (server API or CSR components depending on feature)
pub mod admin;

// Server-side modules (only compiled with server feature)
#[cfg(feature = "server")]
pub mod client;
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
#[cfg(feature = "server")]
pub mod types;
