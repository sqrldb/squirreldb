// Server-side API (only compiled with server feature)
#[cfg(feature = "server")]
mod api;

// CSR components (only compiled for WASM)
#[cfg(feature = "csr")]
pub mod apiclient;
#[cfg(feature = "csr")]
pub mod components;
#[cfg(feature = "csr")]
pub mod state;

#[cfg(feature = "server")]
pub use api::AdminServer;
#[cfg(feature = "server")]
pub use api::{emit_log, get_log_broadcaster, LogEntry};
