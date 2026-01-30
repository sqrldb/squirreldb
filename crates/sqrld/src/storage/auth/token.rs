use sha2::{Digest, Sha256};

use super::AuthContext;
use crate::storage::error::StorageError;
use crate::storage::server::StorageState;

/// Verify SquirrelDB token authentication
pub async fn verify_token(state: &StorageState, token: &str) -> Result<AuthContext, StorageError> {
  // Hash the token
  let token_hash = hash_token(token);

  // Validate against the database
  let project_id = state
    .backend
    .validate_token(&token_hash)
    .await
    .map_err(|_| StorageError::access_denied("Token validation failed"))?;

  if project_id.is_none() {
    return Err(StorageError::access_denied("Invalid token"));
  }

  Ok(AuthContext {
    user_id: None, // SquirrelDB tokens don't have user IDs currently
    access_key_id: None,
    is_authenticated: true,
  })
}

fn hash_token(token: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(token.as_bytes());
  format!("{:x}", hasher.finalize())
}
