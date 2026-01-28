use axum::extract::Request;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::AuthContext;
use crate::s3::error::S3Error;
use crate::s3::server::S3State;

type HmacSha256 = Hmac<Sha256>;

/// Verify AWS Signature Version 4 authentication
pub async fn verify_sigv4(state: &S3State, request: &Request) -> Result<AuthContext, S3Error> {
  // Parse Authorization header
  let auth_header = request
    .headers()
    .get("authorization")
    .and_then(|v| v.to_str().ok())
    .ok_or_else(|| S3Error::access_denied("Missing Authorization header"))?;

  let auth = parse_auth_header(auth_header)?;

  // Get the access key from the database
  let (secret_key, owner_id) = state
    .backend
    .get_s3_access_key(&auth.credential.access_key_id)
    .await
    .map_err(|_| S3Error::access_denied("Invalid access key"))?
    .ok_or_else(|| S3Error::access_denied("Access key not found"))?;

  // Get x-amz-date or Date header
  let request_date = get_request_date(request)?;

  // Check timestamp is within 15 minutes
  let now = Utc::now();
  let time_diff = (now - request_date).num_minutes().abs();
  if time_diff > 15 {
    return Err(S3Error::new(
      crate::s3::error::S3ErrorCode::RequestTimeTooSkewed,
      "Request time is too skewed",
    ));
  }

  // Build canonical request
  let canonical_request = build_canonical_request(request, &auth.signed_headers)?;

  // Build string to sign
  let string_to_sign = build_string_to_sign(
    &request_date,
    &auth.credential.date,
    &auth.credential.region,
    &auth.credential.service,
    &canonical_request,
  );

  // Calculate signature
  let calculated_signature = calculate_signature(
    &secret_key,
    &auth.credential.date,
    &auth.credential.region,
    &auth.credential.service,
    &string_to_sign,
  );

  // Compare signatures
  if calculated_signature != auth.signature {
    return Err(S3Error::new(
      crate::s3::error::S3ErrorCode::SignatureDoesNotMatch,
      "The request signature we calculated does not match the signature you provided",
    ));
  }

  Ok(AuthContext {
    user_id: owner_id.map(|u| u.to_string()),
    access_key_id: Some(auth.credential.access_key_id),
    is_authenticated: true,
  })
}

#[derive(Debug)]
struct ParsedAuth {
  credential: Credential,
  signed_headers: Vec<String>,
  signature: String,
}

#[derive(Debug)]
struct Credential {
  access_key_id: String,
  date: String,
  region: String,
  service: String,
}

fn parse_auth_header(header: &str) -> Result<ParsedAuth, S3Error> {
  // Format: AWS4-HMAC-SHA256 Credential=.../.../.../s3/aws4_request, SignedHeaders=..., Signature=...
  let header = header
    .strip_prefix("AWS4-HMAC-SHA256 ")
    .ok_or_else(|| S3Error::access_denied("Invalid auth algorithm"))?;

  let mut credential = None;
  let mut signed_headers = None;
  let mut signature = None;

  for part in header.split(", ") {
    if let Some(cred) = part.strip_prefix("Credential=") {
      credential = Some(parse_credential(cred)?);
    } else if let Some(headers) = part.strip_prefix("SignedHeaders=") {
      signed_headers = Some(headers.split(';').map(String::from).collect());
    } else if let Some(sig) = part.strip_prefix("Signature=") {
      signature = Some(sig.to_string());
    }
  }

  Ok(ParsedAuth {
    credential: credential.ok_or_else(|| S3Error::access_denied("Missing Credential"))?,
    signed_headers: signed_headers
      .ok_or_else(|| S3Error::access_denied("Missing SignedHeaders"))?,
    signature: signature.ok_or_else(|| S3Error::access_denied("Missing Signature"))?,
  })
}

fn parse_credential(cred: &str) -> Result<Credential, S3Error> {
  // Format: ACCESS_KEY_ID/20130524/us-east-1/s3/aws4_request
  let parts: Vec<&str> = cred.split('/').collect();
  if parts.len() != 5 {
    return Err(S3Error::access_denied("Invalid Credential format"));
  }

  Ok(Credential {
    access_key_id: parts[0].to_string(),
    date: parts[1].to_string(),
    region: parts[2].to_string(),
    service: parts[3].to_string(),
  })
}

fn get_request_date(request: &Request) -> Result<DateTime<Utc>, S3Error> {
  // Try x-amz-date first
  if let Some(date) = request.headers().get("x-amz-date") {
    if let Ok(date_str) = date.to_str() {
      return parse_amz_date(date_str);
    }
  }

  // Fall back to Date header
  if let Some(date) = request.headers().get("date") {
    if let Ok(date_str) = date.to_str() {
      return DateTime::parse_from_rfc2822(date_str)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| S3Error::access_denied("Invalid Date header"));
    }
  }

  Err(S3Error::access_denied("Missing date header"))
}

fn parse_amz_date(date: &str) -> Result<DateTime<Utc>, S3Error> {
  // Format: 20130524T000000Z
  DateTime::parse_from_str(date, "%Y%m%dT%H%M%SZ")
    .map(|d| d.with_timezone(&Utc))
    .map_err(|_| S3Error::access_denied("Invalid x-amz-date format"))
}

fn build_canonical_request(
  request: &Request,
  signed_headers: &[String],
) -> Result<String, S3Error> {
  let method = request.method().as_str();
  let uri = request.uri().path();
  let query = request.uri().query().unwrap_or("");

  // Sort query parameters
  let canonical_query = build_canonical_query(query);

  // Build canonical headers
  let mut headers_map = BTreeMap::new();
  for header in signed_headers {
    let header_lower = header.to_lowercase();
    if let Some(value) = request.headers().get(&header_lower) {
      if let Ok(v) = value.to_str() {
        headers_map.insert(header_lower.clone(), v.trim().to_string());
      }
    }
  }

  let canonical_headers: String = headers_map
    .iter()
    .map(|(k, v)| format!("{}:{}\n", k, v))
    .collect();

  let signed_headers_str = signed_headers.join(";");

  // Get payload hash
  let payload_hash = request
    .headers()
    .get("x-amz-content-sha256")
    .and_then(|v| v.to_str().ok())
    .unwrap_or("UNSIGNED-PAYLOAD");

  Ok(format!(
    "{}\n{}\n{}\n{}\n{}\n{}",
    method, uri, canonical_query, canonical_headers, signed_headers_str, payload_hash
  ))
}

fn build_canonical_query(query: &str) -> String {
  if query.is_empty() {
    return String::new();
  }

  let mut params: Vec<(String, String)> = query
    .split('&')
    .filter_map(|p| {
      let mut parts = p.splitn(2, '=');
      let key = parts.next()?;
      let value = parts.next().unwrap_or("");
      Some((
        urlencoding::encode(key).into_owned(),
        urlencoding::encode(value).into_owned(),
      ))
    })
    .collect();

  params.sort();

  params
    .iter()
    .map(|(k, v)| format!("{}={}", k, v))
    .collect::<Vec<_>>()
    .join("&")
}

fn build_string_to_sign(
  date: &DateTime<Utc>,
  date_str: &str,
  region: &str,
  service: &str,
  canonical_request: &str,
) -> String {
  let timestamp = date.format("%Y%m%dT%H%M%SZ").to_string();
  let scope = format!("{}/{}/{}/aws4_request", date_str, region, service);

  let canonical_request_hash = {
    let mut hasher = Sha256::new();
    hasher.update(canonical_request.as_bytes());
    format!("{:x}", hasher.finalize())
  };

  format!(
    "AWS4-HMAC-SHA256\n{}\n{}\n{}",
    timestamp, scope, canonical_request_hash
  )
}

fn calculate_signature(
  secret_key: &str,
  date: &str,
  region: &str,
  service: &str,
  string_to_sign: &str,
) -> String {
  let k_secret = format!("AWS4{}", secret_key);
  let k_date = hmac_sha256(k_secret.as_bytes(), date.as_bytes());
  let k_region = hmac_sha256(&k_date, region.as_bytes());
  let k_service = hmac_sha256(&k_region, service.as_bytes());
  let k_signing = hmac_sha256(&k_service, b"aws4_request");
  let signature = hmac_sha256(&k_signing, string_to_sign.as_bytes());

  hex::encode(signature)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
  let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
  mac.update(data);
  mac.finalize().into_bytes().to_vec()
}
