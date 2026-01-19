//! SQL sanitization utilities to prevent injection attacks.
//!
//! This module provides functions for safely handling user input in SQL queries.

/// Maximum length for identifiers (collection names, field names)
pub const MAX_IDENTIFIER_LENGTH: usize = 255;

/// Maximum length for string values in queries
pub const MAX_STRING_VALUE_LENGTH: usize = 65535;

/// Validates that a string is a safe SQL identifier (collection name, field name).
/// Only allows alphanumeric characters, underscores, and dots (for nested fields).
/// Returns an error if the identifier is invalid.
pub fn validate_identifier(s: &str) -> Result<(), SqlSanitizeError> {
  if s.is_empty() {
    return Err(SqlSanitizeError::EmptyIdentifier);
  }

  if s.len() > MAX_IDENTIFIER_LENGTH {
    return Err(SqlSanitizeError::IdentifierTooLong(s.len()));
  }

  // Must start with letter or underscore
  let first = s.chars().next().unwrap();
  if !first.is_ascii_alphabetic() && first != '_' {
    return Err(SqlSanitizeError::InvalidIdentifierStart(first));
  }

  // Check all characters
  for c in s.chars() {
    if !c.is_ascii_alphanumeric() && c != '_' && c != '.' {
      return Err(SqlSanitizeError::InvalidIdentifierChar(c));
    }
  }

  // Check for SQL keywords (case-insensitive)
  let upper = s.to_uppercase();
  if SQL_KEYWORDS.contains(&upper.as_str()) {
    return Err(SqlSanitizeError::ReservedKeyword(s.to_string()));
  }

  // Prevent double dots or leading/trailing dots
  if s.starts_with('.') || s.ends_with('.') || s.contains("..") {
    return Err(SqlSanitizeError::InvalidFieldPath(s.to_string()));
  }

  Ok(())
}

/// Validates a collection name. More restrictive than general identifiers.
/// No dots allowed (not nested), must be lowercase alphanumeric + underscore.
pub fn validate_collection_name(s: &str) -> Result<(), SqlSanitizeError> {
  if s.is_empty() {
    return Err(SqlSanitizeError::EmptyIdentifier);
  }

  if s.len() > MAX_IDENTIFIER_LENGTH {
    return Err(SqlSanitizeError::IdentifierTooLong(s.len()));
  }

  // Must start with letter or underscore
  let first = s.chars().next().unwrap();
  if !first.is_ascii_alphabetic() && first != '_' {
    return Err(SqlSanitizeError::InvalidIdentifierStart(first));
  }

  // Only lowercase alphanumeric and underscore
  for c in s.chars() {
    if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' {
      return Err(SqlSanitizeError::InvalidCollectionChar(c));
    }
  }

  // Check for SQL keywords
  let upper = s.to_uppercase();
  if SQL_KEYWORDS.contains(&upper.as_str()) {
    return Err(SqlSanitizeError::ReservedKeyword(s.to_string()));
  }

  Ok(())
}

/// Escapes a string value for safe inclusion in SQL.
/// Handles single quotes, backslashes, and null bytes.
pub fn escape_string(s: &str) -> Result<String, SqlSanitizeError> {
  if s.len() > MAX_STRING_VALUE_LENGTH {
    return Err(SqlSanitizeError::StringTooLong(s.len()));
  }

  let mut escaped = String::with_capacity(s.len() + 10);

  for c in s.chars() {
    match c {
      '\'' => escaped.push_str("''"),
      '\\' => escaped.push_str("\\\\"),
      '\0' => return Err(SqlSanitizeError::NullByteInString),
      _ => escaped.push(c),
    }
  }

  Ok(escaped)
}

/// Validates that a string is a valid numeric literal.
/// Allows integers, decimals, and negative numbers.
pub fn validate_numeric(s: &str) -> Result<(), SqlSanitizeError> {
  if s.is_empty() {
    return Err(SqlSanitizeError::InvalidNumeric(s.to_string()));
  }

  let mut chars = s.chars().peekable();
  let mut has_digit = false;
  let mut has_dot = false;

  // Optional leading minus
  if chars.peek() == Some(&'-') {
    chars.next();
  }

  // Must have at least one digit
  for c in chars {
    match c {
      '0'..='9' => has_digit = true,
      '.' if !has_dot => has_dot = true,
      '.' => return Err(SqlSanitizeError::InvalidNumeric(s.to_string())),
      _ => return Err(SqlSanitizeError::InvalidNumeric(s.to_string())),
    }
  }

  if !has_digit {
    return Err(SqlSanitizeError::InvalidNumeric(s.to_string()));
  }

  Ok(())
}

/// Validates that a limit value is within acceptable bounds.
pub fn validate_limit(limit: usize) -> Result<(), SqlSanitizeError> {
  const MAX_LIMIT: usize = 100_000;
  if limit > MAX_LIMIT {
    return Err(SqlSanitizeError::LimitTooLarge(limit, MAX_LIMIT));
  }
  Ok(())
}

/// Validates an ORDER BY direction.
pub fn validate_order_direction(dir: &str) -> Result<&'static str, SqlSanitizeError> {
  match dir.to_uppercase().as_str() {
    "ASC" => Ok("ASC"),
    "DESC" => Ok("DESC"),
    _ => Err(SqlSanitizeError::InvalidOrderDirection(dir.to_string())),
  }
}

/// Validates a comparison operator.
pub fn validate_operator(op: &str) -> Result<&'static str, SqlSanitizeError> {
  match op {
    "=" | "==" | "===" => Ok("="),
    "!=" | "!==" | "<>" => Ok("!="),
    ">" => Ok(">"),
    "<" => Ok("<"),
    ">=" => Ok(">="),
    "<=" => Ok("<="),
    _ => Err(SqlSanitizeError::InvalidOperator(op.to_string())),
  }
}

/// SQL sanitization errors
#[derive(Debug, Clone, PartialEq)]
pub enum SqlSanitizeError {
  EmptyIdentifier,
  IdentifierTooLong(usize),
  InvalidIdentifierStart(char),
  InvalidIdentifierChar(char),
  InvalidCollectionChar(char),
  InvalidFieldPath(String),
  ReservedKeyword(String),
  StringTooLong(usize),
  NullByteInString,
  InvalidNumeric(String),
  LimitTooLarge(usize, usize),
  InvalidOrderDirection(String),
  InvalidOperator(String),
}

impl std::fmt::Display for SqlSanitizeError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::EmptyIdentifier => write!(f, "Identifier cannot be empty"),
      Self::IdentifierTooLong(len) => {
        write!(
          f,
          "Identifier too long: {} > {}",
          len, MAX_IDENTIFIER_LENGTH
        )
      }
      Self::InvalidIdentifierStart(c) => {
        write!(
          f,
          "Identifier must start with letter or underscore, got '{}'",
          c
        )
      }
      Self::InvalidIdentifierChar(c) => {
        write!(f, "Invalid character in identifier: '{}'", c)
      }
      Self::InvalidCollectionChar(c) => {
        write!(
          f,
          "Collection names must be lowercase alphanumeric, got '{}'",
          c
        )
      }
      Self::InvalidFieldPath(s) => write!(f, "Invalid field path: {}", s),
      Self::ReservedKeyword(s) => write!(f, "'{}' is a reserved SQL keyword", s),
      Self::StringTooLong(len) => {
        write!(f, "String too long: {} > {}", len, MAX_STRING_VALUE_LENGTH)
      }
      Self::NullByteInString => write!(f, "Null bytes not allowed in strings"),
      Self::InvalidNumeric(s) => write!(f, "Invalid numeric value: {}", s),
      Self::LimitTooLarge(got, max) => write!(f, "Limit {} exceeds maximum {}", got, max),
      Self::InvalidOrderDirection(s) => {
        write!(f, "Invalid order direction '{}', must be ASC or DESC", s)
      }
      Self::InvalidOperator(s) => write!(f, "Invalid operator: {}", s),
    }
  }
}

impl std::error::Error for SqlSanitizeError {}

/// Common SQL keywords that cannot be used as identifiers
const SQL_KEYWORDS: &[&str] = &[
  "SELECT",
  "INSERT",
  "UPDATE",
  "DELETE",
  "DROP",
  "CREATE",
  "ALTER",
  "TABLE",
  "INDEX",
  "FROM",
  "WHERE",
  "AND",
  "OR",
  "NOT",
  "NULL",
  "TRUE",
  "FALSE",
  "ORDER",
  "BY",
  "ASC",
  "DESC",
  "LIMIT",
  "OFFSET",
  "JOIN",
  "LEFT",
  "RIGHT",
  "INNER",
  "OUTER",
  "ON",
  "AS",
  "IN",
  "BETWEEN",
  "LIKE",
  "IS",
  "UNION",
  "ALL",
  "DISTINCT",
  "GROUP",
  "HAVING",
  "INTO",
  "VALUES",
  "SET",
  "CASCADE",
  "RESTRICT",
  "REFERENCES",
  "FOREIGN",
  "PRIMARY",
  "KEY",
  "UNIQUE",
  "CHECK",
  "DEFAULT",
  "CONSTRAINT",
  "TRIGGER",
  "FUNCTION",
  "PROCEDURE",
  "VIEW",
  "DATABASE",
  "SCHEMA",
  "GRANT",
  "REVOKE",
  "COMMIT",
  "ROLLBACK",
  "BEGIN",
  "END",
  "TRANSACTION",
  "TRUNCATE",
  "EXECUTE",
  "EXEC",
];

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_validate_identifier_valid() {
    assert!(validate_identifier("users").is_ok());
    assert!(validate_identifier("_private").is_ok());
    assert!(validate_identifier("user_name").is_ok());
    assert!(validate_identifier("address.city").is_ok());
    assert!(validate_identifier("a1").is_ok());
  }

  #[test]
  fn test_validate_identifier_invalid() {
    assert!(validate_identifier("").is_err());
    assert!(validate_identifier("1start").is_err());
    assert!(validate_identifier("has space").is_err());
    assert!(validate_identifier("has-dash").is_err());
    assert!(validate_identifier("SELECT").is_err());
    assert!(validate_identifier("..double").is_err());
    assert!(validate_identifier(".leading").is_err());
    assert!(validate_identifier("trailing.").is_err());
  }

  #[test]
  fn test_validate_collection_name() {
    assert!(validate_collection_name("users").is_ok());
    assert!(validate_collection_name("user_data").is_ok());
    assert!(validate_collection_name("_temp").is_ok());

    assert!(validate_collection_name("Users").is_err()); // uppercase
    assert!(validate_collection_name("user.data").is_err()); // dot
    assert!(validate_collection_name("user-data").is_err()); // dash
  }

  #[test]
  fn test_escape_string() {
    assert_eq!(escape_string("hello").unwrap(), "hello");
    assert_eq!(escape_string("it's").unwrap(), "it''s");
    assert_eq!(escape_string("back\\slash").unwrap(), "back\\\\slash");
    assert_eq!(escape_string("O'Brien's").unwrap(), "O''Brien''s");
  }

  #[test]
  fn test_escape_string_null_byte() {
    assert!(escape_string("has\0null").is_err());
  }

  #[test]
  fn test_validate_numeric() {
    assert!(validate_numeric("123").is_ok());
    assert!(validate_numeric("-456").is_ok());
    assert!(validate_numeric("3.14").is_ok());
    assert!(validate_numeric("-0.5").is_ok());

    assert!(validate_numeric("").is_err());
    assert!(validate_numeric("abc").is_err());
    assert!(validate_numeric("1.2.3").is_err());
    assert!(validate_numeric("-").is_err());
  }

  #[test]
  fn test_validate_operator() {
    assert_eq!(validate_operator("=").unwrap(), "=");
    assert_eq!(validate_operator("===").unwrap(), "=");
    assert_eq!(validate_operator("!=").unwrap(), "!=");
    assert_eq!(validate_operator(">").unwrap(), ">");
    assert_eq!(validate_operator(">=").unwrap(), ">=");

    assert!(validate_operator("LIKE").is_err());
    assert!(validate_operator("; DROP").is_err());
  }

  #[test]
  fn test_sql_injection_attempts() {
    // These should all fail validation
    assert!(validate_identifier("users; DROP TABLE users;--").is_err());
    assert!(validate_identifier("' OR '1'='1").is_err());
    assert!(validate_collection_name("users/**/OR/**/1=1").is_err());
  }
}
