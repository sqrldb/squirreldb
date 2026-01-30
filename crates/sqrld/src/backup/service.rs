//! Database backup service
//!
//! Automatically backs up the database at configurable intervals.
//! Stores backups to S3 Storage (if enabled) or local filesystem.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::db::DatabaseBackend;
use crate::features::{AppState, Feature};
use crate::server::{BackendType, ServerConfig};
use crate::storage::StorageBackend;

/// Information about a backup
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupInfo {
  pub id: String,
  pub filename: String,
  pub size: i64,
  pub created_at: DateTime<Utc>,
  pub backend: String,
  pub location: String,
}

/// Backup feature for automatic database backups
pub struct BackupFeature {
  running: AtomicBool,
  shutdown_tx: RwLock<Option<mpsc::Sender<()>>>,
  last_backup: RwLock<Option<DateTime<Utc>>>,
  next_backup: RwLock<Option<DateTime<Utc>>>,
  storage_backend: RwLock<Option<Arc<dyn StorageBackend>>>,
}

impl Default for BackupFeature {
  fn default() -> Self {
    Self::new()
  }
}

impl BackupFeature {
  pub fn new() -> Self {
    Self {
      running: AtomicBool::new(false),
      shutdown_tx: RwLock::new(None),
      last_backup: RwLock::new(None),
      next_backup: RwLock::new(None),
      storage_backend: RwLock::new(None),
    }
  }

  /// Set the storage backend for storing backups to S3
  pub fn set_storage_backend(&self, backend: Arc<dyn StorageBackend>) {
    if let Ok(mut guard) = self.storage_backend.try_write() {
      *guard = Some(backend);
    }
  }

  /// Get the last backup time
  pub fn last_backup(&self) -> Option<DateTime<Utc>> {
    self.last_backup.try_read().ok().and_then(|g| *g)
  }

  /// Get the next scheduled backup time
  pub fn next_backup(&self) -> Option<DateTime<Utc>> {
    self.next_backup.try_read().ok().and_then(|g| *g)
  }

  /// Check if storage backend is available
  pub fn has_storage(&self) -> bool {
    self.storage_backend.try_read().ok().map_or(false, |g| g.is_some())
  }

  /// Create a backup now
  pub async fn create_backup(
    &self,
    backend: &Arc<dyn DatabaseBackend>,
    config: &ServerConfig,
  ) -> Result<BackupInfo, anyhow::Error> {
    let timestamp = Utc::now();
    let backup_id = Uuid::new_v4().to_string();
    let filename = format!(
      "squirreldb_backup_{}_{}.sql",
      timestamp.format("%Y%m%d_%H%M%S"),
      &backup_id[..8]
    );

    // Generate backup data
    let backup_data = generate_backup_sql(backend, config).await?;
    let size = backup_data.len() as i64;

    // Get storage backend if available
    let storage = {
      let guard = self.storage_backend.read().await;
      guard.clone()
    };

    // Determine storage location
    let location = if let Some(ref storage_backend) = storage {
      // Store to S3 storage in /backups folder
      let key = format!("{}/{}", config.backup.storage_path, filename);

      // Ensure backups bucket exists
      if let Err(e) = storage_backend.init_bucket("backups").await {
        tracing::warn!("Could not create backups bucket (may already exist): {}", e);
      }

      storage_backend
        .write_object("backups", &key, Uuid::new_v4(), backup_data.as_bytes())
        .await?;

      format!("s3://backups/{}", key)
    } else {
      // Store to local filesystem
      let local_path = PathBuf::from(&config.backup.local_path);
      tokio::fs::create_dir_all(&local_path).await?;

      let file_path = local_path.join(&filename);
      tokio::fs::write(&file_path, backup_data.as_bytes()).await?;

      file_path.to_string_lossy().to_string()
    };

    // Update last backup time
    {
      let mut guard = self.last_backup.write().await;
      *guard = Some(timestamp);
    }

    // Schedule next backup
    {
      let next = timestamp + chrono::Duration::seconds(config.backup.interval as i64);
      let mut guard = self.next_backup.write().await;
      *guard = Some(next);
    }

    // Clean up old backups
    self.cleanup_old_backups(config).await?;

    let info = BackupInfo {
      id: backup_id,
      filename,
      size,
      created_at: timestamp,
      backend: match config.backend {
        BackendType::Postgres => "postgres".to_string(),
        BackendType::Sqlite => "sqlite".to_string(),
      },
      location,
    };

    tracing::info!("Backup created: {} ({} bytes)", info.filename, info.size);

    Ok(info)
  }

  /// Clean up old backups based on retention policy
  async fn cleanup_old_backups(&self, config: &ServerConfig) -> Result<(), anyhow::Error> {
    // Get storage backend if available
    let storage = {
      let guard = self.storage_backend.read().await;
      guard.clone()
    };

    if storage.is_some() {
      // Storage mode cleanup is handled via list_backups and manual deletion
      // Future enhancement: implement storage listing in StorageBackend trait
      tracing::debug!("Storage mode backup cleanup deferred");
    } else {
      // Clean up local backups
      let local_path = PathBuf::from(&config.backup.local_path);

      if local_path.exists() {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&local_path).await?;
        while let Some(entry) = dir.next_entry().await? {
          entries.push(entry);
        }

        // Sort by name (contains timestamp) descending
        entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        // Delete backups beyond retention limit
        for entry in entries.iter().skip(config.backup.retention as usize) {
          let path = entry.path();
          if path.extension().map_or(false, |ext| ext == "sql") {
            if let Err(e) = tokio::fs::remove_file(&path).await {
              tracing::warn!("Failed to delete old backup {:?}: {}", path, e);
            } else {
              tracing::info!("Deleted old backup: {:?}", path);
            }
          }
        }
      }
    }

    Ok(())
  }

  /// List all backups
  pub async fn list_backups(&self, config: &ServerConfig) -> Result<Vec<BackupInfo>, anyhow::Error> {
    let mut backups = Vec::new();

    // Get storage backend if available
    let storage = {
      let guard = self.storage_backend.read().await;
      guard.clone()
    };

    if storage.is_some() {
      // For storage mode, listing requires StorageBackend list_objects support
      // For now, return an empty list with a note
      tracing::debug!("Storage mode backup listing not yet implemented");
      // Future enhancement: add list_objects to StorageBackend trait
    } else {
      // List local backups
      let local_path = PathBuf::from(&config.backup.local_path);

      if local_path.exists() {
        let mut entries = tokio::fs::read_dir(&local_path).await?;

        while let Some(entry) = entries.next_entry().await? {
          let path = entry.path();
          if path.extension().map_or(false, |ext| ext == "sql") {
            let metadata = entry.metadata().await?;
            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();

            // Parse timestamp from filename
            let created_at = parse_backup_timestamp(&filename);

            backups.push(BackupInfo {
              id: filename.split('_').last().unwrap_or("unknown").replace(".sql", ""),
              filename: filename.clone(),
              size: metadata.len() as i64,
              created_at,
              backend: "local".to_string(),
              location: path.to_string_lossy().to_string(),
            });
          }
        }
      }
    }

    // Sort by created_at descending
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(backups)
  }

  /// Delete a specific backup
  pub async fn delete_backup(&self, config: &ServerConfig, backup_id: &str) -> Result<bool, anyhow::Error> {
    // Get storage backend if available
    let storage = {
      let guard = self.storage_backend.read().await;
      guard.clone()
    };

    if storage.is_some() {
      // For storage mode, we need to know the full path
      // This is a limitation without list_objects support
      tracing::warn!("Storage mode backup deletion not fully implemented");
      return Ok(false);
    }

    // For local backups, search the directory
    let local_path = PathBuf::from(&config.backup.local_path);

    if local_path.exists() {
      let mut entries = tokio::fs::read_dir(&local_path).await?;

      while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();

        // Check if this backup matches the ID
        if filename.contains(backup_id) && filename.ends_with(".sql") {
          tokio::fs::remove_file(&path).await?;
          tracing::info!("Deleted local backup: {}", filename);
          return Ok(true);
        }
      }
    }

    Ok(false)
  }
}

#[async_trait]
impl Feature for BackupFeature {
  fn name(&self) -> &str {
    "backup"
  }

  fn description(&self) -> &str {
    "Automatic database backups"
  }

  async fn start(&self, state: Arc<AppState>) -> Result<(), anyhow::Error> {
    if self.running.load(Ordering::SeqCst) {
      return Ok(());
    }

    self.running.store(true, Ordering::SeqCst);

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    {
      let mut guard = self.shutdown_tx.write().await;
      *guard = Some(shutdown_tx);
    }

    // Set initial next backup time
    let interval = state.config.backup.interval;
    {
      let next = Utc::now() + chrono::Duration::seconds(interval as i64);
      let mut guard = self.next_backup.write().await;
      *guard = Some(next);
    }

    let backend = state.backend.clone();
    let config = state.config.clone();

    // Get storage backend for the spawned task
    let storage = {
      let guard = self.storage_backend.read().await;
      guard.clone()
    };

    // Spawn backup task
    tokio::spawn(async move {
      tracing::info!(
        "Backup service started (interval: {}s, retention: {})",
        config.backup.interval,
        config.backup.retention
      );

      loop {
        tokio::select! {
          _ = tokio::time::sleep(tokio::time::Duration::from_secs(config.backup.interval)) => {
            // Perform backup
            let timestamp = Utc::now();
            let backup_id = Uuid::new_v4().to_string();
            let filename = format!(
              "squirreldb_backup_{}_{}.sql",
              timestamp.format("%Y%m%d_%H%M%S"),
              &backup_id[..8]
            );

            tracing::info!("Starting scheduled backup: {}", filename);

            // Generate backup data
            match generate_backup_sql(&backend, &config).await {
              Ok(backup_data) => {
                let result: Result<(), anyhow::Error> = if let Some(ref sb) = storage {
                  let key = format!("{}/{}", config.backup.storage_path, filename);
                  sb.write_object("backups", &key, Uuid::new_v4(), backup_data.as_bytes())
                    .await
                    .map(|_| ())
                    .map_err(|e| anyhow::anyhow!("Storage error: {}", e))
                } else {
                  let local_path = PathBuf::from(&config.backup.local_path);
                  let _ = tokio::fs::create_dir_all(&local_path).await;
                  let file_path = local_path.join(&filename);
                  tokio::fs::write(&file_path, backup_data.as_bytes())
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to write backup: {}", e))
                };

                match result {
                  Ok(_) => {
                    tracing::info!("Scheduled backup completed: {}", filename);
                  }
                  Err(e) => {
                    tracing::error!("Scheduled backup failed: {}", e);
                  }
                }
              }
              Err(e) => {
                tracing::error!("Failed to generate backup data: {}", e);
              }
            }
          }
          _ = shutdown_rx.recv() => {
            tracing::info!("Backup service shutting down");
            break;
          }
        }
      }
    });

    tracing::info!("Backup feature started");
    Ok(())
  }

  async fn stop(&self) -> Result<(), anyhow::Error> {
    self.running.store(false, Ordering::SeqCst);

    // Take the sender out of the lock before awaiting
    let tx = {
      let mut guard = self.shutdown_tx.write().await;
      guard.take()
    };

    if let Some(tx) = tx {
      let _ = tx.send(()).await;
    }

    tracing::info!("Backup feature stopped");
    Ok(())
  }

  fn is_running(&self) -> bool {
    self.running.load(Ordering::SeqCst)
  }

  fn as_any(&self) -> &dyn std::any::Any {
    self
  }
}

/// Parse backup timestamp from filename
fn parse_backup_timestamp(filename: &str) -> DateTime<Utc> {
  // Format: squirreldb_backup_YYYYMMDD_HHMMSS_XXXXXXXX.sql
  if let Some(rest) = filename.strip_prefix("squirreldb_backup_") {
    let parts: Vec<&str> = rest.split('_').collect();
    if parts.len() >= 2 {
      if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(
        &format!("{}_{}", parts[0], parts[1]),
        "%Y%m%d_%H%M%S"
      ) {
        return DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
      }
    }
  }
  Utc::now()
}

/// Helper function to generate backup data
async fn generate_backup_sql(
  backend: &Arc<dyn DatabaseBackend>,
  config: &ServerConfig,
) -> Result<String, anyhow::Error> {
  let mut sql = String::new();

  sql.push_str("-- SquirrelDB Backup\n");
  sql.push_str(&format!("-- Created: {}\n", Utc::now().to_rfc3339()));
  sql.push_str(&format!("-- Backend: {:?}\n", config.backend));
  sql.push_str("-- \n\n");

  let projects = backend.list_projects().await?;

  sql.push_str("-- Projects\n");
  for project in &projects {
    sql.push_str(&format!(
      "-- Project: {} ({})\n",
      project.name, project.id
    ));

    let collections = backend.list_collections(project.id).await?;

    for collection in &collections {
      sql.push_str(&format!("\n-- Collection: {}.{}\n", project.name, collection));

      let docs = backend.list(project.id, collection, None, None, None, None).await?;

      for doc in docs {
        let data_json = serde_json::to_string(&doc.data)?;
        sql.push_str(&format!(
          "INSERT INTO {} (id, data, created_at, updated_at) VALUES ('{}', '{}', '{}', '{}');\n",
          collection,
          doc.id,
          data_json.replace('\'', "''"),
          doc.created_at.to_rfc3339(),
          doc.updated_at.to_rfc3339()
        ));
      }
    }
  }

  Ok(sql)
}
