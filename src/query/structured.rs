use crate::db::sanitize::{escape_string, validate_identifier, validate_numeric};
use crate::db::SqlDialect;
use crate::types::{
  ChangesOptions, FieldCondition, FilterOperator, FilterSpec, LogicalFilter, OrderBySpec,
  OrderDirection, QuerySpec, SortSpec, StructuredFilter, StructuredQuery, StructuredSortDirection,
};

/// Compiler for structured queries (no JS evaluation needed)
pub struct StructuredCompiler {
  dialect: SqlDialect,
}

impl StructuredCompiler {
  pub fn new(dialect: SqlDialect) -> Self {
    Self { dialect }
  }

  /// Convert a StructuredQuery to a QuerySpec
  pub fn compile(&self, query: &StructuredQuery) -> Result<QuerySpec, anyhow::Error> {
    let filter = query
      .filter
      .as_ref()
      .map(|f| self.compile_filter(f))
      .transpose()?;

    let order_by = query.sort.as_ref().and_then(|specs| {
      specs.first().map(|s| OrderBySpec {
        field: s.field.clone(),
        direction: match s.direction {
          StructuredSortDirection::Asc => OrderDirection::Asc,
          StructuredSortDirection::Desc => OrderDirection::Desc,
        },
      })
    });

    let changes = query.changes.as_ref().map(|c| ChangesOptions {
      include_initial: c.include_initial,
    });

    Ok(QuerySpec {
      table: query.table.clone(),
      filter,
      map: None,
      order_by,
      limit: query.limit,
      offset: query.skip,
      changes,
    })
  }

  /// Compile a structured filter to FilterSpec with SQL
  fn compile_filter(&self, filter: &StructuredFilter) -> Result<FilterSpec, anyhow::Error> {
    let sql = self.filter_to_sql(filter)?;
    Ok(FilterSpec {
      js_code: String::new(),
      compiled_sql: Some(sql),
    })
  }

  /// Convert a StructuredFilter to SQL WHERE clause
  fn filter_to_sql(&self, filter: &StructuredFilter) -> Result<String, anyhow::Error> {
    match filter {
      StructuredFilter::Logical(logical) => self.logical_to_sql(logical),
      StructuredFilter::Fields(fields) => {
        let parts: Result<Vec<String>, _> = fields
          .iter()
          .map(|(field, cond)| self.field_condition_to_sql(field, cond))
          .collect();
        let parts = parts?;
        if parts.is_empty() {
          Ok("true".to_string())
        } else if parts.len() == 1 {
          Ok(parts.into_iter().next().unwrap())
        } else {
          Ok(format!("({})", parts.join(" AND ")))
        }
      }
    }
  }

  /// Convert logical operators to SQL
  fn logical_to_sql(&self, logical: &LogicalFilter) -> Result<String, anyhow::Error> {
    match logical {
      LogicalFilter::And(filters) => {
        let parts: Result<Vec<String>, _> = filters.iter().map(|f| self.filter_to_sql(f)).collect();
        let parts = parts?;
        Ok(format!("({})", parts.join(" AND ")))
      }
      LogicalFilter::Or(filters) => {
        let parts: Result<Vec<String>, _> = filters.iter().map(|f| self.filter_to_sql(f)).collect();
        let parts = parts?;
        Ok(format!("({})", parts.join(" OR ")))
      }
      LogicalFilter::Not(filter) => {
        let inner = self.filter_to_sql(filter)?;
        Ok(format!("NOT ({})", inner))
      }
    }
  }

  /// Convert a field condition to SQL
  fn field_condition_to_sql(
    &self,
    field: &str,
    condition: &FieldCondition,
  ) -> Result<String, anyhow::Error> {
    validate_identifier(field)?;

    match condition {
      FieldCondition::Operator(op) => self.operator_to_sql(field, op),
      FieldCondition::Value(v) => self.value_eq_sql(field, v),
    }
  }

  /// Convert a filter operator to SQL
  fn operator_to_sql(&self, field: &str, op: &FilterOperator) -> Result<String, anyhow::Error> {
    match op {
      FilterOperator::Eq(v) => self.comparison_sql(field, "=", v),
      FilterOperator::Ne(v) => self.comparison_sql(field, "!=", v),
      FilterOperator::Gt(v) => self.numeric_comparison_sql(field, ">", v),
      FilterOperator::Gte(v) => self.numeric_comparison_sql(field, ">=", v),
      FilterOperator::Lt(v) => self.numeric_comparison_sql(field, "<", v),
      FilterOperator::Lte(v) => self.numeric_comparison_sql(field, "<=", v),
      FilterOperator::In(values) => self.in_sql(field, values, false),
      FilterOperator::NotIn(values) => self.in_sql(field, values, true),
      FilterOperator::Contains(s) => self.like_sql(field, s, "%", "%"),
      FilterOperator::StartsWith(s) => self.like_sql(field, s, "", "%"),
      FilterOperator::EndsWith(s) => self.like_sql(field, s, "%", ""),
      FilterOperator::Exists(exists) => self.exists_sql(field, *exists),
    }
  }

  /// Generate SQL for equality comparison
  fn comparison_sql(
    &self,
    field: &str,
    sql_op: &str,
    value: &serde_json::Value,
  ) -> Result<String, anyhow::Error> {
    match value {
      serde_json::Value::Null => {
        if sql_op == "=" {
          Ok(format!("{} IS NULL", self.dialect.json_text(field)))
        } else {
          Ok(format!("{} IS NOT NULL", self.dialect.json_text(field)))
        }
      }
      serde_json::Value::Bool(b) => Ok(format!(
        "{} = {}",
        self.dialect.json_bool(field),
        if *b { "true" } else { "false" }
      )),
      serde_json::Value::Number(n) => {
        let num_str = n.to_string();
        validate_numeric(&num_str)?;
        Ok(format!(
          "{} {} {}",
          self.dialect.json_numeric(field),
          sql_op,
          num_str
        ))
      }
      serde_json::Value::String(s) => {
        let escaped = escape_string(s)?;
        Ok(format!(
          "{} {} '{}'",
          self.dialect.json_text(field),
          sql_op,
          escaped
        ))
      }
      _ => Err(anyhow::anyhow!(
        "Unsupported value type for equality comparison"
      )),
    }
  }

  /// Generate SQL for numeric comparison (>, <, >=, <=)
  fn numeric_comparison_sql(
    &self,
    field: &str,
    sql_op: &str,
    value: &serde_json::Value,
  ) -> Result<String, anyhow::Error> {
    match value {
      serde_json::Value::Number(n) => {
        let num_str = n.to_string();
        validate_numeric(&num_str)?;
        Ok(format!(
          "{} {} {}",
          self.dialect.json_numeric(field),
          sql_op,
          num_str
        ))
      }
      _ => Err(anyhow::anyhow!(
        "Numeric comparison requires a number value"
      )),
    }
  }

  /// Generate SQL for value equality (shorthand)
  fn value_eq_sql(&self, field: &str, value: &serde_json::Value) -> Result<String, anyhow::Error> {
    self.comparison_sql(field, "=", value)
  }

  /// Generate SQL for IN/NOT IN
  fn in_sql(
    &self,
    field: &str,
    values: &[serde_json::Value],
    negate: bool,
  ) -> Result<String, anyhow::Error> {
    if values.is_empty() {
      return Ok(if negate {
        "true".to_string()
      } else {
        "false".to_string()
      });
    }

    let formatted: Result<Vec<String>, _> = values.iter().map(|v| self.format_value(v)).collect();
    let formatted = formatted?;

    let op = if negate { "NOT IN" } else { "IN" };
    Ok(format!(
      "{} {} ({})",
      self.dialect.json_text(field),
      op,
      formatted.join(", ")
    ))
  }

  /// Generate SQL for LIKE operations
  fn like_sql(
    &self,
    field: &str,
    value: &str,
    prefix: &str,
    suffix: &str,
  ) -> Result<String, anyhow::Error> {
    let escaped = escape_string(value)?;
    let like_escaped = escaped.replace('%', "\\%").replace('_', "\\_");
    Ok(format!(
      "{} LIKE '{}{}{}' ESCAPE '\\'",
      self.dialect.json_text(field),
      prefix,
      like_escaped,
      suffix
    ))
  }

  /// Generate SQL for EXISTS check
  fn exists_sql(&self, field: &str, exists: bool) -> Result<String, anyhow::Error> {
    if exists {
      Ok(format!("{} IS NOT NULL", self.dialect.json_text(field)))
    } else {
      Ok(format!("{} IS NULL", self.dialect.json_text(field)))
    }
  }

  /// Format a JSON value for SQL
  fn format_value(&self, value: &serde_json::Value) -> Result<String, anyhow::Error> {
    match value {
      serde_json::Value::Null => Ok("NULL".to_string()),
      serde_json::Value::Bool(b) => Ok(if *b { "true" } else { "false" }.to_string()),
      serde_json::Value::Number(n) => {
        let s = n.to_string();
        validate_numeric(&s)?;
        Ok(s)
      }
      serde_json::Value::String(s) => {
        let escaped = escape_string(s)?;
        Ok(format!("'{}'", escaped))
      }
      _ => Err(anyhow::anyhow!("Unsupported value type")),
    }
  }
}

impl Default for StructuredCompiler {
  fn default() -> Self {
    Self::new(SqlDialect::Postgres)
  }
}

/// Convert a list of SortSpec to the first OrderBySpec (for compatibility)
pub fn sort_specs_to_order_by(specs: &[SortSpec]) -> Option<OrderBySpec> {
  specs.first().map(|s| OrderBySpec {
    field: s.field.clone(),
    direction: match s.direction {
      StructuredSortDirection::Asc => OrderDirection::Asc,
      StructuredSortDirection::Desc => OrderDirection::Desc,
    },
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap;

  fn pg_compiler() -> StructuredCompiler {
    StructuredCompiler::new(SqlDialect::Postgres)
  }

  fn sqlite_compiler() -> StructuredCompiler {
    StructuredCompiler::new(SqlDialect::Sqlite)
  }

  #[test]
  fn compile_simple_eq() {
    let compiler = pg_compiler();
    let mut fields = HashMap::new();
    fields.insert(
      "status".to_string(),
      FieldCondition::Operator(FilterOperator::Eq(serde_json::json!("active"))),
    );
    let filter = StructuredFilter::Fields(fields);
    let sql = compiler.filter_to_sql(&filter).unwrap();
    assert_eq!(sql, "data->>'status' = 'active'");
  }

  #[test]
  fn compile_numeric_gt() {
    let compiler = pg_compiler();
    let mut fields = HashMap::new();
    fields.insert(
      "age".to_string(),
      FieldCondition::Operator(FilterOperator::Gt(serde_json::json!(21))),
    );
    let filter = StructuredFilter::Fields(fields);
    let sql = compiler.filter_to_sql(&filter).unwrap();
    assert_eq!(sql, "(data->'age')::numeric > 21");
  }

  #[test]
  fn compile_and_condition() {
    let compiler = pg_compiler();

    let mut f1 = HashMap::new();
    f1.insert(
      "age".to_string(),
      FieldCondition::Operator(FilterOperator::Gt(serde_json::json!(21))),
    );

    let mut f2 = HashMap::new();
    f2.insert(
      "status".to_string(),
      FieldCondition::Operator(FilterOperator::Eq(serde_json::json!("active"))),
    );

    let filter = StructuredFilter::Logical(LogicalFilter::And(vec![
      StructuredFilter::Fields(f1),
      StructuredFilter::Fields(f2),
    ]));

    let sql = compiler.filter_to_sql(&filter).unwrap();
    assert!(sql.contains("AND"));
    assert!(sql.contains("age"));
    assert!(sql.contains("status"));
  }

  #[test]
  fn compile_in_operator() {
    let compiler = pg_compiler();
    let mut fields = HashMap::new();
    fields.insert(
      "role".to_string(),
      FieldCondition::Operator(FilterOperator::In(vec![
        serde_json::json!("admin"),
        serde_json::json!("moderator"),
      ])),
    );
    let filter = StructuredFilter::Fields(fields);
    let sql = compiler.filter_to_sql(&filter).unwrap();
    assert!(sql.contains("IN"));
    assert!(sql.contains("'admin'"));
    assert!(sql.contains("'moderator'"));
  }

  #[test]
  fn compile_like_contains() {
    let compiler = pg_compiler();
    let mut fields = HashMap::new();
    fields.insert(
      "name".to_string(),
      FieldCondition::Operator(FilterOperator::Contains("john".to_string())),
    );
    let filter = StructuredFilter::Fields(fields);
    let sql = compiler.filter_to_sql(&filter).unwrap();
    assert!(sql.contains("LIKE '%john%'"));
  }

  #[test]
  fn compile_sqlite_dialect() {
    let compiler = sqlite_compiler();
    let mut fields = HashMap::new();
    fields.insert(
      "age".to_string(),
      FieldCondition::Operator(FilterOperator::Gt(serde_json::json!(21))),
    );
    let filter = StructuredFilter::Fields(fields);
    let sql = compiler.filter_to_sql(&filter).unwrap();
    assert!(sql.contains("json_extract"));
    assert!(sql.contains("CAST"));
  }

  #[test]
  fn compile_full_query() {
    let compiler = pg_compiler();
    let query = StructuredQuery {
      table: "users".to_string(),
      filter: Some(StructuredFilter::Fields({
        let mut m = HashMap::new();
        m.insert(
          "age".to_string(),
          FieldCondition::Operator(FilterOperator::Gt(serde_json::json!(21))),
        );
        m
      })),
      sort: Some(vec![SortSpec {
        field: "name".to_string(),
        direction: StructuredSortDirection::Asc,
      }]),
      limit: Some(10),
      skip: Some(5),
      changes: None,
    };

    let spec = compiler.compile(&query).unwrap();
    assert_eq!(spec.table, "users");
    assert!(spec.filter.is_some());
    assert!(spec.filter.as_ref().unwrap().compiled_sql.is_some());
    assert_eq!(spec.limit, Some(10));
    assert_eq!(spec.offset, Some(5));
    assert!(spec.order_by.is_some());
    assert_eq!(spec.order_by.as_ref().unwrap().field, "name");
  }
}
