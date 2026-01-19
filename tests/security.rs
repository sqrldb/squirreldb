//! Security tests for SquirrelDB
//!
//! These tests verify that security measures are working correctly:
//! - SQL injection prevention
//! - Input validation
//! - Authentication enforcement

use squirreldb::db::sanitize::{
  escape_string, validate_collection_name, validate_identifier, validate_limit, validate_numeric,
  validate_operator, validate_order_direction, SqlSanitizeError,
};
use squirreldb::db::{DatabaseBackend, SqlDialect, SqliteBackend};
use squirreldb::query::QueryCompiler;
use squirreldb::types::CompiledFilter;

// =============================================================================
// SQL Injection Prevention Tests
// =============================================================================

#[test]
fn test_sql_injection_in_string_value() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  // Classic SQL injection attempt
  let result = compiler.compile_predicate(r#"doc => doc.name === "'; DROP TABLE users;--""#);
  match result {
    CompiledFilter::Sql(sql) => {
      // The value should be properly contained within a string literal
      // The injection attempt should be escaped as a string value, not executed as SQL
      // The input quote (') should be escaped to (''), so we expect: = '''...'
      // Structure: opening quote + escaped quote (two quotes) + rest + closing quote
      assert!(
        sql.contains("'''"),
        "Quote should be escaped (opening + doubled internal): {}",
        sql
      );
      // Count quotes - should be even (properly balanced)
      let quote_count = sql.matches('\'').count();
      assert!(quote_count % 2 == 0, "Quotes should be balanced: {}", sql);
    }
    _ => {
      // If it falls back to JS or Hybrid, that's also safe
    }
  }
}

#[test]
fn test_sql_injection_with_backslash() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  // Backslash escape attempt
  let result = compiler.compile_predicate(r#"doc => doc.name === "test\'; DROP TABLE users;--""#);
  if let CompiledFilter::Sql(sql) = result {
    // Backslashes should be escaped
    assert!(sql.contains("\\\\"), "Backslashes should be escaped");
  }
  // Falls back to JS/Hybrid - also safe
}

#[test]
fn test_sql_injection_in_field_name_rejected() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  // Attempt injection via field name
  let result = compiler.compile_predicate(r#"doc => doc["'; DROP TABLE--"] === "test""#);
  // This should fall back to JS (not compile to SQL)
  if let CompiledFilter::Sql(_) = result {
    panic!("Should not compile injection attempt to SQL")
  }
  // Safe - falls back to JS or Hybrid
}

#[test]
fn test_numeric_injection_rejected() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  // Invalid numeric that could be used for injection
  let result = compiler.compile_predicate("doc => doc.age === 1; DROP TABLE users;");
  if let CompiledFilter::Sql(sql) = result {
    assert!(!sql.contains("DROP"), "Should not contain DROP");
  }
  // Safe - falls back to JS or Hybrid
}

#[test]
fn test_multiple_injection_vectors() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  let injection_attempts = vec![
    r#"doc => doc.x === "1' OR '1'='1""#,
    r#"doc => doc.x === "1' UNION SELECT * FROM users--""#,
    r#"doc => doc.x === "1'; DELETE FROM users WHERE '1'='1""#,
    r#"doc => doc.x === "1\x00'; DROP TABLE users;""#,
  ];

  for attempt in injection_attempts {
    let result = compiler.compile_predicate(attempt);
    if let CompiledFilter::Sql(sql) = result {
      // If compiled to SQL, verify it's properly escaped
      // The key security check is that quotes are doubled (escaped)
      // Count the number of single quotes - if injection was NOT escaped,
      // we'd have unbalanced quotes or SQL syntax
      let quote_count = sql.matches('\'').count();
      // A properly escaped string should have an even number of quotes
      // (opening + closing + any escaped internal quotes which are doubled)
      assert!(
        quote_count % 2 == 0,
        "Unbalanced quotes indicate potential injection: {}",
        sql
      );
      // Also verify the SQL structure ends with a closing quote (value is contained)
      assert!(
        sql.ends_with("'") || sql.ends_with("''"),
        "SQL should end with properly closed string: {}",
        sql
      );
    }
    // Falls back to JS or Hybrid - also safe
  }
}

// =============================================================================
// Input Validation Tests
// =============================================================================

#[test]
fn test_identifier_validation_blocks_sql_keywords() {
  let keywords = vec![
    "SELECT", "INSERT", "UPDATE", "DELETE", "DROP", "TRUNCATE", "UNION", "WHERE",
  ];

  for keyword in keywords {
    let result = validate_identifier(keyword);
    assert!(
      result.is_err(),
      "SQL keyword '{}' should be rejected",
      keyword
    );
    match result {
      Err(SqlSanitizeError::ReservedKeyword(_)) => {}
      _ => panic!("Should return ReservedKeyword error for '{}'", keyword),
    }
  }
}

#[test]
fn test_identifier_validation_blocks_special_chars() {
  let invalid = vec![
    "field;", "field--", "field/*", "field'", "field\"", "field\0", "field\n", "field\t",
    "field\\", "field%", "field$", "field@", "field!", "field#", "field&", "field*", "field(",
    "field)", "field=", "field+", "field[", "field]", "field{", "field}", "field|", "field<",
    "field>", "field?", "field/", "field ", " field",
  ];

  for id in invalid {
    assert!(
      validate_identifier(id).is_err(),
      "Identifier '{}' should be rejected",
      id
    );
  }
}

#[test]
fn test_collection_name_validation() {
  // Valid names
  assert!(validate_collection_name("users").is_ok());
  assert!(validate_collection_name("user_data").is_ok());
  assert!(validate_collection_name("_private").is_ok());
  assert!(validate_collection_name("data123").is_ok());

  // Invalid names
  assert!(validate_collection_name("Users").is_err()); // uppercase
  assert!(validate_collection_name("user-data").is_err()); // dash
  assert!(validate_collection_name("user.data").is_err()); // dot
  assert!(validate_collection_name("123data").is_err()); // starts with number
  assert!(validate_collection_name("").is_err()); // empty
  assert!(validate_collection_name("'; DROP TABLE--").is_err()); // injection
}

#[test]
fn test_numeric_validation() {
  // Valid
  assert!(validate_numeric("123").is_ok());
  assert!(validate_numeric("-456").is_ok());
  assert!(validate_numeric("3.14").is_ok());
  assert!(validate_numeric("-0.5").is_ok());
  assert!(validate_numeric("0").is_ok());

  // Invalid
  assert!(validate_numeric("").is_err());
  assert!(validate_numeric("abc").is_err());
  assert!(validate_numeric("1.2.3").is_err()); // multiple dots
  assert!(validate_numeric("-").is_err()); // just minus
  assert!(validate_numeric("1;DROP").is_err()); // injection
  assert!(validate_numeric("1 OR 1=1").is_err()); // injection
}

#[test]
fn test_operator_validation() {
  // Valid
  assert_eq!(validate_operator("=").unwrap(), "=");
  assert_eq!(validate_operator("==").unwrap(), "=");
  assert_eq!(validate_operator("===").unwrap(), "=");
  assert_eq!(validate_operator("!=").unwrap(), "!=");
  assert_eq!(validate_operator(">").unwrap(), ">");
  assert_eq!(validate_operator("<").unwrap(), "<");
  assert_eq!(validate_operator(">=").unwrap(), ">=");
  assert_eq!(validate_operator("<=").unwrap(), "<=");

  // Invalid - potential injection vectors
  assert!(validate_operator("LIKE").is_err());
  assert!(validate_operator("OR").is_err());
  assert!(validate_operator("AND").is_err());
  assert!(validate_operator("UNION").is_err());
  assert!(validate_operator("; DROP").is_err());
  assert!(validate_operator("= OR 1=1 --").is_err());
}

#[test]
fn test_order_direction_validation() {
  assert_eq!(validate_order_direction("ASC").unwrap(), "ASC");
  assert_eq!(validate_order_direction("DESC").unwrap(), "DESC");
  assert_eq!(validate_order_direction("asc").unwrap(), "ASC");
  assert_eq!(validate_order_direction("desc").unwrap(), "DESC");

  assert!(validate_order_direction("ASCENDING").is_err());
  assert!(validate_order_direction("ASC; DROP TABLE").is_err());
  assert!(validate_order_direction("").is_err());
}

#[test]
fn test_limit_validation() {
  assert!(validate_limit(1).is_ok());
  assert!(validate_limit(100).is_ok());
  assert!(validate_limit(1000).is_ok());
  assert!(validate_limit(100000).is_ok());

  // Too large
  assert!(validate_limit(100001).is_err());
  assert!(validate_limit(1000000).is_err());
}

// =============================================================================
// String Escaping Tests
// =============================================================================

#[test]
fn test_escape_string_quotes() {
  assert_eq!(escape_string("hello").unwrap(), "hello");
  assert_eq!(escape_string("it's").unwrap(), "it''s");
  assert_eq!(escape_string("say 'hello'").unwrap(), "say ''hello''");
  assert_eq!(escape_string("O'Brien's").unwrap(), "O''Brien''s");
}

#[test]
fn test_escape_string_backslash() {
  assert_eq!(escape_string("back\\slash").unwrap(), "back\\\\slash");
  assert_eq!(escape_string("\\\\").unwrap(), "\\\\\\\\");
}

#[test]
fn test_escape_string_null_byte_rejected() {
  assert!(escape_string("has\0null").is_err());
  assert!(escape_string("\0").is_err());
  assert!(escape_string("before\0after").is_err());
}

#[test]
fn test_escape_string_unicode() {
  // Unicode should pass through safely
  assert_eq!(escape_string("hello").unwrap(), "hello");
  assert_eq!(
    escape_string("emoji: \u{1F600}").unwrap(),
    "emoji: \u{1F600}"
  );
}

// =============================================================================
// Database Backend Security Tests
// =============================================================================

#[tokio::test]
async fn test_backend_rejects_sql_injection_in_collection() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let injection_attempts = vec![
    "users; DROP TABLE documents;--",
    "users' OR '1'='1",
    "users\"; DELETE FROM documents;--",
    "users/**/UNION/**/SELECT/**/1",
  ];

  for attempt in injection_attempts {
    let result = backend.insert(attempt, serde_json::json!({})).await;
    assert!(
      result.is_err(),
      "Collection name '{}' should be rejected",
      attempt
    );
  }
}

#[tokio::test]
async fn test_backend_rejects_invalid_collection_names() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();

  let too_long = "a".repeat(300);
  let invalid_names: Vec<&str> = vec![
    "CamelCase",   // uppercase
    "with-dashes", // dashes
    "with spaces", // spaces
    "with.dots",   // dots
    "123numeric",  // starts with number
    "",            // empty
    &too_long,     // too long
  ];

  for name in invalid_names {
    let result = backend.insert(name, serde_json::json!({})).await;
    assert!(
      result.is_err(),
      "Collection name '{}' should be rejected",
      name
    );
  }
}

// =============================================================================
// Query Compiler Security Tests
// =============================================================================

#[test]
fn test_compiler_rejects_invalid_field_names() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  // Fields with SQL keywords should not compile to SQL
  let result = compiler.compile_predicate(r#"doc => doc.SELECT === "value""#);
  if let CompiledFilter::Sql(_) = result {
    panic!("SQL keyword field should not compile to SQL")
  }
  // Falls back to JS or Hybrid - safe
}

#[test]
fn test_compiler_unknown_operator_rejected() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  // The LIKE operator is not supported and should fall back to JS
  let result = compiler.compile_predicate(r#"doc => doc.name LIKE "%test%""#);
  if let CompiledFilter::Sql(_) = result {
    panic!("LIKE operator should not compile to SQL")
  }
  // Falls back to JS or Hybrid - safe
}

#[test]
fn test_compiler_boolean_values() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  // Boolean literals should be safe
  let result = compiler.compile_predicate("doc => doc.active === true");
  match result {
    CompiledFilter::Sql(sql) => {
      assert!(sql.contains("= true"));
      assert!(!sql.contains("'true'")); // Not as string
    }
    _ => panic!("Expected SQL compilation"),
  }

  let result = compiler.compile_predicate("doc => doc.active === false");
  match result {
    CompiledFilter::Sql(sql) => {
      assert!(sql.contains("= false"));
    }
    _ => panic!("Expected SQL compilation"),
  }
}

#[test]
fn test_compiler_null_handling() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  let result = compiler.compile_predicate("doc => doc.field === null");
  match result {
    CompiledFilter::Sql(sql) => {
      assert!(sql.contains("IS NULL"));
    }
    _ => panic!("Expected SQL compilation"),
  }

  let result = compiler.compile_predicate("doc => doc.field !== null");
  match result {
    CompiledFilter::Sql(sql) => {
      assert!(sql.contains("IS NOT NULL"));
    }
    _ => panic!("Expected SQL compilation"),
  }
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_empty_string_handling() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  let result = compiler.compile_predicate(r#"doc => doc.name === """#);
  match result {
    CompiledFilter::Sql(sql) => {
      assert!(sql.contains("= ''"));
    }
    _ => panic!("Expected SQL compilation"),
  }
}

#[test]
fn test_very_long_string() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  let long_value = "a".repeat(10000);
  let filter = format!(r#"doc => doc.name === "{}""#, long_value);
  let result = compiler.compile_predicate(&filter);

  // Should either compile safely or fall back to JS/Hybrid
  if let CompiledFilter::Sql(sql) = result {
    assert!(sql.len() > 10000);
  }
  // Falls back to JS or Hybrid - also safe
}

#[test]
fn test_nested_field_access() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  let result = compiler.compile_predicate(r#"doc => doc.address.city === "NYC""#);
  match result {
    CompiledFilter::Sql(sql) => {
      // Should use proper JSON path syntax
      assert!(sql.contains("address") && sql.contains("city"));
      assert!(sql.contains("'NYC'"));
    }
    CompiledFilter::Hybrid { sql, .. } => {
      // Hybrid is also acceptable
      assert!(sql.contains("address") && sql.contains("city"));
    }
    CompiledFilter::Js(_) => panic!("Nested fields should compile to SQL"),
  }
}

#[test]
fn test_deeply_nested_field() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);

  let result = compiler.compile_predicate(r#"doc => doc.level1.level2.level3 === "value""#);
  match result {
    CompiledFilter::Sql(sql) => {
      assert!(sql.contains("level1"));
      assert!(sql.contains("level2"));
      assert!(sql.contains("level3"));
    }
    CompiledFilter::Hybrid { sql, .. } => {
      assert!(sql.contains("level1"));
      assert!(sql.contains("level2"));
      assert!(sql.contains("level3"));
    }
    CompiledFilter::Js(_) => panic!("Deeply nested fields should compile to SQL"),
  }
}
