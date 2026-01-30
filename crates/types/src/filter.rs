use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Structured query sent from SDKs (alternative to JS string queries)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredQuery {
  pub table: String,
  #[serde(default)]
  pub filter: Option<StructuredFilter>,
  #[serde(default)]
  pub sort: Option<Vec<SortSpec>>,
  #[serde(default)]
  pub limit: Option<usize>,
  #[serde(default)]
  pub skip: Option<usize>,
  #[serde(default)]
  pub changes: Option<ChangesSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortSpec {
  pub field: String,
  #[serde(default)]
  pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
  #[default]
  Asc,
  Desc,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChangesSpec {
  #[serde(default, rename = "includeInitial")]
  pub include_initial: bool,
}

/// Structured filter - can be either a field condition or a logical operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StructuredFilter {
  /// Logical operators: { "$and": [...], "$or": [...], "$not": {...} }
  Logical(LogicalFilter),
  /// Field conditions: { "age": { "$gt": 21 }, "name": { "$eq": "Alice" } }
  Fields(HashMap<String, FieldCondition>),
}

/// Logical operators for combining filters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogicalFilter {
  #[serde(rename = "$and")]
  And(Vec<StructuredFilter>),
  #[serde(rename = "$or")]
  Or(Vec<StructuredFilter>),
  #[serde(rename = "$not")]
  Not(Box<StructuredFilter>),
}

/// Condition on a single field
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldCondition {
  /// Operator-based: { "$gt": 21 }
  Operator(FilterOperator),
  /// Direct equality: "active" (shorthand for { "$eq": "active" })
  Value(serde_json::Value),
}

/// Filter operators matching MongoDB-style syntax
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOperator {
  #[serde(rename = "$eq")]
  Eq(serde_json::Value),
  #[serde(rename = "$ne")]
  Ne(serde_json::Value),
  #[serde(rename = "$gt")]
  Gt(serde_json::Value),
  #[serde(rename = "$gte")]
  Gte(serde_json::Value),
  #[serde(rename = "$lt")]
  Lt(serde_json::Value),
  #[serde(rename = "$lte")]
  Lte(serde_json::Value),
  #[serde(rename = "$in")]
  In(Vec<serde_json::Value>),
  #[serde(rename = "$nin")]
  NotIn(Vec<serde_json::Value>),
  #[serde(rename = "$contains")]
  Contains(String),
  #[serde(rename = "$startsWith")]
  StartsWith(String),
  #[serde(rename = "$endsWith")]
  EndsWith(String),
  #[serde(rename = "$exists")]
  Exists(bool),
}

impl StructuredQuery {
  /// Check if this query is for a change subscription
  pub fn is_changes(&self) -> bool {
    self.changes.is_some()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn deserialize_simple_filter() {
    let json = r#"{"table": "users", "filter": {"age": {"$gt": 21}}}"#;
    let query: StructuredQuery = serde_json::from_str(json).unwrap();
    assert_eq!(query.table, "users");
    assert!(query.filter.is_some());
  }

  #[test]
  fn deserialize_logical_filter() {
    let json = r#"{
      "table": "users",
      "filter": {
        "$and": [
          {"age": {"$gt": 21}},
          {"status": {"$eq": "active"}}
        ]
      }
    }"#;
    let query: StructuredQuery = serde_json::from_str(json).unwrap();
    assert_eq!(query.table, "users");
    assert!(query.filter.is_some());
  }

  #[test]
  fn deserialize_with_sort_and_limit() {
    let json = r#"{
      "table": "users",
      "sort": [{"field": "name", "direction": "asc"}],
      "limit": 10,
      "skip": 5
    }"#;
    let query: StructuredQuery = serde_json::from_str(json).unwrap();
    assert_eq!(query.table, "users");
    assert_eq!(query.limit, Some(10));
    assert_eq!(query.skip, Some(5));
    assert!(query.sort.is_some());
  }

  #[test]
  fn deserialize_changes_subscription() {
    let json = r#"{
      "table": "orders",
      "filter": {"status": {"$eq": "pending"}},
      "changes": {"includeInitial": true}
    }"#;
    let query: StructuredQuery = serde_json::from_str(json).unwrap();
    assert!(query.is_changes());
    assert!(query.changes.as_ref().unwrap().include_initial);
  }
}
