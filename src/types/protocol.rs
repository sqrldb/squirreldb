use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Document;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMessage {
  Query {
    id: String,
    query: String,
  },
  Subscribe {
    id: String,
    query: String,
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
