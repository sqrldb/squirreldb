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

  /// Generate SQL for accessing a JSON array field (for array operations)
  fn json_array(&self, field: &str) -> String {
    match self.dialect {
      SqlDialect::Postgres => {
        let parts: Vec<&str> = field.split('.').collect();
        if parts.len() == 1 {
          format!("data->'{}'", parts[0])
        } else {
          let mut result = "data".to_string();
          for part in &parts {
            result.push_str(&format!("->'{}'", part));
          }
          result
        }
      }
      SqlDialect::Sqlite => format!("json_extract(data, '$.{}')", field),
    }
  }

  /// Generate SQL for array length operation
  fn json_array_length(&self, field: &str) -> String {
    match self.dialect {
      SqlDialect::Postgres => format!("jsonb_array_length({})", self.json_array(field)),
      SqlDialect::Sqlite => format!("json_array_length({})", self.json_array(field)),
    }
  }

  /// Generate SQL for array contains operation (checks if array contains a value)
  fn json_array_contains(&self, field: &str, value: &str) -> Option<String> {
    // Validate and escape the value
    let (is_string, clean_value) = if (value.starts_with('"') && value.ends_with('"'))
      || (value.starts_with('\'') && value.ends_with('\''))
    {
      let inner = &value[1..value.len() - 1];
      let escaped = escape_string(inner).ok()?;
      (true, escaped)
    } else if validate_numeric(value).is_ok() {
      (false, value.to_string())
    } else {
      return None;
    };

    match self.dialect {
      SqlDialect::Postgres => {
        // Use the ? operator for JSONB array containment
        if is_string {
          Some(format!("{} ? '{}'", self.json_array(field), clean_value))
        } else {
          // For numbers, use @> containment operator
          Some(format!(
            "{} @> '[{}]'::jsonb",
            self.json_array(field),
            clean_value
          ))
        }
      }
      SqlDialect::Sqlite => {
        // SQLite: use json_each to check for value in array
        if is_string {
          Some(format!(
            "EXISTS(SELECT 1 FROM json_each({}) WHERE value = '{}')",
            self.json_array(field),
            clean_value
          ))
        } else {
          Some(format!(
            "EXISTS(SELECT 1 FROM json_each({}) WHERE value = {})",
            self.json_array(field),
            clean_value
          ))
        }
      }
    }
  }

  /// Generate SQL for string startsWith operation
  fn string_starts_with(&self, field: &str, value: &str) -> Option<String> {
    let inner = extract_string_value(value)?;
    let escaped = escape_string(inner).ok()?;
    // Escape LIKE wildcards in the value itself
    let like_escaped = escaped.replace('%', "\\%").replace('_', "\\_");
    Some(format!(
      "{} LIKE '{}%'",
      self.dialect.json_text(field),
      like_escaped
    ))
  }

  /// Generate SQL for string endsWith operation
  fn string_ends_with(&self, field: &str, value: &str) -> Option<String> {
    let inner = extract_string_value(value)?;
    let escaped = escape_string(inner).ok()?;
    let like_escaped = escaped.replace('%', "\\%").replace('_', "\\_");
    Some(format!(
      "{} LIKE '%{}'",
      self.dialect.json_text(field),
      like_escaped
    ))
  }

  /// Generate SQL for string contains operation
  fn string_contains(&self, field: &str, value: &str) -> Option<String> {
    let inner = extract_string_value(value)?;
    let escaped = escape_string(inner).ok()?;
    let like_escaped = escaped.replace('%', "\\%").replace('_', "\\_");
    Some(format!(
      "{} LIKE '%{}%'",
      self.dialect.json_text(field),
      like_escaped
    ))
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

    // Handle negation: !doc.field
    if let Some(negated) = expr.strip_prefix('!') {
      let inner = negated.trim();
      if let Some(rest) = inner.strip_prefix(&prefix) {
        let field = rest.trim();
        if is_valid_field_path(field) && validate_identifier(field).is_ok() {
          return Some(format!(
            "({} = false OR {} IS NULL)",
            self.dialect.json_bool(field),
            self.dialect.json_text(field)
          ));
        }
      }
      return None;
    }

    if !expr.starts_with(&prefix) {
      return None;
    }

    let rest = &expr[prefix.len()..];

    // Try array/string method calls first
    if let Some(sql) = self.try_compile_method_call(rest) {
      return Some(sql);
    }

    // Try .length comparison (e.g., doc.items.length > 5)
    if let Some(sql) = self.try_compile_length_comparison(rest) {
      return Some(sql);
    }

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

  /// Try to compile method calls like .includes(), .startsWith(), .endsWith()
  fn try_compile_method_call(&self, rest: &str) -> Option<String> {
    // Match patterns like: field.includes('value') or field.startsWith("value")
    // Regex-like parsing: find method name and argument

    // Look for .includes(
    if let Some(pos) = rest.find(".includes(") {
      let field = &rest[..pos];
      if !is_valid_field_path(field) || validate_identifier(field).is_err() {
        return None;
      }
      let after = &rest[pos + 10..]; // skip ".includes("
      let end = after.find(')')?;
      let arg = after[..end].trim();
      return self.json_array_contains(field, arg);
    }

    // Look for .startsWith(
    if let Some(pos) = rest.find(".startsWith(") {
      let field = &rest[..pos];
      if !is_valid_field_path(field) || validate_identifier(field).is_err() {
        return None;
      }
      let after = &rest[pos + 12..]; // skip ".startsWith("
      let end = after.find(')')?;
      let arg = after[..end].trim();
      return self.string_starts_with(field, arg);
    }

    // Look for .endsWith(
    if let Some(pos) = rest.find(".endsWith(") {
      let field = &rest[..pos];
      if !is_valid_field_path(field) || validate_identifier(field).is_err() {
        return None;
      }
      let after = &rest[pos + 10..]; // skip ".endsWith("
      let end = after.find(')')?;
      let arg = after[..end].trim();
      return self.string_ends_with(field, arg);
    }

    // Look for .contains( (alias for string contains, not array)
    // Note: For clarity, we use .includes() for arrays and this could be string contains
    if let Some(pos) = rest.find(".contains(") {
      let field = &rest[..pos];
      if !is_valid_field_path(field) || validate_identifier(field).is_err() {
        return None;
      }
      let after = &rest[pos + 10..]; // skip ".contains("
      let end = after.find(')')?;
      let arg = after[..end].trim();
      return self.string_contains(field, arg);
    }

    None
  }

  /// Try to compile .length comparisons like doc.items.length > 5
  fn try_compile_length_comparison(&self, rest: &str) -> Option<String> {
    // Find .length followed by comparison operator
    let length_pos = rest.find(".length")?;
    let field = &rest[..length_pos];

    if !is_valid_field_path(field) || validate_identifier(field).is_err() {
      return None;
    }

    let after_length = &rest[length_pos + 7..].trim(); // skip ".length"

    // Parse comparison operator and value
    for op in ["===", "!==", "==", "!=", ">=", "<=", ">", "<"] {
      if let Some(remainder) = after_length.strip_prefix(op) {
        let value = remainder.trim();
        if validate_numeric(value).is_ok() {
          let sql_op = match op {
            "===" | "==" => "=",
            "!==" | "!=" => "!=",
            ">" => ">",
            "<" => "<",
            ">=" => ">=",
            "<=" => "<=",
            _ => return None,
          };
          return Some(format!(
            "{} {} {}",
            self.json_array_length(field),
            sql_op,
            value
          ));
        }
      }
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

/// Extract string value from quoted string (returns inner content)
fn extract_string_value(value: &str) -> Option<&str> {
  if (value.starts_with('"') && value.ends_with('"'))
    || (value.starts_with('\'') && value.ends_with('\''))
  {
    Some(&value[1..value.len() - 1])
  } else {
    None
  }
}
