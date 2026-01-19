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

#[test]
fn test_fallback_to_js_complex_expression() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("doc => doc.tags.includes('rust')");
  match result {
    CompiledFilter::Js(js) => assert_eq!(js, "doc => doc.tags.includes('rust')"),
    _ => panic!("Expected JS filter"),
  }
}

#[test]
fn test_fallback_to_js_method_call() {
  let compiler = QueryCompiler::new(SqlDialect::Postgres);
  let result = compiler.compile_predicate("doc => doc.name.startsWith('A')");
  match result {
    CompiledFilter::Js(js) => assert!(js.contains("startsWith")),
    _ => panic!("Expected JS filter"),
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
