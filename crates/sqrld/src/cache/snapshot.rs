//! Cache snapshot persistence

use std::path::Path;
use std::sync::Arc;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::entry::SnapshotEntry;
use super::store::InMemoryCacheStore;

/// Snapshot file header
const SNAPSHOT_MAGIC: &[u8] = b"SQRLCACHE";
const SNAPSHOT_VERSION: u8 = 1;

/// Snapshot persistence manager
pub struct SnapshotManager {
  path: String,
}

impl SnapshotManager {
  pub fn new(path: &str) -> Self {
    Self {
      path: path.to_string(),
    }
  }

  /// Save cache state to snapshot file
  pub async fn save(&self, store: &InMemoryCacheStore) -> Result<usize, SnapshotError> {
    let entries = store.snapshot_entries();
    let count = entries.len();

    if count == 0 {
      // Don't save empty snapshots
      return Ok(0);
    }

    // Ensure parent directory exists
    if let Some(parent) = Path::new(&self.path).parent() {
      fs::create_dir_all(parent).await.map_err(SnapshotError::Io)?;
    }

    // Write to temp file first
    let temp_path = format!("{}.tmp", self.path);
    let mut file = File::create(&temp_path).await.map_err(SnapshotError::Io)?;

    // Write header
    file.write_all(SNAPSHOT_MAGIC).await.map_err(SnapshotError::Io)?;
    file.write_all(&[SNAPSHOT_VERSION]).await.map_err(SnapshotError::Io)?;

    // Write entry count
    let count_bytes = (entries.len() as u64).to_le_bytes();
    file.write_all(&count_bytes).await.map_err(SnapshotError::Io)?;

    // Serialize entries as JSON (simple, human-readable)
    let json = serde_json::to_vec(&entries).map_err(SnapshotError::Serialize)?;
    let json_len = (json.len() as u64).to_le_bytes();
    file.write_all(&json_len).await.map_err(SnapshotError::Io)?;
    file.write_all(&json).await.map_err(SnapshotError::Io)?;

    // Sync and close
    file.sync_all().await.map_err(SnapshotError::Io)?;
    drop(file);

    // Atomic rename
    fs::rename(&temp_path, &self.path).await.map_err(SnapshotError::Io)?;

    tracing::info!("Cache snapshot saved: {} entries to {}", count, self.path);
    Ok(count)
  }

  /// Load cache state from snapshot file
  pub async fn load(&self, store: &InMemoryCacheStore) -> Result<usize, SnapshotError> {
    if !Path::new(&self.path).exists() {
      return Ok(0);
    }

    let mut file = File::open(&self.path).await.map_err(SnapshotError::Io)?;

    // Read and verify header
    let mut magic = [0u8; 9];
    file.read_exact(&mut magic).await.map_err(SnapshotError::Io)?;
    if magic != SNAPSHOT_MAGIC {
      return Err(SnapshotError::InvalidFormat("invalid magic header".to_string()));
    }

    let mut version = [0u8; 1];
    file.read_exact(&mut version).await.map_err(SnapshotError::Io)?;
    if version[0] != SNAPSHOT_VERSION {
      return Err(SnapshotError::InvalidFormat(format!(
        "unsupported version: {}",
        version[0]
      )));
    }

    // Read entry count
    let mut count_bytes = [0u8; 8];
    file.read_exact(&mut count_bytes).await.map_err(SnapshotError::Io)?;
    let _expected_count = u64::from_le_bytes(count_bytes);

    // Read JSON length and data
    let mut json_len_bytes = [0u8; 8];
    file.read_exact(&mut json_len_bytes).await.map_err(SnapshotError::Io)?;
    let json_len = u64::from_le_bytes(json_len_bytes) as usize;

    let mut json_data = vec![0u8; json_len];
    file.read_exact(&mut json_data).await.map_err(SnapshotError::Io)?;

    // Deserialize entries
    let entries: Vec<SnapshotEntry> =
      serde_json::from_slice(&json_data).map_err(SnapshotError::Deserialize)?;

    let count = entries.len();
    store.restore_from_snapshot(entries);

    tracing::info!("Cache snapshot loaded: {} entries from {}", count, self.path);
    Ok(count)
  }

  /// Delete snapshot file
  pub async fn delete(&self) -> Result<(), SnapshotError> {
    if Path::new(&self.path).exists() {
      fs::remove_file(&self.path).await.map_err(SnapshotError::Io)?;
    }
    Ok(())
  }

  /// Get snapshot file size
  pub async fn size(&self) -> Option<u64> {
    fs::metadata(&self.path).await.ok().map(|m| m.len())
  }
}

/// Snapshot errors
#[derive(Debug)]
pub enum SnapshotError {
  Io(std::io::Error),
  Serialize(serde_json::Error),
  Deserialize(serde_json::Error),
  InvalidFormat(String),
}

impl std::fmt::Display for SnapshotError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      SnapshotError::Io(e) => write!(f, "IO error: {}", e),
      SnapshotError::Serialize(e) => write!(f, "Serialization error: {}", e),
      SnapshotError::Deserialize(e) => write!(f, "Deserialization error: {}", e),
      SnapshotError::InvalidFormat(msg) => write!(f, "Invalid snapshot format: {}", msg),
    }
  }
}

impl std::error::Error for SnapshotError {}

/// Periodic snapshot task
pub async fn run_snapshot_task(
  store: Arc<InMemoryCacheStore>,
  path: String,
  interval_secs: u64,
) {
  let manager = SnapshotManager::new(&path);

  loop {
    tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;

    match manager.save(&store).await {
      Ok(count) => {
        if count > 0 {
          tracing::debug!("Periodic snapshot saved: {} entries", count);
        }
      }
      Err(e) => {
        tracing::error!("Failed to save snapshot: {}", e);
      }
    }
  }
}

/// TTL expiration task
pub async fn run_expiration_task(store: Arc<InMemoryCacheStore>, interval_secs: u64) {
  loop {
    tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
    let expired = store.evict_expired();
    if expired > 0 {
      tracing::debug!("Evicted {} expired keys", expired);
    }
  }
}
