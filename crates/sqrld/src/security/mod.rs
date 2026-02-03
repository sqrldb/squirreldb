//! Security utilities for SquirrelDB
//!
//! This module provides security primitives including:
//! - Constant-time comparison for cryptographic values
//! - Object key validation to prevent path traversal
//! - Security headers middleware

use sha2::{Digest, Sha256};

/// Constant-time string comparison to prevent timing attacks.
/// Returns true if both strings are equal.
pub fn constant_time_compare(a: &str, b: &str) -> bool {
  if a.len() != b.len() {
    return false;
  }

  let mut result: u8 = 0;
  for (x, y) in a.bytes().zip(b.bytes()) {
    result |= x ^ y;
  }
  result == 0
}

/// Hash a value using SHA-256 and return as hex string
pub fn hash_sha256(value: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(value.as_bytes());
  format!("{:x}", hasher.finalize())
}

/// Compare a plaintext value against its expected hash using constant-time comparison.
/// This prevents timing attacks on sensitive comparisons like admin tokens.
pub fn verify_hash(plaintext: &str, expected_hash: &str) -> bool {
  let actual_hash = hash_sha256(plaintext);
  constant_time_compare(&actual_hash, expected_hash)
}

/// Validates an object key to prevent path traversal attacks.
/// Returns Ok(()) if the key is safe, or an error describing the issue.
pub fn validate_object_key(key: &str) -> Result<(), ObjectKeyError> {
  if key.is_empty() {
    return Err(ObjectKeyError::Empty);
  }

  // Check for path traversal attempts
  if key.contains("..") {
    return Err(ObjectKeyError::PathTraversal);
  }

  // Check for backslashes (Windows path separator)
  if key.contains('\\') {
    return Err(ObjectKeyError::InvalidCharacter('\\'));
  }

  // Check for null bytes
  if key.contains('\0') {
    return Err(ObjectKeyError::NullByte);
  }

  // Disallow absolute paths
  if key.starts_with('/') {
    return Err(ObjectKeyError::AbsolutePath);
  }

  // Check for control characters
  for c in key.chars() {
    if c.is_control() && c != '\t' {
      return Err(ObjectKeyError::ControlCharacter);
    }
  }

  // Limit key length (S3 max is 1024 bytes)
  if key.len() > 1024 {
    return Err(ObjectKeyError::TooLong(key.len()));
  }

  Ok(())
}

/// Errors that can occur when validating object keys
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectKeyError {
  /// Key is empty
  Empty,
  /// Key contains path traversal sequence (..)
  PathTraversal,
  /// Key contains invalid character
  InvalidCharacter(char),
  /// Key contains null byte
  NullByte,
  /// Key is an absolute path
  AbsolutePath,
  /// Key contains control characters
  ControlCharacter,
  /// Key exceeds maximum length
  TooLong(usize),
}

impl std::fmt::Display for ObjectKeyError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Empty => write!(f, "Object key cannot be empty"),
      Self::PathTraversal => write!(f, "Object key contains path traversal sequence"),
      Self::InvalidCharacter(c) => write!(f, "Object key contains invalid character: {:?}", c),
      Self::NullByte => write!(f, "Object key contains null byte"),
      Self::AbsolutePath => write!(f, "Object key cannot be an absolute path"),
      Self::ControlCharacter => write!(f, "Object key contains control character"),
      Self::TooLong(len) => write!(f, "Object key too long: {} bytes (max 1024)", len),
    }
  }
}

impl std::error::Error for ObjectKeyError {}

/// Security headers middleware for HTTP responses.
/// Adds standard security headers to all responses.
#[cfg(feature = "server")]
pub mod headers {
  use axum::http::{header, HeaderValue, Request, Response};
  use std::future::Future;
  use std::pin::Pin;
  use std::task::{Context, Poll};
  use tower::{Layer, Service};

  /// Layer that adds security headers to all responses
  #[derive(Clone, Default)]
  pub struct SecurityHeadersLayer;

  impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersService<S>;

    fn layer(&self, inner: S) -> Self::Service {
      SecurityHeadersService { inner }
    }
  }

  /// Service that adds security headers to responses
  #[derive(Clone)]
  pub struct SecurityHeadersService<S> {
    inner: S,
  }

  impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for SecurityHeadersService<S>
  where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
  {
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
      self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
      let mut inner = self.inner.clone();
      Box::pin(async move {
        let mut response = inner.call(req).await?;
        let headers = response.headers_mut();

        // Prevent MIME type sniffing
        headers.insert(
          header::X_CONTENT_TYPE_OPTIONS,
          HeaderValue::from_static("nosniff"),
        );

        // Prevent clickjacking
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));

        // XSS protection (legacy, but still useful for older browsers)
        headers.insert(
          "X-XSS-Protection",
          HeaderValue::from_static("1; mode=block"),
        );

        // Referrer policy
        headers.insert(
          header::REFERRER_POLICY,
          HeaderValue::from_static("strict-origin-when-cross-origin"),
        );

        // Content Security Policy (permissive for admin UI)
        headers.insert(
          header::CONTENT_SECURITY_POLICY,
          HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ws: wss:; font-src 'self' data:; object-src 'none'; frame-ancestors 'none';",
          ),
        );

        // Permissions policy (formerly Feature-Policy)
        headers.insert(
          "Permissions-Policy",
          HeaderValue::from_static(
            "accelerometer=(), camera=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), payment=(), usb=()",
          ),
        );

        Ok(response)
      })
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_constant_time_compare_equal() {
    assert!(constant_time_compare("hello", "hello"));
    assert!(constant_time_compare("", ""));
    assert!(constant_time_compare("a", "a"));
  }

  #[test]
  fn test_constant_time_compare_not_equal() {
    assert!(!constant_time_compare("hello", "world"));
    assert!(!constant_time_compare("hello", "hello!"));
    assert!(!constant_time_compare("", "a"));
  }

  #[test]
  fn test_verify_hash() {
    let plaintext = "test_token";
    let hash = hash_sha256(plaintext);
    assert!(verify_hash(plaintext, &hash));
    assert!(!verify_hash("wrong_token", &hash));
  }

  #[test]
  fn test_validate_object_key_valid() {
    assert!(validate_object_key("file.txt").is_ok());
    assert!(validate_object_key("path/to/file.txt").is_ok());
    assert!(validate_object_key("a/b/c/d.json").is_ok());
    assert!(validate_object_key("file-name_123.tar.gz").is_ok());
  }

  #[test]
  fn test_validate_object_key_path_traversal() {
    assert_eq!(
      validate_object_key("../etc/passwd"),
      Err(ObjectKeyError::PathTraversal)
    );
    assert_eq!(
      validate_object_key("path/../secret"),
      Err(ObjectKeyError::PathTraversal)
    );
    assert_eq!(
      validate_object_key(".."),
      Err(ObjectKeyError::PathTraversal)
    );
  }

  #[test]
  fn test_validate_object_key_backslash() {
    assert_eq!(
      validate_object_key("path\\to\\file"),
      Err(ObjectKeyError::InvalidCharacter('\\'))
    );
  }

  #[test]
  fn test_validate_object_key_absolute() {
    assert_eq!(
      validate_object_key("/etc/passwd"),
      Err(ObjectKeyError::AbsolutePath)
    );
  }

  #[test]
  fn test_validate_object_key_empty() {
    assert_eq!(validate_object_key(""), Err(ObjectKeyError::Empty));
  }

  #[test]
  fn test_validate_object_key_null_byte() {
    assert_eq!(
      validate_object_key("file\0.txt"),
      Err(ObjectKeyError::NullByte)
    );
  }
}
