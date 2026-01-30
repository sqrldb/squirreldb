use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ChangeOperation {
  Insert,
  Update,
  Delete,
}

impl std::str::FromStr for ChangeOperation {
  type Err = String;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_uppercase().as_str() {
      "INSERT" => Ok(Self::Insert),
      "UPDATE" => Ok(Self::Update),
      "DELETE" => Ok(Self::Delete),
      _ => Err(format!("Unknown operation: {}", s)),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
  pub id: i64,
  pub project_id: Uuid,
  pub collection: String,
  pub document_id: Uuid,
  pub operation: ChangeOperation,
  pub old_data: Option<serde_json::Value>,
  pub new_data: Option<serde_json::Value>,
  pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeNotification {
  pub project_id: Option<Uuid>,
  pub collection: String,
  pub id: Uuid,
  pub op: String,
}
