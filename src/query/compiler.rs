use crate::db::sanitize::{escape_string, validate_identifier, validate_numeric};
use crate::db::SqlDialect;
use crate::types::CompiledFilter;

pub struct QueryCompiler {
  dialect: SqlDialect,
}

impl QueryCompiler {
  pub fn new(dialect: SqlDialect) -> Self {
    Self { dialect }
  }

  pub fn compile_predicate(&self, js: &str) -> CompiledFilter {
    self
      .try_compile_to_sql(js)
      .map(CompiledFilter::Sql)
      .unwrap_or_else(|| CompiledFilter::Js(js.into()))
  }

  fn try_compile_to_sql(&self, js: &str) -> Option<String> {
    let code = js.trim();
    let parts: Vec<&str> = code.splitn(2, "=>").collect();
    if parts.len() != 2 {
      return None;
    }

    let param = parts[0].trim();
    let body = parts[1].trim();
    if !param.chars().all(|c| c.is_alphanumeric() || c == '_') {
      return None;
    }

    // Try to compile the expression (supports logical operators)
    self.compile_expression(body, param)
  }

  /// Compile a JS expression to SQL, handling logical operators && and ||
  fn compile_expression(&self, expr: &str, param: &str) -> Option<String> {
    let expr = expr.trim();

    // Handle parenthesized expressions
    if expr.starts_with('(') && expr.ends_with(')') {
      let inner = &expr[1..expr.len() - 1];
      return self
        .compile_expression(inner, param)
        .map(|s| format!("({})", s));
    }

    // Try to split on logical OR (||) - lowest precedence
    if let Some(sql) = self.try_split_logical(expr, param, "||", "OR") {
      return Some(sql);
    }

    // Try to split on logical AND (&&)
    if let Some(sql) = self.try_split_logical(expr, param, "&&", "AND") {
      return Some(sql);
    }

    // Try to compile as a simple comparison
    self.compile_comparison(expr, param)
  }

  /// Try to split expression on a logical operator
  fn try_split_logical(
    &self,
    expr: &str,
    param: &str,
    js_op: &str,
    sql_op: &str,
  ) -> Option<String> {
    // Find the operator, but not inside parentheses
    // Use char indices to handle unicode correctly
    let mut depth = 0;
    let mut last_byte_pos = 0;
    let mut parts = Vec::new();

    let char_indices = expr.char_indices();
    for (byte_pos, c) in char_indices {
      match c {
        '(' => depth += 1,
        ')' => depth -= 1,
        _ if depth == 0 => {
          // Check if we have the operator starting here
          let remaining = &expr[byte_pos..];
          if remaining.starts_with(js_op) {
            parts.push(expr[last_byte_pos..byte_pos].trim());
            // Skip past the operator
            last_byte_pos = byte_pos + js_op.len();
          }
        }
        _ => {}
      }
    }

    if parts.is_empty() {
      return None;
    }

    // Add the last part
    parts.push(expr[last_byte_pos..].trim());

    // Compile each part
    let sql_parts: Option<Vec<String>> = parts
      .into_iter()
      .map(|p| self.compile_expression(p, param))
      .collect();

    sql_parts.map(|p| p.join(&format!(" {} ", sql_op)))
  }

  /// Compile a single comparison expression
  fn compile_comparison(&self, expr: &str, param: &str) -> Option<String> {
    let prefix = format!("{}.", param);
    if !expr.starts_with(&prefix) {
      return None;
    }

    let rest = &expr[prefix.len()..];

    // Try to parse as comparison with possibly nested field
    if let Some((field, op, value)) = parse_comparison_nested(rest) {
      return self.generate_sql(&field, &op, &value);
    }

    // Handle boolean field access (e.g., doc.active)
    let field = rest.trim();
    if is_valid_field_path(field) && validate_identifier(field).is_ok() {
      return Some(format!("{} = true", self.dialect.json_bool(field)));
    }

    None
  }

  fn generate_sql(&self, field: &str, op: &str, value: &str) -> Option<String> {
    // Validate field name to prevent injection
    if validate_identifier(field).is_err() {
      return None;
    }

    let sql_op = match op {
      "===" | "==" => "=",
      "!==" | "!=" => "!=",
      ">" => ">",
      "<" => "<",
      ">=" => ">=",
      "<=" => "<=",
      _ => return None, // Unknown operator - reject
    };

    // Boolean values
    if value == "true" {
      return Some(format!("{} = true", self.dialect.json_bool(field)));
    }
    if value == "false" {
      return Some(format!("{} = false", self.dialect.json_bool(field)));
    }

    // Null check
    if value == "null" {
      return Some(if sql_op == "=" {
        format!("{} IS NULL", self.dialect.json_text(field))
      } else {
        format!("{} IS NOT NULL", self.dialect.json_text(field))
      });
    }

    // String value - properly escape using sanitize module
    if (value.starts_with('"') && value.ends_with('"'))
      || (value.starts_with('\'') && value.ends_with('\''))
    {
      let inner = &value[1..value.len() - 1];
      // Use proper escaping from sanitize module
      let escaped = escape_string(inner).ok()?;
      return Some(format!(
        "{} {} '{}'",
        self.dialect.json_text(field),
        sql_op,
        escaped
      ));
    }

    // Numeric value - validate properly
    if validate_numeric(value).is_ok() {
      return Some(format!(
        "{} {} {}",
        self.dialect.json_numeric(field),
        sql_op,
        value
      ));
    }

    // Unknown value type - do NOT fall back to raw interpolation
    // This is a security measure to prevent SQL injection
    None
  }
}

impl Default for QueryCompiler {
  fn default() -> Self {
    Self::new(SqlDialect::Postgres)
  }
}

/// Check if a string is a valid field path (e.g., "name", "address.city")
fn is_valid_field_path(s: &str) -> bool {
  !s.is_empty()
    && s
      .split('.')
      .all(|part| !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

/// Parse comparison with support for nested fields (e.g., "address.city === 'NYC'")
fn parse_comparison_nested(s: &str) -> Option<(String, String, String)> {
  for op in ["===", "!==", "==", "!=", ">=", "<=", ">", "<"] {
    if let Some(pos) = s.find(op) {
      let field = s[..pos].trim().to_string();
      let value = s[pos + op.len()..].trim().to_string();
      // Allow nested fields with dots
      if is_valid_field_path(&field) {
        return Some((field, op.into(), value));
      }
    }
  }
  None
}
