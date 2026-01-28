use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySpec {
  pub table: String,
  pub filter: Option<FilterSpec>,
  pub map: Option<String>,
  pub order_by: Option<OrderBySpec>,
  pub limit: Option<usize>,
  pub offset: Option<usize>,
  pub changes: Option<ChangesOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSpec {
  pub js_code: String,
  pub compiled_sql: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBySpec {
  pub field: String,
  pub direction: OrderDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderDirection {
  #[default]
  Asc,
  Desc,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChangesOptions {
  #[serde(default)]
  pub include_initial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompiledFilter {
  Sql(String),
  Js(String),
  Hybrid { sql: String, js: String },
}
