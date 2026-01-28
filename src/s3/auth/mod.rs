#![allow(dead_code)]

mod sigv4;
mod token;

pub use sigv4::verify_sigv4;
pub use token::verify_token;

use axum::{
  extract::{Request, State},
  middleware::Next,
  response::{IntoResponse, Response},
};
use std::sync::Arc;

use super::error::S3Error;
use super::server::S3State;

/// Authenticated user context
#[derive(Debug, Clone, Default)]
pub struct AuthContext {
  pub user_id: Option<String>,
  pub access_key_id: Option<String>,
  pub is_authenticated: bool,
}

/// S3 authentication middleware
/// Supports both AWS Signature V4 and SquirrelDB tokens
pub async fn s3_auth_middleware(
  State(state): State<Arc<S3State>>,
  mut request: Request,
  next: Next,
) -> Response {
  // 1. Check for AWS Signature V4
  if let Some(auth) = request.headers().get("authorization") {
    if let Ok(auth_str) = auth.to_str() {
      if auth_str.starts_with("AWS4-HMAC-SHA256") {
        match verify_sigv4(&state, &request).await {
          Ok(ctx) => {
            request.extensions_mut().insert(ctx);
            return next.run(request).await;
          }
          Err(e) => {
            return e.into_response();
          }
        }
      }
    }
  }

  // 2. Check for SquirrelDB token (X-Sqrl-Token header or Bearer token)
  if let Some(token) = extract_sqrl_token(&request) {
    match verify_token(&state, &token).await {
      Ok(ctx) => {
        request.extensions_mut().insert(ctx);
        return next.run(request).await;
      }
      Err(e) => {
        return e.into_response();
      }
    }
  }

  // 3. No authentication provided - return error
  S3Error::access_denied("No valid authentication provided").into_response()
}

/// Extract SquirrelDB token from request
fn extract_sqrl_token(request: &Request) -> Option<String> {
  // Check X-Sqrl-Token header
  if let Some(token) = request.headers().get("x-sqrl-token") {
    if let Ok(s) = token.to_str() {
      return Some(s.to_string());
    }
  }

  // Check Authorization: Bearer header (if not AWS auth)
  if let Some(auth) = request.headers().get("authorization") {
    if let Ok(auth_str) = auth.to_str() {
      if let Some(token) = auth_str.strip_prefix("Bearer ") {
        // Only accept sqrl_ prefixed tokens
        if token.starts_with("sqrl_") {
          return Some(token.to_string());
        }
      }
    }
  }

  None
}
