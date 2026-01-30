use squirreldb::db::SqlDialect;
use squirreldb::query::QueryCompiler;
use squirreldb::types::CompiledFilter;

#[test]
fn test_compile_string_equality_double_quotes_postgres() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate(r#"doc => doc.status === "active""#);
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "data->>'status' = 'active'"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_string_equality_sqlite() {
  let compiler = QueryCompiler::new(SqlDialect::Sqlite);
  let result = compiler.compile_predicate(r#"doc => doc.status === "active""#);
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "json_extract(data, '$.status') = 'active'"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_string_equality_single_quotes() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("doc => doc.status === 'active'");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "data->>'status' = 'active'"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_numeric_equality_postgres() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("doc => doc.age === 25");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "(data->'age')::numeric = 25"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_numeric_equality_sqlite() {
  let compiler = QueryCompiler::new(SqlDialect::Sqlite);
  let result = compiler.compile_predicate("doc => doc.age === 25");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "CAST(json_extract(data, '$.age') AS REAL) = 25"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_numeric_greater_than() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("doc => doc.age > 21");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "(data->'age')::numeric > 21"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_numeric_less_than() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("u => u.score < 100");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "(data->'score')::numeric < 100"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_numeric_gte() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("x => x.value >= 50");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "(data->'value')::numeric >= 50"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_numeric_lte() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("x => x.value <= 50");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "(data->'value')::numeric <= 50"),
    _ => panic!("Expected SQL filter"),
  }
}

// Tests for array operations (now compiled to SQL)
#[test]
fn test_compile_array_includes_postgres() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("doc => doc.tags.includes('rust')");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "data->'tags' ? 'rust'"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_array_includes_sqlite() {
  let compiler = QueryCompiler::new(SqlDialect::Sqlite);
  let result = compiler.compile_predicate("doc => doc.tags.includes('rust')");
  match result {
    CompiledFilter::Sql(sql) => {
      assert_eq!(
        sql,
        "EXISTS(SELECT 1 FROM json_each(json_extract(data, '$.tags')) WHERE value = 'rust')"
      )
    }
    _ => panic!("Expected SQL filter"),
  }
}

// Tests for string operations (now compiled to SQL)
#[test]
fn test_compile_string_starts_with_postgres() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("doc => doc.name.startsWith('A')");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "data->>'name' LIKE 'A%'"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_string_ends_with_postgres() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("doc => doc.name.endsWith('son')");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "data->>'name' LIKE '%son'"),
    _ => panic!("Expected SQL filter"),
  }
}

// Tests for array length operations
#[test]
fn test_compile_array_length_postgres() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("doc => doc.items.length > 5");
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "jsonb_array_length(data->'items') > 5"),
    _ => panic!("Expected SQL filter"),
  }
}

#[test]
fn test_compile_array_length_sqlite() {
  let compiler = QueryCompiler::new(SqlDialect::Sqlite);
  let result = compiler.compile_predicate("doc => doc.items.length >= 3");
  match result {
    CompiledFilter::Sql(sql) => {
      assert_eq!(sql, "json_array_length(json_extract(data, '$.items')) >= 3")
    }
    _ => panic!("Expected SQL filter"),
  }
}

// Test for expressions that still fall back to JS
#[test]
fn test_fallback_to_js_unsupported_method() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  // .slice() is not supported, should fall back to JS
  let result = compiler.compile_predicate("doc => doc.items.slice(0, 5).length > 0");
  match result {
    CompiledFilter::Js(js) => assert!(js.contains("slice")),
    _ => panic!("Expected JS filter for unsupported method"),
  }
}

#[test]
fn test_sql_injection_prevention() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate(r#"doc => doc.name === "O'Brien""#);
  match result {
    CompiledFilter::Sql(sql) => assert_eq!(sql, "data->>'name' = 'O''Brien'"),
    _ => panic!("Expected SQL filter"),
  }
}
