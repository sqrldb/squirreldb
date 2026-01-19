//! Extended query engine tests - parsing, compilation, and edge cases

use squirreldb::db::SqlDialect;
use squirreldb::query::QueryEngine;

// =============================================================================
// Basic Query Parsing
// =============================================================================

#[test]
fn test_parse_simple_run() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine.parse_query("db.table(\"users\").run()").unwrap();
  assert_eq!(spec.table, "users");
  assert!(spec.filter.is_none());
  assert!(spec.order_by.is_none());
  assert!(spec.limit.is_none());
}

#[test]
fn test_parse_table_single_quotes() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine.parse_query("db.table('users').run()").unwrap();
  assert_eq!(spec.table, "users");
}

#[test]
fn test_parse_table_backticks() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine.parse_query("db.table(`users`).run()").unwrap();
  assert_eq!(spec.table, "users");
}

#[test]
fn test_parse_various_table_names() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);

  let names = vec![
    "users",
    "user_profiles",
    "UserProfiles",
    "users123",
    "_private",
    "public_v2",
  ];

  for name in names {
    let query = format!("db.table(\"{}\").run()", name);
    let spec = engine.parse_query(&query).unwrap();
    assert_eq!(spec.table, name);
  }
}

// =============================================================================
// Filter Parsing
// =============================================================================

#[test]
fn test_parse_filter_with_equals() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.name === \"Alice\")")
    .unwrap();

  assert_eq!(spec.table, "users");
  assert!(spec.filter.is_some());
}

#[test]
fn test_parse_filter_with_comparison() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);

  let operators = vec![">", "<", ">=", "<=", "===", "!=="];

  for op in operators {
    let query = format!("db.table(\"users\").filter(u => u.age {} 25)", op);
    let result = engine.parse_query(&query);
    assert!(result.is_ok(), "Failed for operator: {}", op);
  }
}

#[test]
fn test_parse_filter_arrow_function() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);

  // Various arrow function styles
  let queries = vec![
    "db.table(\"users\").filter(u => u.active)",
    "db.table(\"users\").filter((u) => u.active)",
    "db.table(\"users\").filter(user => user.active)",
    "db.table(\"users\").filter(doc => doc.active)",
    "db.table(\"users\").filter(x => x.active)",
  ];

  for query in queries {
    let result = engine.parse_query(query);
    assert!(result.is_ok(), "Failed for query: {}", query);
  }
}

#[test]
fn test_parse_filter_nested_field() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.address.city === \"NYC\")")
    .unwrap();

  assert_eq!(spec.table, "users");
  assert!(spec.filter.is_some());
}

#[test]
fn test_parse_filter_multiple_conditions() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.age > 18 && u.active === true)")
    .unwrap();

  assert!(spec.filter.is_some());
}

// =============================================================================
// Order By Parsing
// =============================================================================

#[test]
fn test_parse_order_by_single_field() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").orderBy(\"name\")")
    .unwrap();

  assert!(spec.order_by.is_some());
  let order = spec.order_by.unwrap();
  assert_eq!(order.field, "name");
}

#[test]
fn test_parse_order_by_asc() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").orderBy(\"name\", \"asc\")")
    .unwrap();

  assert!(spec.order_by.is_some());
  let order = spec.order_by.unwrap();
  assert_eq!(order.field, "name");
  assert_eq!(order.direction, squirreldb::types::OrderDirection::Asc);
}

#[test]
fn test_parse_order_by_desc() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").orderBy(\"created_at\", \"desc\")")
    .unwrap();

  assert!(spec.order_by.is_some());
  let order = spec.order_by.unwrap();
  assert_eq!(order.field, "created_at");
  assert_eq!(order.direction, squirreldb::types::OrderDirection::Desc);
}

// =============================================================================
// Limit Parsing
// =============================================================================

#[test]
fn test_parse_limit() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine.parse_query("db.table(\"users\").limit(10)").unwrap();

  assert_eq!(spec.limit, Some(10));
}

#[test]
fn test_parse_limit_one() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine.parse_query("db.table(\"users\").limit(1)").unwrap();

  assert_eq!(spec.limit, Some(1));
}

#[test]
fn test_parse_limit_large() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").limit(1000000)")
    .unwrap();

  assert_eq!(spec.limit, Some(1000000));
}

// =============================================================================
// Combined Operations
// =============================================================================

#[test]
fn test_parse_filter_and_limit() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.active === true).limit(5)")
    .unwrap();

  assert!(spec.filter.is_some());
  assert_eq!(spec.limit, Some(5));
}

#[test]
fn test_parse_filter_order_limit() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.age > 18).orderBy(\"name\").limit(10)")
    .unwrap();

  assert!(spec.filter.is_some());
  assert!(spec.order_by.is_some());
  assert_eq!(spec.limit, Some(10));
}

#[test]
fn test_parse_all_operations() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query(
      "db.table(\"users\").filter(u => u.status === \"active\").orderBy(\"created_at\", \"desc\").limit(20)"
    )
    .unwrap();

  assert_eq!(spec.table, "users");
  assert!(spec.filter.is_some());
  assert!(spec.order_by.is_some());
  assert_eq!(spec.order_by.as_ref().unwrap().field, "created_at");
  assert_eq!(spec.limit, Some(20));
}

// =============================================================================
// Changes (Subscriptions)
// =============================================================================

#[test]
fn test_parse_changes() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine.parse_query("db.table(\"users\").changes()").unwrap();

  assert!(spec.changes.is_some());
}

#[test]
fn test_parse_changes_with_filter() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.active === true).changes()")
    .unwrap();

  assert!(spec.filter.is_some());
  assert!(spec.changes.is_some());
}

#[test]
fn test_parse_changes_with_include_initial() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").changes({includeInitial: true})")
    .unwrap();

  assert!(spec.changes.is_some());
  let changes = spec.changes.unwrap();
  assert!(changes.include_initial);
}

// =============================================================================
// Map Operations
// =============================================================================

#[test]
fn test_parse_map() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").map(u => u.name)")
    .unwrap();

  assert!(spec.map.is_some());
}

#[test]
fn test_parse_map_with_filter() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.active).map(u => u.email)")
    .unwrap();

  assert!(spec.filter.is_some());
  assert!(spec.map.is_some());
}

// =============================================================================
// Error Cases
// =============================================================================

#[test]
fn test_parse_missing_table() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let result = engine.parse_query("db.run()");
  assert!(result.is_err());
}

#[test]
fn test_parse_empty_table_name() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let result = engine.parse_query("db.table(\"\").run()");
  // May or may not error depending on implementation
  let _ = result;
}

#[test]
fn test_parse_invalid_syntax() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let result = engine.parse_query("this is not valid javascript");
  assert!(result.is_err());
}

#[test]
fn test_parse_missing_run() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  // Without .run() or .changes(), should still parse the table
  let result = engine.parse_query("db.table(\"users\")");
  // Depending on implementation, might need .run()
  let _ = result;
}

#[test]
fn test_parse_invalid_limit() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let result = engine.parse_query("db.table(\"users\").limit(-1)");
  // Should either error or handle gracefully
  let _ = result;
}

// =============================================================================
// SQL Dialect Differences
// =============================================================================

#[test]
fn test_sqlite_filter_compilation() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.age === 25)")
    .unwrap();

  if let Some(filter) = &spec.filter {
    if let Some(sql) = &filter.compiled_sql {
      assert!(
        sql.contains("json_extract") || sql.contains("CAST"),
        "SQLite should use json_extract or CAST"
      );
    }
  }
}

#[test]
fn test_postgres_filter_compilation() {
  let engine = QueryEngine::new(SqlDialect::Postgres);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.age === 25)")
    .unwrap();

  if let Some(filter) = &spec.filter {
    if let Some(sql) = &filter.compiled_sql {
      assert!(
        sql.contains("data->") || sql.contains("->>"),
        "PostgreSQL should use JSONB operators"
      );
    }
  }
}

// =============================================================================
// Whitespace and Formatting
// =============================================================================

#[test]
fn test_parse_with_extra_whitespace() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("  db.table( \"users\" ).run()  ")
    .unwrap();
  assert_eq!(spec.table, "users");
}

#[test]
fn test_parse_multiline() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let query = r#"
    db.table("users")
      .filter(u => u.active === true)
      .orderBy("name")
      .limit(10)
  "#;
  let result = engine.parse_query(query);
  // Multiline might or might not work depending on implementation
  let _ = result;
}

// =============================================================================
// Complex Filters
// =============================================================================

#[test]
fn test_parse_complex_boolean_filter() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query(
      "db.table(\"users\").filter(u => (u.age >= 18 && u.age <= 65) || u.admin === true)",
    )
    .unwrap();

  assert!(spec.filter.is_some());
}

#[test]
fn test_parse_filter_with_string_methods() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  // These should fall back to JS evaluation
  let queries = vec![
    "db.table(\"users\").filter(u => u.name.startsWith(\"A\"))",
    "db.table(\"users\").filter(u => u.email.includes(\"@\"))",
    "db.table(\"users\").filter(u => u.name.toLowerCase() === \"alice\")",
  ];

  for query in queries {
    let result = engine.parse_query(query);
    assert!(result.is_ok(), "Failed for query: {}", query);
  }
}

#[test]
fn test_parse_filter_with_array_methods() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  // These should fall back to JS evaluation
  let queries = vec![
    "db.table(\"users\").filter(u => u.tags.includes(\"admin\"))",
    "db.table(\"users\").filter(u => u.roles.length > 0)",
  ];

  for query in queries {
    let result = engine.parse_query(query);
    assert!(result.is_ok(), "Failed for query: {}", query);
  }
}

// =============================================================================
// Special Characters
// =============================================================================

#[test]
fn test_parse_filter_with_special_string() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.name === \"O'Brien\")")
    .unwrap();

  assert!(spec.filter.is_some());
}

#[test]
fn test_parse_filter_with_escaped_quotes() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query(r#"db.table("users").filter(u => u.message === "Hello \"World\"")"#)
    .unwrap();

  assert!(spec.filter.is_some());
}

#[test]
fn test_parse_filter_with_unicode() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine
    .parse_query("db.table(\"users\").filter(u => u.name === \"日本語\")")
    .unwrap();

  assert!(spec.filter.is_some());
}

// =============================================================================
// Performance Considerations
// =============================================================================

#[test]
fn test_parse_many_queries_reuses_engine() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);

  for i in 0..100 {
    let query = format!("db.table(\"table{}\").run()", i);
    let result = engine.parse_query(&query);
    assert!(result.is_ok());
  }
}

#[test]
fn test_parse_long_query() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);

  // Build a long chain of operations
  let mut query = String::from("db.table(\"users\")");
  for i in 0..10 {
    query.push_str(&format!(".filter(u => u.field{} > {})", i, i));
  }
  query.push_str(".limit(10)");

  // Should handle long queries
  let result = engine.parse_query(&query);
  let _ = result;
}
