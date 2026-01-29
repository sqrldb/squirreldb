use async_trait::async_trait;
use chrono::Utc;
use rusqlite::params;
use tokio::sync::broadcast;
use tokio_rusqlite::Connection;
use uuid::Uuid;

use super::backend::{
  AdminRole, AdminSession, AdminUser, ApiTokenInfo, DatabaseBackend, SqlDialect,
  StorageAccessKeyInfo,
};
use super::sanitize::{validate_collection_name, validate_identifier, validate_limit};
use crate::storage::{MultipartPart, MultipartUpload, ObjectAcl, StorageBucket, StorageObject};
use crate::types::{Change, ChangeOperation, Document, OrderBySpec, OrderDirection};

const PRAGMAS: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -64000;
PRAGMA temp_store = MEMORY;
PRAGMA mmap_size = 268435456;
PRAGMA page_size = 4096;
"#;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    collection TEXT NOT NULL,
    data TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection);

CREATE TABLE IF NOT EXISTS change_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    collection TEXT NOT NULL,
    document_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    old_data TEXT,
    new_data TEXT,
    changed_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_change_queue_id ON change_queue(id);
CREATE INDEX IF NOT EXISTS idx_change_queue_collection ON change_queue(collection);

CREATE TRIGGER IF NOT EXISTS documents_insert AFTER INSERT ON documents BEGIN
    INSERT INTO change_queue (collection, document_id, operation, new_data, changed_at)
    VALUES (NEW.collection, NEW.id, 'INSERT', NEW.data, datetime('now'));
END;

CREATE TRIGGER IF NOT EXISTS documents_update AFTER UPDATE ON documents BEGIN
    INSERT INTO change_queue (collection, document_id, operation, old_data, new_data, changed_at)
    VALUES (NEW.collection, NEW.id, 'UPDATE', OLD.data, NEW.data, datetime('now'));
END;

CREATE TRIGGER IF NOT EXISTS documents_delete AFTER DELETE ON documents BEGIN
    INSERT INTO change_queue (collection, document_id, operation, old_data, changed_at)
    VALUES (OLD.collection, OLD.id, 'DELETE', OLD.data, datetime('now'));
END;

CREATE TABLE IF NOT EXISTS api_tokens (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    token_hash TEXT NOT NULL,
    created_at TEXT NOT NULL
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash);
"#;

pub struct SqliteBackend {
  conn: Connection,
  change_tx: broadcast::Sender<Change>,
}

impl SqliteBackend {
  pub async fn new(path: &str) -> Result<Self, anyhow::Error> {
    let conn = if path == ":memory:" {
      Connection::open_in_memory().await?
    } else {
      Connection::open(path).await?
    };

    // Apply performance pragmas
    conn
      .call(|conn| conn.execute_batch(PRAGMAS).map_err(|e| e.into()))
      .await?;

    let (change_tx, _) = broadcast::channel(4096);
    Ok(Self { conn, change_tx })
  }

  pub async fn in_memory() -> Result<Self, anyhow::Error> {
    Self::new(":memory:").await
  }
}

#[async_trait]
impl DatabaseBackend for SqliteBackend {
  fn dialect(&self) -> SqlDialect {
    SqlDialect::Sqlite
  }

  async fn init_schema(&self) -> Result<(), anyhow::Error> {
    self
      .conn
      .call(|conn| conn.execute_batch(SCHEMA).map_err(|e| e.into()))
      .await?;
    tracing::info!("SQLite schema initialized");
    Ok(())
  }

  async fn drop_schema(&self) -> Result<(), anyhow::Error> {
    self
      .conn
      .call(|conn| {
        conn
          .execute_batch(
            "DROP TRIGGER IF EXISTS documents_insert;
         DROP TRIGGER IF EXISTS documents_update;
         DROP TRIGGER IF EXISTS documents_delete;
         DROP TABLE IF EXISTS change_queue;
         DROP TABLE IF EXISTS documents;",
          )
          .map_err(|e| e.into())
      })
      .await?;
    Ok(())
  }

  async fn insert(
    &self,
    collection: &str,
    data: serde_json::Value,
  ) -> Result<Document, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    let id = Uuid::new_v4();
    let now = Utc::now();
    let data_str = serde_json::to_string(&data)?;
    let now_str = now.to_rfc3339();
    let col = collection.to_string();
    let id_str = id.to_string();

    self.conn.call(move |conn| {
      conn.execute(
        "INSERT INTO documents (id, collection, data, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id_str, col, data_str, now_str, now_str],
      ).map_err(|e| e.into())
    }).await?;

    Ok(Document {
      id,
      collection: collection.into(),
      data,
      created_at: now,
      updated_at: now,
    })
  }

  async fn get(&self, collection: &str, id: Uuid) -> Result<Option<Document>, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    let col = collection.to_string();
    let id_str = id.to_string();

    self.conn.call(move |conn| {
      let mut stmt = conn.prepare_cached("SELECT id, collection, data, created_at, updated_at FROM documents WHERE collection = ?1 AND id = ?2")?;
      let mut rows = stmt.query(params![col, id_str])?;
      if let Some(row) = rows.next()? {
        Ok(Some(row_to_doc(row)?))
      } else {
        Ok(None)
      }
    }).await.map_err(|e| anyhow::anyhow!("{}", e))
  }

  async fn update(
    &self,
    collection: &str,
    id: Uuid,
    data: serde_json::Value,
  ) -> Result<Option<Document>, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    let col = collection.to_string();
    let id_str = id.to_string();
    let data_str = serde_json::to_string(&data)?;
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    self
      .conn
      .call(move |conn| {
        let changed = conn.execute(
          "UPDATE documents SET data = ?1, updated_at = ?2 WHERE collection = ?3 AND id = ?4",
          params![data_str, now_str, col, id_str],
        )?;
        if changed == 0 {
          return Ok(None);
        }

        let mut stmt = conn.prepare_cached(
          "SELECT id, collection, data, created_at, updated_at FROM documents WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id_str])?;
        if let Some(row) = rows.next()? {
          Ok(Some(row_to_doc(row)?))
        } else {
          Ok(None)
        }
      })
      .await
      .map_err(|e| anyhow::anyhow!("{}", e))
  }

  async fn delete(&self, collection: &str, id: Uuid) -> Result<Option<Document>, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    let col = collection.to_string();
    let id_str = id.to_string();

    self.conn.call(move |conn| {
      let mut stmt = conn.prepare_cached("SELECT id, collection, data, created_at, updated_at FROM documents WHERE collection = ?1 AND id = ?2")?;
      let mut rows = stmt.query(params![col.clone(), id_str.clone()])?;
      let doc = if let Some(row) = rows.next()? { Some(row_to_doc(row)?) } else { return Ok(None) };
      drop(rows);
      drop(stmt);
      conn.execute("DELETE FROM documents WHERE collection = ?1 AND id = ?2", params![col, id_str])?;
      Ok(doc)
    }).await.map_err(|e| anyhow::anyhow!("{}", e))
  }

  async fn list(
    &self,
    collection: &str,
    filter: Option<&str>,
    order: Option<&OrderBySpec>,
    limit: Option<usize>,
    offset: Option<usize>,
  ) -> Result<Vec<Document>, anyhow::Error> {
    // Validate collection name
    validate_collection_name(collection)?;

    // Validate order field if present
    if let Some(o) = order {
      validate_identifier(&o.field)?;
    }

    // Validate limit if present
    if let Some(l) = limit {
      validate_limit(l)?;
    }

    // Validate offset if present
    if let Some(o) = offset {
      if o > 1_000_000 {
        anyhow::bail!("Offset too large (max 1000000)");
      }
    }

    let col = collection.to_string();
    let mut sql = String::with_capacity(256);
    sql.push_str(
      "SELECT id, collection, data, created_at, updated_at FROM documents WHERE collection = ?1",
    );

    // Filter is pre-validated by query compiler
    if let Some(f) = filter {
      sql.push_str(" AND ");
      sql.push_str(f);
    }

    if let Some(o) = order {
      let dir = if o.direction == OrderDirection::Desc {
        "DESC"
      } else {
        "ASC"
      };
      sql.push_str(" ORDER BY json_extract(data, '$.");
      sql.push_str(&o.field);
      sql.push_str("') ");
      sql.push_str(dir);
    }

    if let Some(l) = limit {
      sql.push_str(&format!(" LIMIT {}", l));
    }

    if let Some(o) = offset {
      sql.push_str(&format!(" OFFSET {}", o));
    }

    self
      .conn
      .call(move |conn| {
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params![col])?;
        let mut docs = Vec::with_capacity(limit.unwrap_or(100));
        while let Some(row) = rows.next()? {
          docs.push(row_to_doc(row)?);
        }
        Ok(docs)
      })
      .await
      .map_err(|e| anyhow::anyhow!("{}", e))
  }

  async fn list_collections(&self) -> Result<Vec<String>, anyhow::Error> {
    self
      .conn
      .call(|conn| {
        let mut stmt =
          conn.prepare_cached("SELECT DISTINCT collection FROM documents ORDER BY collection")?;
        let mut rows = stmt.query([])?;
        let mut cols = Vec::new();
        while let Some(row) = rows.next()? {
          cols.push(row.get(0)?);
        }
        Ok(cols)
      })
      .await
      .map_err(|e| anyhow::anyhow!("{}", e))
  }

  fn subscribe_changes(&self) -> broadcast::Receiver<Change> {
    self.change_tx.subscribe()
  }

  async fn start_change_listener(&self) -> Result<(), anyhow::Error> {
    let tx = self.change_tx.clone();
    let conn = self.conn.clone();
    tracing::info!("SQLite change listener started");

    tokio::spawn(async move {
      let mut last_id: i64 = 0;
      loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let lid = last_id;
        let changes: Result<Vec<Change>, _> = conn.call(move |conn| {
          let mut stmt = conn.prepare_cached(
            "SELECT id, collection, document_id, operation, old_data, new_data, changed_at FROM change_queue WHERE id > ?1 ORDER BY id LIMIT 100"
          )?;
          let mut rows = stmt.query(params![lid])?;
          let mut changes = Vec::with_capacity(100);
          while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let op_str: String = row.get(3)?;
            let Ok(op) = op_str.parse::<ChangeOperation>() else { continue };
            let old_data: Option<String> = row.get(4)?;
            let new_data: Option<String> = row.get(5)?;
            let changed_at_str: String = row.get(6)?;
            changes.push(Change {
              id,
              collection: row.get(1)?,
              document_id: row.get::<_, String>(2)?.parse().unwrap_or_default(),
              operation: op,
              old_data: old_data.and_then(|s| serde_json::from_str(&s).ok()),
              new_data: new_data.and_then(|s| serde_json::from_str(&s).ok()),
              changed_at: chrono::DateTime::parse_from_rfc3339(&changed_at_str).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
            });
          }
          Ok(changes)
        }).await;

        if let Ok(changes) = changes {
          for change in changes {
            last_id = change.id;
            let _ = tx.send(change);
          }
        }
      }
    });

    // Spawn cleanup task to prevent unbounded growth of change_queue
    let cleanup_conn = self.conn.clone();
    tokio::spawn(async move {
      loop {
        // Run cleanup every 5 minutes
        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
        // Keep only the last 10000 entries (or entries from the last hour)
        let result: Result<usize, _> = cleanup_conn.call(|conn| {
          conn.execute(
            "DELETE FROM change_queue WHERE id < (SELECT MAX(id) - 10000 FROM change_queue) AND changed_at < datetime('now', '-1 hour')",
            []
          ).map_err(|e| e.into())
        }).await;
        if let Ok(count) = result {
          if count > 0 {
            tracing::debug!("Cleaned up {} old change_queue entries", count);
          }
        }
      }
    });

    Ok(())
  }

  async fn create_token(
    &self,
    name: &str,
    token_hash: &str,
  ) -> Result<ApiTokenInfo, anyhow::Error> {
    let id = Uuid::new_v4();
    let now = Utc::now();
    let id_str = id.to_string();
    let now_str = now.to_rfc3339();
    let name_owned = name.to_string();
    let hash_owned = token_hash.to_string();

    self
      .conn
      .call(move |conn| {
        conn
          .execute(
            "INSERT INTO api_tokens (id, name, token_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id_str, name_owned, hash_owned, now_str],
          )
          .map_err(|e| e.into())
      })
      .await?;

    Ok(ApiTokenInfo {
      id,
      name: name.into(),
      created_at: now,
    })
  }

  async fn delete_token(&self, id: Uuid) -> Result<bool, anyhow::Error> {
    let id_str = id.to_string();
    let result: usize = self
      .conn
      .call(move |conn| {
        conn
          .execute("DELETE FROM api_tokens WHERE id = ?1", params![id_str])
          .map_err(|e| e.into())
      })
      .await?;
    Ok(result > 0)
  }

  async fn list_tokens(&self) -> Result<Vec<ApiTokenInfo>, anyhow::Error> {
    self
      .conn
      .call(|conn| {
        let mut stmt = conn
          .prepare_cached("SELECT id, name, created_at FROM api_tokens ORDER BY created_at DESC")?;
        let mut rows = stmt.query([])?;
        let mut tokens = Vec::new();
        while let Some(row) = rows.next()? {
          let id_str: String = row.get(0)?;
          let created_str: String = row.get(2)?;
          tokens.push(ApiTokenInfo {
            id: id_str.parse().unwrap_or_default(),
            name: row.get(1)?,
            created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
              .map(|d| d.with_timezone(&Utc))
              .unwrap_or_else(|_| Utc::now()),
          });
        }
        Ok(tokens)
      })
      .await
      .map_err(|e| anyhow::anyhow!("{}", e))
  }

  async fn validate_token(&self, token_hash: &str) -> Result<bool, anyhow::Error> {
    let hash_owned = token_hash.to_string();
    self
      .conn
      .call(move |conn| {
        let mut stmt = conn.prepare_cached("SELECT 1 FROM api_tokens WHERE token_hash = ?1")?;
        let exists = stmt.exists(params![hash_owned])?;
        Ok(exists)
      })
      .await
      .map_err(|e| anyhow::anyhow!("{}", e))
  }

  // Subscription filter methods - SQLite uses in-memory filtering (stubs for trait compatibility)
  async fn add_subscription_filter(
    &self,
    _client_id: Uuid,
    _subscription_id: &str,
    _collection: &str,
    _compiled_sql: Option<&str>,
  ) -> Result<(), anyhow::Error> {
    // SQLite uses in-memory subscription management, no DB-side filtering
    Ok(())
  }

  async fn remove_subscription_filter(
    &self,
    _client_id: Uuid,
    _subscription_id: &str,
  ) -> Result<(), anyhow::Error> {
    Ok(())
  }

  async fn remove_client_filters(&self, _client_id: Uuid) -> Result<u64, anyhow::Error> {
    Ok(0)
  }

  // Rate limiting methods - SQLite uses in-memory rate limiting (stubs for trait compatibility)
  async fn rate_limit_check(
    &self,
    _ip: std::net::IpAddr,
    _rate: u32,
    _capacity: u32,
  ) -> Result<bool, anyhow::Error> {
    // SQLite doesn't support distributed rate limiting, always allow
    // The actual rate limiting happens in-memory via RateLimiter
    Ok(true)
  }

  async fn connection_acquire(
    &self,
    _ip: std::net::IpAddr,
    _max_connections: u32,
  ) -> Result<bool, anyhow::Error> {
    // SQLite uses in-memory connection tracking
    Ok(true)
  }

  async fn connection_release(&self, _ip: std::net::IpAddr) -> Result<(), anyhow::Error> {
    Ok(())
  }

  // =========================================================================
  // S3 Storage Methods - SQLite stubs (not implemented)
  // =========================================================================

  async fn get_storage_access_key(
    &self,
    _access_key_id: &str,
  ) -> Result<Option<(String, Option<Uuid>)>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn create_storage_access_key(
    &self,
    _access_key_id: &str,
    _secret_key: &str,
    _owner_id: Option<Uuid>,
    _name: &str,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn delete_storage_access_key(&self, _access_key_id: &str) -> Result<bool, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn list_storage_access_keys(&self) -> Result<Vec<StorageAccessKeyInfo>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn get_storage_bucket(&self, _name: &str) -> Result<Option<StorageBucket>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn create_storage_bucket(
    &self,
    _name: &str,
    _owner_id: Option<Uuid>,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn delete_storage_bucket(&self, _name: &str) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn list_storage_buckets(&self) -> Result<Vec<StorageBucket>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn update_storage_bucket_stats(
    &self,
    _bucket: &str,
    _size_delta: i64,
    _count_delta: i64,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn get_storage_object(
    &self,
    _bucket: &str,
    _key: &str,
    _version_id: Option<Uuid>,
  ) -> Result<Option<StorageObject>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn create_storage_object(
    &self,
    _bucket: &str,
    _key: &str,
    _version_id: Uuid,
    _etag: &str,
    _size: i64,
    _content_type: &str,
    _storage_path: &str,
    _metadata: serde_json::Value,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn delete_storage_object(
    &self,
    _bucket: &str,
    _key: &str,
    _version_id: Option<Uuid>,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn create_storage_delete_marker(
    &self,
    _bucket: &str,
    _key: &str,
    _version_id: Uuid,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn unset_storage_object_latest(
    &self,
    _bucket: &str,
    _key: &str,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn update_storage_object_acl(
    &self,
    _bucket: &str,
    _key: &str,
    _acl: ObjectAcl,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn list_storage_objects(
    &self,
    _bucket: &str,
    _prefix: Option<&str>,
    _delimiter: Option<&str>,
    _max_keys: i32,
    _continuation_token: Option<&str>,
  ) -> Result<(Vec<StorageObject>, bool, Option<String>), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn list_storage_common_prefixes(
    &self,
    _bucket: &str,
    _prefix: Option<&str>,
    _delimiter: Option<&str>,
  ) -> Result<Vec<String>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn list_storage_object_versions(
    &self,
    _bucket: &str,
    _prefix: Option<&str>,
    _max_keys: i32,
  ) -> Result<(Vec<StorageObject>, bool, Option<String>), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn get_multipart_upload(
    &self,
    _upload_id: Uuid,
  ) -> Result<Option<MultipartUpload>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn create_multipart_upload(
    &self,
    _upload_id: Uuid,
    _bucket: &str,
    _key: &str,
    _content_type: Option<&str>,
    _metadata: serde_json::Value,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn delete_multipart_upload(&self, _upload_id: Uuid) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn list_multipart_uploads(
    &self,
    _bucket: &str,
    _max_uploads: i32,
  ) -> Result<(Vec<MultipartUpload>, bool), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn get_multipart_part(
    &self,
    _upload_id: Uuid,
    _part_number: i32,
  ) -> Result<Option<MultipartPart>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn upsert_multipart_part(
    &self,
    _upload_id: Uuid,
    _part_number: i32,
    _etag: &str,
    _size: i64,
    _storage_path: &str,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn list_multipart_parts(
    &self,
    _upload_id: Uuid,
    _max_parts: i32,
  ) -> Result<(Vec<MultipartPart>, bool), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  // =========================================================================
  // Feature Settings Methods
  // =========================================================================

  async fn get_feature_settings(
    &self,
    _name: &str,
  ) -> Result<Option<(bool, serde_json::Value)>, anyhow::Error> {
    // SQLite doesn't support feature settings storage (features not available)
    Ok(None)
  }

  async fn update_feature_settings(
    &self,
    _name: &str,
    _enabled: bool,
    _settings: serde_json::Value,
  ) -> Result<(), anyhow::Error> {
    // SQLite doesn't support feature settings storage
    anyhow::bail!("Feature settings are not supported with SQLite backend")
  }

  // =========================================================================
  // Admin Users (authentication) - Stubs for SQLite
  // =========================================================================

  async fn has_admin_users(&self) -> Result<bool, anyhow::Error> {
    // SQLite admin auth not yet implemented - return false to allow setup
    Ok(false)
  }

  async fn create_admin_user(
    &self,
    _username: &str,
    _email: Option<&str>,
    _password_hash: &str,
    _role: AdminRole,
  ) -> Result<AdminUser, anyhow::Error> {
    anyhow::bail!("Admin authentication requires PostgreSQL backend")
  }

  async fn get_admin_user_by_username(
    &self,
    _username: &str,
  ) -> Result<Option<(AdminUser, String)>, anyhow::Error> {
    Ok(None) // No users in SQLite mode
  }

  async fn get_admin_user(&self, _id: Uuid) -> Result<Option<AdminUser>, anyhow::Error> {
    Ok(None)
  }

  async fn list_admin_users(&self) -> Result<Vec<AdminUser>, anyhow::Error> {
    Ok(vec![])
  }

  async fn delete_admin_user(&self, _id: Uuid) -> Result<bool, anyhow::Error> {
    Ok(false)
  }

  async fn update_admin_user_role(
    &self,
    _id: Uuid,
    _role: AdminRole,
  ) -> Result<bool, anyhow::Error> {
    Ok(false)
  }

  // =========================================================================
  // Admin Sessions - Stubs for SQLite
  // =========================================================================

  async fn create_admin_session(
    &self,
    _user_id: Uuid,
    _session_token_hash: &str,
    _expires_at: chrono::DateTime<chrono::Utc>,
  ) -> Result<AdminSession, anyhow::Error> {
    anyhow::bail!("Admin authentication requires PostgreSQL backend")
  }

  async fn validate_admin_session(
    &self,
    _session_token_hash: &str,
  ) -> Result<Option<(AdminSession, AdminUser)>, anyhow::Error> {
    Ok(None)
  }

  async fn delete_admin_session(&self, _session_id: Uuid) -> Result<bool, anyhow::Error> {
    Ok(false)
  }

  async fn delete_admin_sessions_for_user(&self, _user_id: Uuid) -> Result<u64, anyhow::Error> {
    Ok(0)
  }

  async fn cleanup_expired_sessions(&self) -> Result<u64, anyhow::Error> {
    Ok(0)
  }

  // =========================================================================
  // S3 Atomic Operations (stubs - S3 not supported on SQLite)
  // =========================================================================

  async fn create_storage_object_with_stats(
    &self,
    _bucket: &str,
    _key: &str,
    _version_id: Uuid,
    _etag: &str,
    _size: i64,
    _content_type: &str,
    _storage_path: &str,
    _metadata: serde_json::Value,
  ) -> Result<(), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn delete_storage_object_with_stats(
    &self,
    _bucket: &str,
    _key: &str,
    _version_id: Option<Uuid>,
  ) -> Result<Option<(String, i64)>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn replace_storage_object(
    &self,
    _bucket: &str,
    _key: &str,
    _version_id: Uuid,
    _etag: &str,
    _size: i64,
    _content_type: &str,
    _storage_path: &str,
    _metadata: serde_json::Value,
  ) -> Result<Option<String>, anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }

  async fn list_storage_objects_with_prefixes(
    &self,
    _bucket: &str,
    _prefix: Option<&str>,
    _delimiter: Option<&str>,
    _max_keys: i32,
    _continuation_token: Option<&str>,
  ) -> Result<(Vec<StorageObject>, Vec<String>, bool, Option<String>), anyhow::Error> {
    anyhow::bail!("S3 storage is not supported with SQLite backend")
  }
}

#[inline]
fn row_to_doc(row: &rusqlite::Row) -> Result<Document, rusqlite::Error> {
  let id_str: String = row.get(0)?;
  let data_str: String = row.get(2)?;
  let created_str: String = row.get(3)?;
  let updated_str: String = row.get(4)?;
  Ok(Document {
    id: id_str.parse().unwrap_or_default(),
    collection: row.get(1)?,
    data: serde_json::from_str(&data_str).unwrap_or(serde_json::Value::Null),
    created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
      .map(|d| d.with_timezone(&Utc))
      .unwrap_or_else(|_| Utc::now()),
    updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
      .map(|d| d.with_timezone(&Utc))
      .unwrap_or_else(|_| Utc::now()),
  })
}
