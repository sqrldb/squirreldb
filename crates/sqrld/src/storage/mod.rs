mod auth;
pub mod backend;
pub mod config;
pub mod error;
mod filesystem;
pub mod proxy;
mod routes;
mod server;
pub mod types;
pub mod xml;

pub use backend::StorageBackend;
pub use config::StorageConfig;
pub use error::{StorageError, StorageErrorCode};
pub use filesystem::LocalFileStorage;
pub use proxy::S3ProxyClient;
pub use server::StorageFeature;
pub use types::*;
