use async_trait::async_trait;
use chrono::Utc;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio::sync::broadcast;
use tokio_postgres::NoTls;
use uuid::Uuid;

use super::backend::{ApiTokenInfo, DatabaseBackend, SqlDialect};
use super::sanitize::{validate_collection_name, validate_identifier, validate_limit};
use crate::types::{Change, ChangeOperation, Document, OrderBySpec, OrderDirection};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    collection VARCHAR(255) NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection);
CREATE INDEX IF NOT EXISTS idx_documents_data ON documents USING GIN(data);

CREATE TABLE IF NOT EXISTS change_queue (
    id BIGSERIAL PRIMARY KEY,
    collection VARCHAR(255) NOT NULL,
    document_id UUID NOT NULL,
    operation VARCHAR(10) NOT NULL,
    old_data JSONB,
    new_data JSONB,
    changed_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_change_queue_id ON change_queue(id);
CREATE INDEX IF NOT EXISTS idx_change_queue_collection ON change_queue(collection);

CREATE OR REPLACE FUNCTION capture_document_changes() RETURNS TRIGGER AS $$
DECLARE
    change_id BIGINT;
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO change_queue (collection, document_id, operation, new_data)
        VALUES (NEW.collection, NEW.id, 'INSERT', NEW.data)
        RETURNING id INTO change_id;
    ELSIF TG_OP = 'UPDATE' THEN
        INSERT INTO change_queue (collection, document_id, operation, old_data, new_data)
        VALUES (NEW.collection, NEW.id, 'UPDATE', OLD.data, NEW.data)
        RETURNING id INTO change_id;
    ELSIF TG_OP = 'DELETE' THEN
        INSERT INTO change_queue (collection, document_id, operation, old_data)
        VALUES (OLD.collection, OLD.id, 'DELETE', OLD.data)
        RETURNING id INTO change_id;
    END IF;
    -- Notify immediately with the change_id for instant processing
    PERFORM pg_notify('doc_changes', change_id::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS document_changes_trigger ON documents;
CREATE TRIGGER document_changes_trigger AFTER INSERT OR UPDATE OR DELETE ON documents FOR EACH ROW EXECUTE FUNCTION capture_document_changes();

CREATE TABLE IF NOT EXISTS api_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    token_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash);
"#;

pub struct PostgresBackend {
  pool: Pool,
  url: String,
  change_tx: broadcast::Sender<Change>,
}

impl PostgresBackend {
  pub fn new(url: &str, _max_connections: usize) -> Result<Self, anyhow::Error> {
    let mut cfg = Config::new();
    cfg.url = Some(url.into());
    cfg.manager = Some(ManagerConfig {
      recycling_method: RecyclingMethod::Fast,
    });
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
    let (change_tx, _) = broadcast::channel(1024);
    Ok(Self {
      pool,
      url: url.into(),
      change_tx,
    })
  }
}

#[async_trait]
impl DatabaseBackend for PostgresBackend {
  fn dialect(&self) -> SqlDialect {
    SqlDialect::Postgres
  }

  async fn init_schema(&self) -> Result<(), anyhow::Error> {
    self.pool.get().await?.batch_execute(SCHEMA).await?;
    tracing::info!("PostgreSQL schema initialized");
    Ok(())
  }

  async fn drop_schema(&self) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .batch_execute(
        "DROP TRIGGER IF EXISTS document_changes_trigger ON documents;
       DROP FUNCTION IF EXISTS capture_document_changes();
       DROP TABLE IF EXISTS change_queue; DROP TABLE IF EXISTS documents;",
      )
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
    self.pool.get().await?.execute(
      "INSERT INTO documents (id, collection, data, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)",
      &[&id, &collection, &data, &now, &now],
    ).await?;
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

    let row = self.pool.get().await?.query_opt(
      "SELECT id, collection, data, created_at, updated_at FROM documents WHERE collection = $1 AND id = $2",
      &[&collection, &id],
    ).await?;
    Ok(row.map(|r| Document {
      id: r.get(0),
      collection: r.get(1),
      data: r.get(2),
      created_at: r.get(3),
      updated_at: r.get(4),
    }))
  }

  async fn update(
    &self,
    collection: &str,
    id: Uuid,
    data: serde_json::Value,
  ) -> Result<Option<Document>, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    let now = Utc::now();
    let row = self.pool.get().await?.query_opt(
      "UPDATE documents SET data = $1, updated_at = $2 WHERE collection = $3 AND id = $4 RETURNING id, collection, data, created_at, updated_at",
      &[&data, &now, &collection, &id],
    ).await?;
    Ok(row.map(|r| Document {
      id: r.get(0),
      collection: r.get(1),
      data: r.get(2),
      created_at: r.get(3),
      updated_at: r.get(4),
    }))
  }

  async fn delete(&self, collection: &str, id: Uuid) -> Result<Option<Document>, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    let row = self.pool.get().await?.query_opt(
      "DELETE FROM documents WHERE collection = $1 AND id = $2 RETURNING id, collection, data, created_at, updated_at",
      &[&collection, &id],
    ).await?;
    Ok(row.map(|r| Document {
      id: r.get(0),
      collection: r.get(1),
      data: r.get(2),
      created_at: r.get(3),
      updated_at: r.get(4),
    }))
  }

  async fn list(
    &self,
    collection: &str,
    filter: Option<&str>,
    order: Option<&OrderBySpec>,
    limit: Option<usize>,
  ) -> Result<Vec<Document>, anyhow::Error> {
    // Validate collection name to prevent injection
    validate_collection_name(collection)?;

    let mut sql =
      "SELECT id, collection, data, created_at, updated_at FROM documents WHERE collection = $1"
        .to_string();

    // Filter is pre-validated by query compiler - only append if present
    // The compiler ensures only safe SQL is generated
    if let Some(f) = filter {
      sql.push_str(" AND ");
      sql.push_str(f);
    }

    if let Some(o) = order {
      // Validate field name to prevent injection
      validate_identifier(&o.field)?;
      let dir = if o.direction == OrderDirection::Desc {
        "DESC"
      } else {
        "ASC"
      };
      sql.push_str(&format!(" ORDER BY data->>'{}' {}", o.field, dir));
    }

    if let Some(l) = limit {
      // Validate limit is within bounds
      validate_limit(l)?;
      sql.push_str(&format!(" LIMIT {}", l));
    }

    let rows = self.pool.get().await?.query(&sql, &[&collection]).await?;
    Ok(
      rows
        .into_iter()
        .map(|r| Document {
          id: r.get(0),
          collection: r.get(1),
          data: r.get(2),
          created_at: r.get(3),
          updated_at: r.get(4),
        })
        .collect(),
    )
  }

  async fn list_collections(&self) -> Result<Vec<String>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT DISTINCT collection FROM documents ORDER BY collection",
        &[],
      )
      .await?;
    Ok(rows.into_iter().map(|r| r.get(0)).collect())
  }

  fn subscribe_changes(&self) -> broadcast::Receiver<Change> {
    self.change_tx.subscribe()
  }

  async fn start_change_listener(&self) -> Result<(), anyhow::Error> {
    // Get the notification stream from the connection
    let (tx_notifications, mut rx_notifications) = tokio::sync::mpsc::unbounded_channel::<i64>();

    // Create a dedicated connection for listening to notifications
    let (listen_client, mut listen_connection) = tokio_postgres::connect(&self.url, NoTls).await?;

    // Spawn a task to process notifications
    let tx_notif = tx_notifications;
    tokio::spawn(async move {
      // Poll connection and extract notifications
      loop {
        match futures_util::future::poll_fn(|cx| listen_connection.poll_message(cx)).await {
          Some(Ok(tokio_postgres::AsyncMessage::Notification(n))) => {
            if let Ok(change_id) = n.payload().parse::<i64>() {
              let _ = tx_notif.send(change_id);
            }
          }
          Some(Ok(_)) => {}
          Some(Err(e)) => {
            tracing::error!("PostgreSQL notification error: {}", e);
            break;
          }
          None => break,
        }
      }
    });

    listen_client.execute("LISTEN doc_changes", &[]).await?;
    tracing::info!("PostgreSQL LISTEN/NOTIFY change listener started");

    let tx = self.change_tx.clone();
    let pool = self.pool.clone();

    tokio::spawn(async move {
      let mut last_id: i64 = 0;

      loop {
        tokio::select! {
          // Process notifications immediately (< 1ms latency)
          Some(change_id) = rx_notifications.recv() => {
            // Fetch the specific change by ID
            let Ok(conn) = pool.get().await else { continue };
            let Ok(rows) = conn.query(
              "SELECT id, collection, document_id, operation, old_data, new_data, changed_at FROM change_queue WHERE id = $1",
              &[&change_id]
            ).await else { continue };

            for row in rows {
              let id: i64 = row.get(0);
              let Ok(op) = row.get::<_, String>(3).parse::<ChangeOperation>() else {
                continue;
              };
              let _ = tx.send(Change {
                id,
                collection: row.get(1),
                document_id: row.get(2),
                operation: op,
                old_data: row.get(4),
                new_data: row.get(5),
                changed_at: row.get(6),
              });
              if id > last_id {
                last_id = id;
              }
            }
          }
          // Fallback polling every 5s to catch any missed notifications
          _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
            let Ok(conn) = pool.get().await else { continue };
            let Ok(rows) = conn.query(
              "SELECT id, collection, document_id, operation, old_data, new_data, changed_at FROM change_queue WHERE id > $1 ORDER BY id LIMIT 100",
              &[&last_id]
            ).await else { continue };

            for row in rows {
              let id: i64 = row.get(0);
              let Ok(op) = row.get::<_, String>(3).parse::<ChangeOperation>() else {
                continue;
              };
              let _ = tx.send(Change {
                id,
                collection: row.get(1),
                document_id: row.get(2),
                operation: op,
                old_data: row.get(4),
                new_data: row.get(5),
                changed_at: row.get(6),
              });
              last_id = id;
            }
          }
        }
      }
    });

    // Spawn cleanup task to prevent unbounded growth of change_queue
    let cleanup_pool = self.pool.clone();
    tokio::spawn(async move {
      loop {
        // Run cleanup every 5 minutes
        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
        let Ok(conn) = cleanup_pool.get().await else {
          continue;
        };
        // Keep only the last 10000 entries (or entries from the last hour)
        let result = conn.execute(
          "DELETE FROM change_queue WHERE id < (SELECT MAX(id) - 10000 FROM change_queue) AND changed_at < NOW() - INTERVAL '1 hour'",
          &[]
        ).await;
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
    self
      .pool
      .get()
      .await?
      .execute(
        "INSERT INTO api_tokens (id, name, token_hash, created_at) VALUES ($1, $2, $3, $4)",
        &[&id, &name, &token_hash, &now],
      )
      .await?;
    Ok(ApiTokenInfo {
      id,
      name: name.into(),
      created_at: now,
    })
  }

  async fn delete_token(&self, id: Uuid) -> Result<bool, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute("DELETE FROM api_tokens WHERE id = $1", &[&id])
      .await?;
    Ok(result > 0)
  }

  async fn list_tokens(&self) -> Result<Vec<ApiTokenInfo>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT id, name, created_at FROM api_tokens ORDER BY created_at DESC",
        &[],
      )
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| ApiTokenInfo {
          id: r.get(0),
          name: r.get(1),
          created_at: r.get(2),
        })
        .collect(),
    )
  }

  async fn validate_token(&self, token_hash: &str) -> Result<bool, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT 1 FROM api_tokens WHERE token_hash = $1",
        &[&token_hash],
      )
      .await?;
    Ok(row.is_some())
  }
}
