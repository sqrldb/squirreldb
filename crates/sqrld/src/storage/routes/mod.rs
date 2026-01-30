mod bucket;
mod multipart;
mod object;

pub use bucket::*;
pub use multipart::*;
pub use object::*;

use axum::{
  routing::{delete, get, head, post, put},
  Router,
};
use std::sync::Arc;

use super::server::StorageState;

/// Build S3 API router
pub fn build_router(state: Arc<StorageState>) -> Router {
  Router::new()
    // Service level operations
    .route("/", get(list_buckets))
    // Bucket operations
    .route("/{bucket}", put(create_bucket))
    .route("/{bucket}", delete(delete_bucket))
    .route("/{bucket}", head(head_bucket))
    .route("/{bucket}", get(list_objects_or_operation))
    // Object operations
    .route("/{bucket}/{*key}", put(put_object_or_operation))
    .route("/{bucket}/{*key}", get(get_object_or_operation))
    .route("/{bucket}/{*key}", head(head_object))
    .route("/{bucket}/{*key}", delete(delete_object_or_operation))
    .route("/{bucket}/{*key}", post(post_object_operation))
    .with_state(state)
}
