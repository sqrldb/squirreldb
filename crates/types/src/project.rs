use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectRole {
  Owner,
  Admin,
  #[default]
  Member,
  Viewer,
}

impl std::fmt::Display for ProjectRole {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Owner => write!(f, "owner"),
      Self::Admin => write!(f, "admin"),
      Self::Member => write!(f, "member"),
      Self::Viewer => write!(f, "viewer"),
    }
  }
}

impl std::str::FromStr for ProjectRole {
  type Err = String;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "owner" => Ok(Self::Owner),
      "admin" => Ok(Self::Admin),
      "member" => Ok(Self::Member),
      "viewer" => Ok(Self::Viewer),
      _ => Err(format!("Unknown role: {}", s)),
    }
  }
}

impl ProjectRole {
  pub fn can_write(&self) -> bool {
    matches!(self, Self::Owner | Self::Admin | Self::Member)
  }

  pub fn can_read(&self) -> bool {
    true
  }

  pub fn can_manage_members(&self) -> bool {
    matches!(self, Self::Owner | Self::Admin)
  }

  pub fn can_delete_project(&self) -> bool {
    matches!(self, Self::Owner)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
  pub id: Uuid,
  pub name: String,
  pub description: Option<String>,
  pub owner_id: Uuid,
  pub created_at: DateTime<Utc>,
  pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMember {
  pub id: Uuid,
  pub project_id: Uuid,
  pub user_id: Uuid,
  pub role: ProjectRole,
  pub created_at: DateTime<Utc>,
}

pub const DEFAULT_PROJECT_ID: Uuid = Uuid::from_u128(0);
