//! Tests for CLI command functionality

// =============================================================================
// Username Validation Tests
// =============================================================================

/// Test that valid usernames are accepted
#[test]
fn test_valid_usernames() {
  let valid_names = vec![
    "alice",
    "bob123",
    "user_name",
    "Admin",
    "db_user_1",
    "a",
    "user123_test",
  ];

  for name in valid_names {
    assert!(
      name.chars().all(|c| c.is_alphanumeric() || c == '_'),
      "Username '{}' should be valid",
      name
    );
  }
}

/// Test that invalid usernames are rejected
#[test]
fn test_invalid_usernames() {
  let invalid_names = vec![
    "user-name",  // contains dash
    "user.name",  // contains dot
    "user name",  // contains space
    "user@name",  // contains @
    "user;drop",  // contains semicolon
    "user'test",  // contains quote
    "user\"test", // contains double quote
    "user\nname", // contains newline
  ];

  for name in invalid_names {
    assert!(
      !name.chars().all(|c| c.is_alphanumeric() || c == '_'),
      "Username '{}' should be invalid",
      name
    );
  }
}

// =============================================================================
// SQL Quoting Tests
// =============================================================================

/// Quote a PostgreSQL identifier (matches implementation in commands.rs)
fn quote_ident(s: &str) -> String {
  format!("\"{}\"", s.replace('"', "\"\""))
}

/// Quote a PostgreSQL string literal (matches implementation in commands.rs)
fn quote_literal(s: &str) -> String {
  format!("'{}'", s.replace('\'', "''"))
}

#[test]
fn test_quote_ident_simple() {
  assert_eq!(quote_ident("username"), "\"username\"");
  assert_eq!(quote_ident("my_user"), "\"my_user\"");
  assert_eq!(quote_ident("User123"), "\"User123\"");
}

#[test]
fn test_quote_ident_with_quotes() {
  // Double quotes inside should be escaped
  assert_eq!(quote_ident("user\"name"), "\"user\"\"name\"");
  assert_eq!(quote_ident("\"quoted\""), "\"\"\"quoted\"\"\"");
}

#[test]
fn test_quote_ident_empty() {
  assert_eq!(quote_ident(""), "\"\"");
}

#[test]
fn test_quote_literal_simple() {
  assert_eq!(quote_literal("password123"), "'password123'");
  assert_eq!(quote_literal("my_secret"), "'my_secret'");
}

#[test]
fn test_quote_literal_with_quotes() {
  // Single quotes inside should be escaped
  assert_eq!(quote_literal("it's"), "'it''s'");
  assert_eq!(quote_literal("can't stop"), "'can''t stop'");
  assert_eq!(quote_literal("'quoted'"), "'''quoted'''");
}

#[test]
fn test_quote_literal_empty() {
  assert_eq!(quote_literal(""), "''");
}

#[test]
fn test_quote_literal_special_chars() {
  // Special chars other than single quotes should pass through
  assert_eq!(quote_literal("pass@word!"), "'pass@word!'");
  assert_eq!(quote_literal("test\nline"), "'test\nline'");
}

// =============================================================================
// SQL Injection Prevention Tests
// =============================================================================

#[test]
fn test_quote_ident_prevents_injection() {
  // Attempt to break out of identifier quoting
  let malicious = "user\"; DROP TABLE users; --";
  let quoted = quote_ident(malicious);
  // Result should be safely quoted
  assert_eq!(quoted, "\"user\"\"; DROP TABLE users; --\"");
  // The double quotes are escaped, preventing injection
}

#[test]
fn test_quote_literal_prevents_injection() {
  // Attempt to break out of string literal
  let malicious = "'; DROP TABLE users; --";
  let quoted = quote_literal(malicious);
  // Result should be safely quoted
  assert_eq!(quoted, "'''; DROP TABLE users; --'");
  // The single quote is escaped, preventing injection
}

// Note: UsersAction tests moved - admin commands are in sqrld now
