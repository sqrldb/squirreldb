use sha2::{Digest, Sha256};

use super::AuthContext;
use crate::s3::error::S3Error;
use crate::s3::server::S3State;

/// Verify SquirrelDB token authentication
pub async fn verify_token(state: &S3State, token: &str) -> Result<AuthContext, S3Error> {
  // Hash the token
  let token_hash = hash_token(token);

  // Validate against the database
  let valid = state
    .backend
    .validate_token(&token_hash)
    .await
    .map_err(|_| S3Error::access_denied("Token validation failed"))?;

  if !valid {
    return Err(S3Error::access_denied("Invalid token"));
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
