use squirreldb::db::SqlDialect;
use squirreldb::query::QueryEngine;

#[test]
fn test_parse_simple_query() {
  let engine = QueryEngine::new(SqlDialect::Postgres);
  let spec = engine.parse_query(r#"db.table("users").run()"#).unwrap();
  assert_eq!(spec.table, "users");
  assert!(spec.filter.is_none());
  assert!(spec.map.is_none());
  assert!(spec.limit.is_none());
}

#[test]
fn test_parse_query_with_filter() {
  let engine = QueryEngine::new(SqlDialect::Postgres);
  let spec = engine
    .parse_query(r#"db.table("users").filter(doc => doc.age > 21).run()"#)
    .unwrap();
  assert_eq!(spec.table, "users");
  assert!(spec.filter.is_some());
  let filter = spec.filter.unwrap();
  assert!(filter.js_code.contains("doc.age > 21"));
  // Should compile to SQL
  assert!(filter.compiled_sql.is_some());
}

#[test]
fn test_parse_query_with_map() {
  let engine = QueryEngine::new(SqlDialect::Postgres);
  let spec = engine
    .parse_query(r#"db.table("users").map(doc => ({ name: doc.name })).run()"#)
    .unwrap();
  assert_eq!(spec.table, "users");
  assert!(spec.map.is_some());
  assert!(spec.map.unwrap().contains("name"));
}

#[test]
fn test_parse_query_with_limit() {
  let engine = QueryEngine::new(SqlDialect::Postgres);
  let spec = engine
    .parse_query(r#"db.table("posts").limit(10).run()"#)
    .unwrap();
  assert_eq!(spec.table, "posts");
  assert_eq!(spec.limit, Some(10));
}

#[test]
fn test_parse_query_with_order_by() {
  let engine = QueryEngine::new(SqlDialect::Postgres);
  let spec = engine
    .parse_query(r#"db.table("posts").orderBy("created_at", "desc").run()"#)
    .unwrap();
  assert_eq!(spec.table, "posts");
  let order = spec.order_by.unwrap();
  assert_eq!(order.field, "created_at");
  assert_eq!(order.direction, squirreldb::types::OrderDirection::Desc);
}

#[test]
fn test_parse_query_with_changes() {
  let engine = QueryEngine::new(SqlDialect::Postgres);
  let spec = engine
    .parse_query(r#"db.table("messages").changes({ includeInitial: true })"#)
    .unwrap();
  assert_eq!(spec.table, "messages");
  let changes = spec.changes.unwrap();
  assert!(changes.include_initial);
}

#[test]
fn test_parse_complex_query() {
  let engine = QueryEngine::new(SqlDialect::Postgres);
  let spec = engine
    .parse_query(
      r#"
    db.table("orders")
      .filter(o => o.status === "pending")
      .orderBy("created_at", "desc")
      .limit(50)
      .run()
  "#,
    )
    .unwrap();

  assert_eq!(spec.table, "orders");
  assert!(spec.filter.is_some());
  assert_eq!(spec.limit, Some(50));
  let order = spec.order_by.unwrap();
  assert_eq!(order.field, "created_at");
}

#[test]
fn test_parse_invalid_query_missing_table() {
  let engine = QueryEngine::new(SqlDialect::Postgres);
  let result = engine.parse_query("db.run()");
  assert!(result.is_err());
}
