//! Redis RESP protocol parser and encoder

use std::io::{self, Write};

/// RESP protocol value types
#[derive(Debug, Clone, PartialEq)]
pub enum RespValue {
  /// Simple string (+OK\r\n)
  SimpleString(String),
  /// Error (-ERR message\r\n)
  Error(String),
  /// Integer (:123\r\n)
  Integer(i64),
  /// Bulk string ($5\r\nhello\r\n)
  BulkString(Option<String>),
  /// Array (*2\r\n...)
  Array(Option<Vec<RespValue>>),
}

impl RespValue {
  pub fn ok() -> Self {
    RespValue::SimpleString("OK".to_string())
  }

  pub fn pong() -> Self {
    RespValue::SimpleString("PONG".to_string())
  }

  pub fn null_bulk() -> Self {
    RespValue::BulkString(None)
  }

  pub fn null_array() -> Self {
    RespValue::Array(None)
  }

  pub fn error(msg: &str) -> Self {
    RespValue::Error(msg.to_string())
  }

  pub fn bulk(s: &str) -> Self {
    RespValue::BulkString(Some(s.to_string()))
  }

  pub fn integer(i: i64) -> Self {
    RespValue::Integer(i)
  }

  pub fn array(items: Vec<RespValue>) -> Self {
    RespValue::Array(Some(items))
  }

  /// Encode to RESP wire format
  pub fn encode(&self) -> Vec<u8> {
    let mut buf = Vec::new();
    self.write_to(&mut buf).unwrap();
    buf
  }

  fn write_to<W: Write>(&self, w: &mut W) -> io::Result<()> {
    match self {
      RespValue::SimpleString(s) => {
        write!(w, "+{}\r\n", s)?;
      }
      RespValue::Error(e) => {
        write!(w, "-{}\r\n", e)?;
      }
      RespValue::Integer(i) => {
        write!(w, ":{}\r\n", i)?;
      }
      RespValue::BulkString(None) => {
        write!(w, "$-1\r\n")?;
      }
      RespValue::BulkString(Some(s)) => {
        write!(w, "${}\r\n{}\r\n", s.len(), s)?;
      }
      RespValue::Array(None) => {
        write!(w, "*-1\r\n")?;
      }
      RespValue::Array(Some(items)) => {
        write!(w, "*{}\r\n", items.len())?;
        for item in items {
          item.write_to(w)?;
        }
      }
    }
    Ok(())
  }

  /// Extract string value
  pub fn as_str(&self) -> Option<&str> {
    match self {
      RespValue::SimpleString(s) | RespValue::BulkString(Some(s)) => Some(s),
      _ => None,
    }
  }

  /// Extract integer value
  pub fn as_i64(&self) -> Option<i64> {
    match self {
      RespValue::Integer(i) => Some(*i),
      RespValue::SimpleString(s) | RespValue::BulkString(Some(s)) => s.parse().ok(),
      _ => None,
    }
  }

  /// Extract array elements
  pub fn as_array(&self) -> Option<&[RespValue]> {
    match self {
      RespValue::Array(Some(arr)) => Some(arr),
      _ => None,
    }
  }
}

/// RESP parse error
#[derive(Debug, Clone)]
pub enum RespError {
  /// Incomplete data, need more bytes
  Incomplete,
  /// Invalid protocol format
  Invalid(String),
  /// IO error
  Io(String),
}

impl std::fmt::Display for RespError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      RespError::Incomplete => write!(f, "incomplete data"),
      RespError::Invalid(msg) => write!(f, "invalid RESP: {}", msg),
      RespError::Io(msg) => write!(f, "IO error: {}", msg),
    }
  }
}

impl std::error::Error for RespError {}

/// RESP protocol parser
pub struct RespParser {
  buffer: Vec<u8>,
  pos: usize,
}

impl Default for RespParser {
  fn default() -> Self {
    Self::new()
  }
}

impl RespParser {
  pub fn new() -> Self {
    Self {
      buffer: Vec::new(),
      pos: 0,
    }
  }

  /// Add data to the parse buffer
  pub fn feed(&mut self, data: &[u8]) {
    self.buffer.extend_from_slice(data);
  }

  /// Try to parse the next value from the buffer
  pub fn parse(&mut self) -> Result<Option<RespValue>, RespError> {
    if self.pos >= self.buffer.len() {
      return Ok(None);
    }

    let start_pos = self.pos;
    match self.parse_value() {
      Ok(value) => {
        // Successfully parsed, remove consumed bytes
        self.buffer.drain(..self.pos);
        self.pos = 0;
        Ok(Some(value))
      }
      Err(RespError::Incomplete) => {
        // Reset position, wait for more data
        self.pos = start_pos;
        Ok(None)
      }
      Err(e) => Err(e),
    }
  }

  /// Clear the buffer
  pub fn clear(&mut self) {
    self.buffer.clear();
    self.pos = 0;
  }

  fn parse_value(&mut self) -> Result<RespValue, RespError> {
    let byte = self.read_byte()?;

    match byte {
      b'+' => self.parse_simple_string(),
      b'-' => self.parse_error(),
      b':' => self.parse_integer(),
      b'$' => self.parse_bulk_string(),
      b'*' => self.parse_array(),
      _ => {
        // Inline command (no prefix) - treat as simple command
        self.pos -= 1; // Put back the byte
        self.parse_inline_command()
      }
    }
  }

  fn parse_simple_string(&mut self) -> Result<RespValue, RespError> {
    let line = self.read_line()?;
    Ok(RespValue::SimpleString(line))
  }

  fn parse_error(&mut self) -> Result<RespValue, RespError> {
    let line = self.read_line()?;
    Ok(RespValue::Error(line))
  }

  fn parse_integer(&mut self) -> Result<RespValue, RespError> {
    let line = self.read_line()?;
    let i = line
      .parse()
      .map_err(|_| RespError::Invalid(format!("invalid integer: {}", line)))?;
    Ok(RespValue::Integer(i))
  }

  fn parse_bulk_string(&mut self) -> Result<RespValue, RespError> {
    let len_str = self.read_line()?;
    let len: i64 = len_str
      .parse()
      .map_err(|_| RespError::Invalid(format!("invalid bulk string length: {}", len_str)))?;

    if len < 0 {
      return Ok(RespValue::BulkString(None));
    }

    let len = len as usize;
    if self.pos + len + 2 > self.buffer.len() {
      return Err(RespError::Incomplete);
    }

    let data = &self.buffer[self.pos..self.pos + len];
    let s = String::from_utf8_lossy(data).to_string();
    self.pos += len;

    // Skip \r\n
    if self.pos + 2 > self.buffer.len() {
      return Err(RespError::Incomplete);
    }
    if &self.buffer[self.pos..self.pos + 2] != b"\r\n" {
      return Err(RespError::Invalid("missing CRLF after bulk string".to_string()));
    }
    self.pos += 2;

    Ok(RespValue::BulkString(Some(s)))
  }

  fn parse_array(&mut self) -> Result<RespValue, RespError> {
    let len_str = self.read_line()?;
    let len: i64 = len_str
      .parse()
      .map_err(|_| RespError::Invalid(format!("invalid array length: {}", len_str)))?;

    if len < 0 {
      return Ok(RespValue::Array(None));
    }

    let len = len as usize;
    let mut items = Vec::with_capacity(len);

    for _ in 0..len {
      items.push(self.parse_value()?);
    }

    Ok(RespValue::Array(Some(items)))
  }

  fn parse_inline_command(&mut self) -> Result<RespValue, RespError> {
    let line = self.read_line()?;
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.is_empty() {
      return Err(RespError::Invalid("empty command".to_string()));
    }

    let items: Vec<RespValue> = parts
      .into_iter()
      .map(|s| RespValue::BulkString(Some(s.to_string())))
      .collect();

    Ok(RespValue::Array(Some(items)))
  }

  fn read_byte(&mut self) -> Result<u8, RespError> {
    if self.pos >= self.buffer.len() {
      return Err(RespError::Incomplete);
    }
    let byte = self.buffer[self.pos];
    self.pos += 1;
    Ok(byte)
  }

  fn read_line(&mut self) -> Result<String, RespError> {
    let start = self.pos;

    loop {
      if self.pos + 1 >= self.buffer.len() {
        return Err(RespError::Incomplete);
      }

      if self.buffer[self.pos] == b'\r' && self.buffer[self.pos + 1] == b'\n' {
        let line = &self.buffer[start..self.pos];
        let s = String::from_utf8_lossy(line).to_string();
        self.pos += 2; // Skip \r\n
        return Ok(s);
      }

      self.pos += 1;
    }
  }
}

/// Parse a single RESP value from bytes
pub fn parse_resp(data: &[u8]) -> Result<RespValue, RespError> {
  let mut parser = RespParser::new();
  parser.feed(data);
  parser.parse()?.ok_or(RespError::Incomplete)
}

/// Extract command name and arguments from a RESP array
pub fn extract_command(value: &RespValue) -> Option<(String, Vec<String>)> {
  let arr = value.as_array()?;
  if arr.is_empty() {
    return None;
  }

  let cmd = arr[0].as_str()?.to_uppercase();
  let args: Vec<String> = arr[1..]
    .iter()
    .filter_map(|v| v.as_str().map(String::from))
    .collect();

  Some((cmd, args))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_simple_string() {
    let data = b"+OK\r\n";
    let result = parse_resp(data).unwrap();
    assert_eq!(result, RespValue::SimpleString("OK".to_string()));
  }

  #[test]
  fn test_parse_error() {
    let data = b"-ERR unknown command\r\n";
    let result = parse_resp(data).unwrap();
    assert_eq!(result, RespValue::Error("ERR unknown command".to_string()));
  }

  #[test]
  fn test_parse_integer() {
    let data = b":42\r\n";
    let result = parse_resp(data).unwrap();
    assert_eq!(result, RespValue::Integer(42));
  }

  #[test]
  fn test_parse_bulk_string() {
    let data = b"$5\r\nhello\r\n";
    let result = parse_resp(data).unwrap();
    assert_eq!(result, RespValue::BulkString(Some("hello".to_string())));
  }

  #[test]
  fn test_parse_null_bulk() {
    let data = b"$-1\r\n";
    let result = parse_resp(data).unwrap();
    assert_eq!(result, RespValue::BulkString(None));
  }

  #[test]
  fn test_parse_array() {
    let data = b"*2\r\n$3\r\nGET\r\n$3\r\nfoo\r\n";
    let result = parse_resp(data).unwrap();
    assert_eq!(
      result,
      RespValue::Array(Some(vec![
        RespValue::BulkString(Some("GET".to_string())),
        RespValue::BulkString(Some("foo".to_string())),
      ]))
    );
  }

  #[test]
  fn test_encode_roundtrip() {
    let values = vec![
      RespValue::ok(),
      RespValue::error("ERR test"),
      RespValue::integer(123),
      RespValue::bulk("hello"),
      RespValue::null_bulk(),
      RespValue::array(vec![
        RespValue::bulk("SET"),
        RespValue::bulk("key"),
        RespValue::bulk("value"),
      ]),
    ];

    for original in values {
      let encoded = original.encode();
      let parsed = parse_resp(&encoded).unwrap();
      assert_eq!(original, parsed);
    }
  }

  #[test]
  fn test_extract_command() {
    let value = RespValue::Array(Some(vec![
      RespValue::BulkString(Some("set".to_string())),
      RespValue::BulkString(Some("key".to_string())),
      RespValue::BulkString(Some("value".to_string())),
    ]));

    let (cmd, args) = extract_command(&value).unwrap();
    assert_eq!(cmd, "SET");
    assert_eq!(args, vec!["key", "value"]);
  }

  #[test]
  fn test_inline_command() {
    let data = b"PING\r\n";
    let result = parse_resp(data).unwrap();
    let (cmd, args) = extract_command(&result).unwrap();
    assert_eq!(cmd, "PING");
    assert!(args.is_empty());
  }
}
