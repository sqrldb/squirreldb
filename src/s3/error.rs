use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::fmt;

/// S3 API error
#[derive(Debug, Clone)]
pub struct S3Error {
  pub code: S3ErrorCode,
  pub message: String,
  pub resource: Option<String>,
  pub request_id: Option<String>,
}

/// S3 error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum S3ErrorCode {
  AccessDenied,
  AccountProblem,
  AllAccessDisabled,
  AmbiguousGrantByEmailAddress,
  AuthorizationHeaderMalformed,
  BadDigest,
  BucketAlreadyExists,
  BucketAlreadyOwnedByYou,
  BucketNotEmpty,
  CredentialsNotSupported,
  CrossLocationLoggingProhibited,
  EntityTooSmall,
  EntityTooLarge,
  ExpiredToken,
  IllegalVersioningConfigurationException,
  IncompleteBody,
  IncorrectNumberOfFilesInPostRequest,
  InlineDataTooLarge,
  InternalError,
  InvalidAccessKeyId,
  InvalidAddressingHeader,
  InvalidArgument,
  InvalidBucketName,
  InvalidBucketState,
  InvalidDigest,
  InvalidEncryptionAlgorithmError,
  InvalidLocationConstraint,
  InvalidObjectState,
  InvalidPart,
  InvalidPartOrder,
  InvalidPayer,
  InvalidPolicyDocument,
  InvalidRange,
  InvalidRequest,
  InvalidSecurity,
  InvalidSOAPRequest,
  InvalidStorageClass,
  InvalidTargetBucketForLogging,
  InvalidToken,
  InvalidURI,
  KeyTooLongError,
  MalformedACLError,
  MalformedPOSTRequest,
  MalformedXML,
  MaxMessageLengthExceeded,
  MaxPostPreDataLengthExceededError,
  MetadataTooLarge,
  MethodNotAllowed,
  MissingAttachment,
  MissingContentLength,
  MissingRequestBodyError,
  MissingSecurityElement,
  MissingSecurityHeader,
  NoLoggingStatusForKey,
  NoSuchBucket,
  NoSuchBucketPolicy,
  NoSuchKey,
  NoSuchLifecycleConfiguration,
  NoSuchUpload,
  NoSuchVersion,
  NotImplemented,
  NotSignedUp,
  OperationAborted,
  PermanentRedirect,
  PreconditionFailed,
  Redirect,
  RestoreAlreadyInProgress,
  RequestIsNotMultiPartContent,
  RequestTimeout,
  RequestTimeTooSkewed,
  RequestTorrentOfBucketError,
  SignatureDoesNotMatch,
  ServiceUnavailable,
  SlowDown,
  TemporaryRedirect,
  TokenRefreshRequired,
  TooManyBuckets,
  UnexpectedContent,
  UnresolvableGrantByEmailAddress,
  UserKeyMustBeSpecified,
}

impl S3ErrorCode {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::AccessDenied => "AccessDenied",
      Self::AccountProblem => "AccountProblem",
      Self::AllAccessDisabled => "AllAccessDisabled",
      Self::AmbiguousGrantByEmailAddress => "AmbiguousGrantByEmailAddress",
      Self::AuthorizationHeaderMalformed => "AuthorizationHeaderMalformed",
      Self::BadDigest => "BadDigest",
      Self::BucketAlreadyExists => "BucketAlreadyExists",
      Self::BucketAlreadyOwnedByYou => "BucketAlreadyOwnedByYou",
      Self::BucketNotEmpty => "BucketNotEmpty",
      Self::CredentialsNotSupported => "CredentialsNotSupported",
      Self::CrossLocationLoggingProhibited => "CrossLocationLoggingProhibited",
      Self::EntityTooSmall => "EntityTooSmall",
      Self::EntityTooLarge => "EntityTooLarge",
      Self::ExpiredToken => "ExpiredToken",
      Self::IllegalVersioningConfigurationException => "IllegalVersioningConfigurationException",
      Self::IncompleteBody => "IncompleteBody",
      Self::IncorrectNumberOfFilesInPostRequest => "IncorrectNumberOfFilesInPostRequest",
      Self::InlineDataTooLarge => "InlineDataTooLarge",
      Self::InternalError => "InternalError",
      Self::InvalidAccessKeyId => "InvalidAccessKeyId",
      Self::InvalidAddressingHeader => "InvalidAddressingHeader",
      Self::InvalidArgument => "InvalidArgument",
      Self::InvalidBucketName => "InvalidBucketName",
      Self::InvalidBucketState => "InvalidBucketState",
      Self::InvalidDigest => "InvalidDigest",
      Self::InvalidEncryptionAlgorithmError => "InvalidEncryptionAlgorithmError",
      Self::InvalidLocationConstraint => "InvalidLocationConstraint",
      Self::InvalidObjectState => "InvalidObjectState",
      Self::InvalidPart => "InvalidPart",
      Self::InvalidPartOrder => "InvalidPartOrder",
      Self::InvalidPayer => "InvalidPayer",
      Self::InvalidPolicyDocument => "InvalidPolicyDocument",
      Self::InvalidRange => "InvalidRange",
      Self::InvalidRequest => "InvalidRequest",
      Self::InvalidSecurity => "InvalidSecurity",
      Self::InvalidSOAPRequest => "InvalidSOAPRequest",
      Self::InvalidStorageClass => "InvalidStorageClass",
      Self::InvalidTargetBucketForLogging => "InvalidTargetBucketForLogging",
      Self::InvalidToken => "InvalidToken",
      Self::InvalidURI => "InvalidURI",
      Self::KeyTooLongError => "KeyTooLongError",
      Self::MalformedACLError => "MalformedACLError",
      Self::MalformedPOSTRequest => "MalformedPOSTRequest",
      Self::MalformedXML => "MalformedXML",
      Self::MaxMessageLengthExceeded => "MaxMessageLengthExceeded",
      Self::MaxPostPreDataLengthExceededError => "MaxPostPreDataLengthExceededError",
      Self::MetadataTooLarge => "MetadataTooLarge",
      Self::MethodNotAllowed => "MethodNotAllowed",
      Self::MissingAttachment => "MissingAttachment",
      Self::MissingContentLength => "MissingContentLength",
      Self::MissingRequestBodyError => "MissingRequestBodyError",
      Self::MissingSecurityElement => "MissingSecurityElement",
      Self::MissingSecurityHeader => "MissingSecurityHeader",
      Self::NoLoggingStatusForKey => "NoLoggingStatusForKey",
      Self::NoSuchBucket => "NoSuchBucket",
      Self::NoSuchBucketPolicy => "NoSuchBucketPolicy",
      Self::NoSuchKey => "NoSuchKey",
      Self::NoSuchLifecycleConfiguration => "NoSuchLifecycleConfiguration",
      Self::NoSuchUpload => "NoSuchUpload",
      Self::NoSuchVersion => "NoSuchVersion",
      Self::NotImplemented => "NotImplemented",
      Self::NotSignedUp => "NotSignedUp",
      Self::OperationAborted => "OperationAborted",
      Self::PermanentRedirect => "PermanentRedirect",
      Self::PreconditionFailed => "PreconditionFailed",
      Self::Redirect => "Redirect",
      Self::RestoreAlreadyInProgress => "RestoreAlreadyInProgress",
      Self::RequestIsNotMultiPartContent => "RequestIsNotMultiPartContent",
      Self::RequestTimeout => "RequestTimeout",
      Self::RequestTimeTooSkewed => "RequestTimeTooSkewed",
      Self::RequestTorrentOfBucketError => "RequestTorrentOfBucketError",
      Self::SignatureDoesNotMatch => "SignatureDoesNotMatch",
      Self::ServiceUnavailable => "ServiceUnavailable",
      Self::SlowDown => "SlowDown",
      Self::TemporaryRedirect => "TemporaryRedirect",
      Self::TokenRefreshRequired => "TokenRefreshRequired",
      Self::TooManyBuckets => "TooManyBuckets",
      Self::UnexpectedContent => "UnexpectedContent",
      Self::UnresolvableGrantByEmailAddress => "UnresolvableGrantByEmailAddress",
      Self::UserKeyMustBeSpecified => "UserKeyMustBeSpecified",
    }
  }

  pub fn http_status(&self) -> StatusCode {
    match self {
      Self::AccessDenied => StatusCode::FORBIDDEN,
      Self::AccountProblem => StatusCode::FORBIDDEN,
      Self::AllAccessDisabled => StatusCode::FORBIDDEN,
      Self::AuthorizationHeaderMalformed => StatusCode::BAD_REQUEST,
      Self::BadDigest => StatusCode::BAD_REQUEST,
      Self::BucketAlreadyExists => StatusCode::CONFLICT,
      Self::BucketAlreadyOwnedByYou => StatusCode::CONFLICT,
      Self::BucketNotEmpty => StatusCode::CONFLICT,
      Self::EntityTooSmall => StatusCode::BAD_REQUEST,
      Self::EntityTooLarge => StatusCode::BAD_REQUEST,
      Self::ExpiredToken => StatusCode::BAD_REQUEST,
      Self::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
      Self::InvalidAccessKeyId => StatusCode::FORBIDDEN,
      Self::InvalidArgument => StatusCode::BAD_REQUEST,
      Self::InvalidBucketName => StatusCode::BAD_REQUEST,
      Self::InvalidDigest => StatusCode::BAD_REQUEST,
      Self::InvalidPart => StatusCode::BAD_REQUEST,
      Self::InvalidPartOrder => StatusCode::BAD_REQUEST,
      Self::InvalidRange => StatusCode::RANGE_NOT_SATISFIABLE,
      Self::InvalidRequest => StatusCode::BAD_REQUEST,
      Self::InvalidToken => StatusCode::BAD_REQUEST,
      Self::MalformedXML => StatusCode::BAD_REQUEST,
      Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
      Self::MissingContentLength => StatusCode::LENGTH_REQUIRED,
      Self::NoSuchBucket => StatusCode::NOT_FOUND,
      Self::NoSuchKey => StatusCode::NOT_FOUND,
      Self::NoSuchUpload => StatusCode::NOT_FOUND,
      Self::NoSuchVersion => StatusCode::NOT_FOUND,
      Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
      Self::PreconditionFailed => StatusCode::PRECONDITION_FAILED,
      Self::RequestTimeout => StatusCode::BAD_REQUEST,
      Self::RequestTimeTooSkewed => StatusCode::FORBIDDEN,
      Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
      Self::SignatureDoesNotMatch => StatusCode::FORBIDDEN,
      Self::SlowDown => StatusCode::SERVICE_UNAVAILABLE,
      Self::TooManyBuckets => StatusCode::BAD_REQUEST,
      _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
  }
}

impl S3Error {
  pub fn new(code: S3ErrorCode, message: impl Into<String>) -> Self {
    Self {
      code,
      message: message.into(),
      resource: None,
      request_id: None,
    }
  }

  pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
    self.resource = Some(resource.into());
    self
  }

  pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
    self.request_id = Some(request_id.into());
    self
  }

  pub fn access_denied(message: impl Into<String>) -> Self {
    Self::new(S3ErrorCode::AccessDenied, message)
  }

  pub fn no_such_bucket(bucket: impl Into<String>) -> Self {
    let bucket = bucket.into();
    Self::new(
      S3ErrorCode::NoSuchBucket,
      format!("The specified bucket does not exist: {}", bucket),
    )
    .with_resource(bucket)
  }

  pub fn no_such_key(key: impl Into<String>) -> Self {
    let key = key.into();
    Self::new(S3ErrorCode::NoSuchKey, "The specified key does not exist.").with_resource(key)
  }

  pub fn bucket_already_exists(bucket: impl Into<String>) -> Self {
    let bucket = bucket.into();
    Self::new(
      S3ErrorCode::BucketAlreadyExists,
      "The requested bucket name is not available.",
    )
    .with_resource(bucket)
  }

  pub fn bucket_not_empty(bucket: impl Into<String>) -> Self {
    let bucket = bucket.into();
    Self::new(
      S3ErrorCode::BucketNotEmpty,
      "The bucket you tried to delete is not empty.",
    )
    .with_resource(bucket)
  }

  pub fn internal_error(message: impl Into<String>) -> Self {
    Self::new(S3ErrorCode::InternalError, message)
  }

  pub fn invalid_argument(message: impl Into<String>) -> Self {
    Self::new(S3ErrorCode::InvalidArgument, message)
  }

  pub fn invalid_bucket_name(name: impl Into<String>) -> Self {
    Self::new(
      S3ErrorCode::InvalidBucketName,
      format!("The specified bucket is not valid: {}", name.into()),
    )
  }

  pub fn no_such_upload(upload_id: impl Into<String>) -> Self {
    Self::new(
      S3ErrorCode::NoSuchUpload,
      "The specified upload does not exist.",
    )
    .with_resource(upload_id)
  }

  /// Convert to XML error response
  pub fn to_xml(&self) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Error>\n");
    xml.push_str(&format!("  <Code>{}</Code>\n", self.code.as_str()));
    xml.push_str(&format!(
      "  <Message>{}</Message>\n",
      escape_xml(&self.message)
    ));
    if let Some(ref resource) = self.resource {
      xml.push_str(&format!(
        "  <Resource>{}</Resource>\n",
        escape_xml(resource)
      ));
    }
    if let Some(ref request_id) = self.request_id {
      xml.push_str(&format!(
        "  <RequestId>{}</RequestId>\n",
        escape_xml(request_id)
      ));
    }
    xml.push_str("</Error>");
    xml
  }
}

impl IntoResponse for S3Error {
  fn into_response(self) -> Response {
    let body = self.to_xml();
    let status = self.code.http_status();
    (status, [("Content-Type", "application/xml")], body).into_response()
  }
}

impl From<anyhow::Error> for S3Error {
  fn from(e: anyhow::Error) -> Self {
    S3Error::internal_error(e.to_string())
  }
}

impl From<std::io::Error> for S3Error {
  fn from(e: std::io::Error) -> Self {
    S3Error::internal_error(e.to_string())
  }
}

impl fmt::Display for S3Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}: {}", self.code.as_str(), self.message)
  }
}

impl std::error::Error for S3Error {}

fn escape_xml(s: &str) -> String {
  s.replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('\'', "&apos;")
    .replace('"', "&quot;")
}
