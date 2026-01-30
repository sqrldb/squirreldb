//! Authentication utilities for admin UI

use argon2::{
  password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
  Argon2,
};
use sha2::{Digest, Sha256};

/// Hash a password using Argon2id
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
  let salt = SaltString::generate(&mut OsRng);
  let argon2 = Argon2::default();
  let hash = argon2.hash_password(password.as_bytes(), &salt)?;
  Ok(hash.to_string())
}

/// Verify a password against an Argon2 hash
pub fn verify_password(password: &str, hash: &str) -> bool {
  let parsed_hash = match PasswordHash::new(hash) {
    Ok(h) => h,
    Err(_) => return false,
  };
  Argon2::default()
    .verify_password(password.as_bytes(), &parsed_hash)
    .is_ok()
}

/// Generate a random session token
pub fn generate_session_token() -> String {
  use rand::Rng;
  let mut rng = rand::thread_rng();
  let bytes: [u8; 32] = rng.gen();
  hex::encode(bytes)
}

/// Hash a session token for storage
pub fn hash_session_token(token: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(token.as_bytes());
  hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_password_hash_and_verify() {
    let password = "test_password_123!";
    let hash = hash_password(password).unwrap();
    assert!(verify_password(password, &hash));
    assert!(!verify_password("wrong_password", &hash));
  }

  #[test]
  fn test_session_token() {
    let token = generate_session_token();
    assert_eq!(token.len(), 64); // 32 bytes = 64 hex chars
    let hash = hash_session_token(&token);
    assert_eq!(hash.len(), 64); // SHA256 = 64 hex chars
  }
}
