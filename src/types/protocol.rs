use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Document, StructuredQuery};

/// Query input - either a JS string (legacy) or a structured query object
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryInput {
  /// Structured query object sent from SDKs
  Structured(StructuredQuery),
  /// Legacy JS string query
  Script(String),
}

impl QueryInput {
  /// Check if this is a structured query
  pub fn is_structured(&self) -> bool {
    matches!(self, Self::Structured(_))
  }

  /// Get the table name from the query (if determinable)
  pub fn table(&self) -> Option<&str> {
    match self {
      Self::Structured(q) => Some(&q.table),
      Self::Script(_) => None,
    }
  }

  /// Check if the query (as a string representation) contains a pattern
  /// For Script queries, checks the script string
  /// For Structured queries, checks the table name
  pub fn contains(&self, pattern: &str) -> bool {
    match self {
      Self::Script(s) => s.contains(pattern),
      Self::Structured(q) => q.table.contains(pattern),
    }
  }

  /// Get the script string if this is a Script query
  pub fn as_script(&self) -> Option<&str> {
    match self {
      Self::Script(s) => Some(s),
      Self::Structured(_) => None,
    }
  }

  /// Get the structured query if this is a Structured query
  pub fn as_structured(&self) -> Option<&StructuredQuery> {
    match self {
      Self::Structured(q) => Some(q),
      Self::Script(_) => None,
    }
  }
}

impl From<String> for QueryInput {
  fn from(s: String) -> Self {
    Self::Script(s)
  }
}

impl From<&str> for QueryInput {
  fn from(s: &str) -> Self {
    Self::Script(s.to_string())
  }
}

impl From<StructuredQuery> for QueryInput {
  fn from(q: StructuredQuery) -> Self {
    Self::Structured(q)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMessage {
  Query {
    id: String,
    query: QueryInput,
  },
  Subscribe {
    id: String,
    query: QueryInput,
  },
  Unsubscribe {
    id: String,
  },
  Insert {
    id: String,
    collection: String,
    data: serde_json::Value,
  },
  Update {
    id: String,
    collection: String,
    document_id: Uuid,
    data: serde_json::Value,
  },
  Delete {
    id: String,
    collection: String,
    document_id: Uuid,
  },
  ListCollections {
    id: String,
  },
  Ping {
    id: String,
  },
}

impl ClientMessage {
  pub fn id(&self) -> &str {
    match self {
      Self::Query { id, .. }
      | Self::Subscribe { id, .. }
      | Self::Unsubscribe { id }
      | Self::Insert { id, .. }
      | Self::Update { id, .. }
      | Self::Delete { id, .. }
      | Self::ListCollections { id }
      | Self::Ping { id } => id,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerMessage {
  Result { id: String, data: serde_json::Value },
  Change { id: String, change: ChangeEvent },
  Subscribed { id: String },
  Unsubscribed { id: String },
  Error { id: String, error: String },
  Pong { id: String },
}

impl ServerMessage {
  pub fn result(id: impl Into<String>, data: serde_json::Value) -> Self {
    Self::Result {
      id: id.into(),
      data,
    }
  }
  pub fn error(id: impl Into<String>, error: impl Into<String>) -> Self {
    Self::Error {
      id: id.into(),
      error: error.into(),
    }
  }
  pub fn subscribed(id: impl Into<String>) -> Self {
    Self::Subscribed { id: id.into() }
  }
  pub fn change(id: impl Into<String>, change: ChangeEvent) -> Self {
    Self::Change {
      id: id.into(),
      change,
    }
  }
  pub fn pong(id: impl Into<String>) -> Self {
    Self::Pong { id: id.into() }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ChangeEvent {
  Initial {
    document: Document,
  },
  Insert {
    new: Document,
  },
  Update {
    old: serde_json::Value,
    new: Document,
  },
  Delete {
    old: Document,
  },
}
