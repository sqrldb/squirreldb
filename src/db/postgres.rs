use async_trait::async_trait;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio::sync::broadcast;
use tokio_postgres::NoTls;
use uuid::Uuid;

use super::backend::{ApiTokenInfo, DatabaseBackend, S3AccessKeyInfo, SqlDialect};
use super::sanitize::{validate_collection_name, validate_identifier, validate_limit};
use crate::s3::{ObjectAcl, S3Bucket, S3MultipartUpload, S3Object, S3Part};
use crate::types::{Change, ChangeOperation, Document, OrderBySpec, OrderDirection};

/// Pipe trait for method chaining
trait Pipe: Sized {
  fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R {
    f(self)
  }
}

impl<T> Pipe for T {}

const SCHEMA: &str = r#"
-- JavaScript-friendly UUID alias
CREATE OR REPLACE FUNCTION uuid() RETURNS UUID AS $$
  SELECT gen_random_uuid();
$$ LANGUAGE SQL;

CREATE TABLE IF NOT EXISTS documents (
    id UUID PRIMARY KEY DEFAULT uuid(),
    collection VARCHAR(255) NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection);
CREATE INDEX IF NOT EXISTS idx_documents_data ON documents USING GIN(data);

-- Optimized change_queue with delta storage and fillfactor for INSERT-heavy workload
CREATE TABLE IF NOT EXISTS change_queue (
    id BIGSERIAL PRIMARY KEY,
    collection VARCHAR(255) NOT NULL,
    document_id UUID NOT NULL,
    operation VARCHAR(10) NOT NULL,
    old_data JSONB,
    new_data JSONB,
    delta JSONB,  -- Only changed fields for UPDATE operations (reduces storage 50-70%)
    changed_at TIMESTAMPTZ DEFAULT NOW()
);
ALTER TABLE change_queue SET (fillfactor = 70);
CREATE INDEX IF NOT EXISTS idx_change_queue_id ON change_queue(id);
CREATE INDEX IF NOT EXISTS idx_change_queue_collection ON change_queue(collection);
CREATE INDEX IF NOT EXISTS idx_change_queue_changed_at ON change_queue(changed_at);

-- Function to compute delta between two JSONB objects (only top-level keys that changed)
CREATE OR REPLACE FUNCTION sqrl_json_delta(old_data JSONB, new_data JSONB) RETURNS JSONB AS $$
DECLARE
    result JSONB := '{}';
    key TEXT;
BEGIN
    -- If either is NULL, return NULL (no delta possible)
    IF old_data IS NULL OR new_data IS NULL THEN
        RETURN NULL;
    END IF;

    -- Find keys that are different or new in new_data
    FOR key IN SELECT jsonb_object_keys(new_data)
    LOOP
        IF NOT old_data ? key OR old_data->key IS DISTINCT FROM new_data->key THEN
            result := result || jsonb_build_object(key, new_data->key);
        END IF;
    END LOOP;

    -- Find keys that were removed (present in old, not in new)
    FOR key IN SELECT jsonb_object_keys(old_data)
    LOOP
        IF NOT new_data ? key THEN
            result := result || jsonb_build_object(key, NULL);
        END IF;
    END LOOP;

    RETURN result;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Optimized trigger with delta calculation
CREATE OR REPLACE FUNCTION capture_document_changes() RETURNS TRIGGER AS $$
DECLARE
    change_id BIGINT;
    computed_delta JSONB;
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO change_queue (collection, document_id, operation, new_data)
        VALUES (NEW.collection, NEW.id, 'INSERT', NEW.data)
        RETURNING id INTO change_id;
    ELSIF TG_OP = 'UPDATE' THEN
        -- Compute delta for UPDATE operations
        computed_delta := sqrl_json_delta(OLD.data, NEW.data);
        INSERT INTO change_queue (collection, document_id, operation, old_data, new_data, delta)
        VALUES (NEW.collection, NEW.id, 'UPDATE', OLD.data, NEW.data, computed_delta)
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

-- Auto-cleanup function: keeps last N entries or entries within time window
CREATE OR REPLACE FUNCTION sqrl_cleanup_change_queue(
    max_entries INTEGER DEFAULT 10000,
    max_age INTERVAL DEFAULT INTERVAL '1 hour'
) RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
    min_id BIGINT;
BEGIN
    -- Find the minimum ID to keep (either by count or by age, whichever is more permissive)
    SELECT GREATEST(
        COALESCE((SELECT MAX(id) - max_entries FROM change_queue), 0),
        COALESCE((SELECT MIN(id) FROM change_queue WHERE changed_at > NOW() - max_age), 0)
    ) INTO min_id;

    -- Delete old entries
    DELETE FROM change_queue WHERE id < min_id;
    GET DIAGNOSTICS deleted_count = ROW_COUNT;

    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Subscription filters table for PostgreSQL-side filtering
CREATE TABLE IF NOT EXISTS subscription_filters (
    id BIGSERIAL PRIMARY KEY,
    subscription_id VARCHAR(255) NOT NULL,
    client_id UUID NOT NULL,
    collection VARCHAR(255) NOT NULL,
    compiled_sql TEXT,  -- Pre-compiled SQL WHERE clause (NULL = match all)
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(client_id, subscription_id)
);
CREATE INDEX IF NOT EXISTS idx_subscription_filters_collection ON subscription_filters(collection);
CREATE INDEX IF NOT EXISTS idx_subscription_filters_client ON subscription_filters(client_id);

-- Function to evaluate if a JSONB document matches a compiled SQL filter
-- This is used for PostgreSQL-side subscription filtering
CREATE OR REPLACE FUNCTION sqrl_filter_matches(doc_data JSONB, filter_sql TEXT) RETURNS BOOLEAN AS $$
DECLARE
    result BOOLEAN;
BEGIN
    -- NULL filter means match all
    IF filter_sql IS NULL OR filter_sql = '' THEN
        RETURN TRUE;
    END IF;

    -- Evaluate the filter against the document using dynamic SQL
    -- The filter_sql is pre-compiled and validated by Rust, so it's safe
    EXECUTE format('SELECT %s', filter_sql)
    USING doc_data
    INTO result;

    RETURN COALESCE(result, FALSE);
EXCEPTION
    WHEN OTHERS THEN
        -- If filter evaluation fails, don't match (safer default)
        RETURN FALSE;
END;
$$ LANGUAGE plpgsql;

-- Function to broadcast change to matching subscriptions only
-- Returns the number of subscriptions notified
CREATE OR REPLACE FUNCTION sqrl_broadcast_filtered_change(
    change_id BIGINT,
    change_collection VARCHAR(255),
    change_data JSONB
) RETURNS INTEGER AS $$
DECLARE
    filter_row RECORD;
    notified_count INTEGER := 0;
    matches BOOLEAN;
BEGIN
    -- Find all subscriptions for this collection and check if they match
    FOR filter_row IN
        SELECT subscription_id, client_id, compiled_sql
        FROM subscription_filters
        WHERE collection = change_collection
    LOOP
        -- Check if the data matches this subscription's filter
        IF filter_row.compiled_sql IS NULL THEN
            matches := TRUE;
        ELSE
            matches := sqrl_filter_matches(change_data, filter_row.compiled_sql);
        END IF;

        IF matches THEN
            -- Notify with subscription-specific payload
            PERFORM pg_notify('filtered_changes', json_build_object(
                'change_id', change_id,
                'client_id', filter_row.client_id,
                'subscription_id', filter_row.subscription_id
            )::text);
            notified_count := notified_count + 1;
        END IF;
    END LOOP;

    RETURN notified_count;
END;
$$ LANGUAGE plpgsql;

-- Subscription management functions
CREATE OR REPLACE FUNCTION sqrl_add_subscription(
    p_client_id UUID,
    p_subscription_id VARCHAR(255),
    p_collection VARCHAR(255),
    p_compiled_sql TEXT DEFAULT NULL
) RETURNS VOID AS $$
BEGIN
    INSERT INTO subscription_filters (client_id, subscription_id, collection, compiled_sql)
    VALUES (p_client_id, p_subscription_id, p_collection, p_compiled_sql)
    ON CONFLICT (client_id, subscription_id)
    DO UPDATE SET collection = p_collection, compiled_sql = p_compiled_sql;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sqrl_remove_subscription(
    p_client_id UUID,
    p_subscription_id VARCHAR(255)
) RETURNS VOID AS $$
BEGIN
    DELETE FROM subscription_filters
    WHERE client_id = p_client_id AND subscription_id = p_subscription_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sqrl_remove_client_subscriptions(p_client_id UUID) RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM subscription_filters WHERE client_id = p_client_id;
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Rate limiting table for distributed rate limiting (Phase 5 preparation)
CREATE TABLE IF NOT EXISTS rate_limits (
    ip INET PRIMARY KEY,
    tokens NUMERIC DEFAULT 100,
    capacity NUMERIC DEFAULT 100,
    rate NUMERIC DEFAULT 10,  -- tokens per second
    last_refill TIMESTAMPTZ DEFAULT NOW(),
    connection_count INTEGER DEFAULT 0
);

-- Atomic rate limit check and consume function
CREATE OR REPLACE FUNCTION sqrl_rate_limit_check(
    check_ip INET,
    default_rate NUMERIC DEFAULT 10,
    default_capacity NUMERIC DEFAULT 100
) RETURNS BOOLEAN AS $$
DECLARE
    current_tokens NUMERIC;
    elapsed_secs NUMERIC;
    row_found BOOLEAN;
BEGIN
    -- Try to get existing rate limit entry
    SELECT TRUE INTO row_found FROM rate_limits WHERE ip = check_ip FOR UPDATE;

    IF row_found THEN
        -- Refill tokens based on elapsed time and check
        UPDATE rate_limits
        SET
            tokens = LEAST(capacity, tokens + (EXTRACT(EPOCH FROM NOW() - last_refill) * rate)) - 1,
            last_refill = NOW()
        WHERE ip = check_ip
          AND (tokens + (EXTRACT(EPOCH FROM NOW() - last_refill) * rate)) >= 1
        RETURNING tokens INTO current_tokens;

        RETURN current_tokens IS NOT NULL;
    ELSE
        -- Create new entry with one token consumed
        INSERT INTO rate_limits (ip, tokens, capacity, rate, last_refill)
        VALUES (check_ip, default_capacity - 1, default_capacity, default_rate, NOW())
        ON CONFLICT (ip) DO UPDATE
        SET tokens = LEAST(rate_limits.capacity, rate_limits.tokens + 1) - 1,
            last_refill = NOW()
        WHERE rate_limits.tokens >= 0;

        RETURN TRUE;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Connection tracking functions
CREATE OR REPLACE FUNCTION sqrl_connection_acquire(check_ip INET, max_connections INTEGER DEFAULT 100)
RETURNS BOOLEAN AS $$
DECLARE
    current_count INTEGER;
BEGIN
    INSERT INTO rate_limits (ip, connection_count)
    VALUES (check_ip, 1)
    ON CONFLICT (ip) DO UPDATE
    SET connection_count = rate_limits.connection_count + 1
    WHERE rate_limits.connection_count < max_connections
    RETURNING connection_count INTO current_count;

    RETURN current_count IS NOT NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sqrl_connection_release(check_ip INET)
RETURNS VOID AS $$
BEGIN
    UPDATE rate_limits
    SET connection_count = GREATEST(0, connection_count - 1)
    WHERE ip = check_ip;

    -- Clean up entries with no connections and full tokens
    DELETE FROM rate_limits
    WHERE ip = check_ip
      AND connection_count = 0
      AND tokens >= capacity;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE IF NOT EXISTS api_tokens (
    id UUID PRIMARY KEY DEFAULT uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    token_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash);

-- S3 Buckets
CREATE TABLE IF NOT EXISTS s3_buckets (
    name VARCHAR(63) PRIMARY KEY,
    owner_id UUID,
    versioning_enabled BOOLEAN DEFAULT FALSE,
    acl JSONB DEFAULT '{"grants": []}',
    lifecycle_rules JSONB DEFAULT '[]',
    quota_bytes BIGINT,
    current_size BIGINT DEFAULT 0,
    object_count BIGINT DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- S3 Objects
CREATE TABLE IF NOT EXISTS s3_objects (
    bucket VARCHAR(63) NOT NULL,
    key TEXT NOT NULL,
    version_id UUID DEFAULT uuid(),
    is_latest BOOLEAN DEFAULT TRUE,
    etag VARCHAR(32) NOT NULL,
    size BIGINT NOT NULL,
    content_type VARCHAR(255) DEFAULT 'application/octet-stream',
    storage_path TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    acl JSONB DEFAULT '{"grants": []}',
    is_delete_marker BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (bucket, key, version_id),
    FOREIGN KEY (bucket) REFERENCES s3_buckets(name) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_s3_objects_bucket_key ON s3_objects(bucket, key);
CREATE INDEX IF NOT EXISTS idx_s3_objects_latest ON s3_objects(bucket, key) WHERE is_latest = TRUE;

-- Multipart Uploads
CREATE TABLE IF NOT EXISTS s3_multipart_uploads (
    upload_id UUID PRIMARY KEY DEFAULT uuid(),
    bucket VARCHAR(63) NOT NULL,
    key TEXT NOT NULL,
    content_type VARCHAR(255),
    metadata JSONB DEFAULT '{}',
    initiated_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (bucket) REFERENCES s3_buckets(name) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS s3_multipart_parts (
    upload_id UUID NOT NULL,
    part_number INTEGER NOT NULL,
    etag VARCHAR(32) NOT NULL,
    size BIGINT NOT NULL,
    storage_path TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (upload_id, part_number),
    FOREIGN KEY (upload_id) REFERENCES s3_multipart_uploads(upload_id) ON DELETE CASCADE
);

-- S3 Access Keys (for AWS Signature V4)
CREATE TABLE IF NOT EXISTS s3_access_keys (
    access_key_id VARCHAR(20) PRIMARY KEY,
    secret_access_key VARCHAR(64) NOT NULL,
    owner_id UUID,
    name VARCHAR(255) NOT NULL,
    permissions JSONB DEFAULT '{"buckets": "*", "actions": "*"}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Feature settings for runtime configuration
CREATE TABLE IF NOT EXISTS feature_settings (
    feature_name VARCHAR(255) PRIMARY KEY,
    enabled BOOLEAN DEFAULT FALSE,
    settings JSONB NOT NULL DEFAULT '{}',
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
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

    // Let PostgreSQL generate UUID and timestamps via DEFAULTs, use RETURNING to get them back
    let row = self.pool.get().await?.query_one(
      "INSERT INTO documents (collection, data) VALUES ($1, $2) RETURNING id, collection, data, created_at, updated_at",
      &[&collection, &data],
    ).await?;

    Ok(Document {
      id: row.get(0),
      collection: row.get(1),
      data: row.get(2),
      created_at: row.get(3),
      updated_at: row.get(4),
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

    // Let PostgreSQL generate updated_at via NOW()
    let row = self.pool.get().await?.query_opt(
      "UPDATE documents SET data = $1, updated_at = NOW() WHERE collection = $2 AND id = $3 RETURNING id, collection, data, created_at, updated_at",
      &[&data, &collection, &id],
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
    offset: Option<usize>,
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

    if let Some(o) = offset {
      // Validate offset is within bounds
      if o > 1_000_000 {
        anyhow::bail!("Offset too large (max 1000000)");
      }
      sql.push_str(&format!(" OFFSET {}", o));
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

    // Spawn cleanup task using PostgreSQL function for efficient cleanup
    let cleanup_pool = self.pool.clone();
    tokio::spawn(async move {
      loop {
        // Run cleanup every 5 minutes
        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
        let Ok(conn) = cleanup_pool.get().await else {
          continue;
        };
        // Use PostgreSQL function for atomic, efficient cleanup
        let result = conn
          .query_one(
            "SELECT sqrl_cleanup_change_queue(10000, INTERVAL '1 hour')",
            &[],
          )
          .await;
        if let Ok(row) = result {
          let count: i32 = row.get(0);
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
    // Let PostgreSQL generate UUID and timestamp via DEFAULTs
    let row = self
      .pool
      .get()
      .await?
      .query_one(
        "INSERT INTO api_tokens (name, token_hash) VALUES ($1, $2) RETURNING id, name, created_at",
        &[&name, &token_hash],
      )
      .await?;
    Ok(ApiTokenInfo {
      id: row.get(0),
      name: row.get(1),
      created_at: row.get(2),
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

  // Subscription filter methods for PostgreSQL-side filtering
  async fn add_subscription_filter(
    &self,
    client_id: Uuid,
    subscription_id: &str,
    collection: &str,
    compiled_sql: Option<&str>,
  ) -> Result<(), anyhow::Error> {
    validate_collection_name(collection)?;

    self
      .pool
      .get()
      .await?
      .execute(
        "SELECT sqrl_add_subscription($1, $2, $3, $4)",
        &[&client_id, &subscription_id, &collection, &compiled_sql],
      )
      .await?;
    Ok(())
  }

  async fn remove_subscription_filter(
    &self,
    client_id: Uuid,
    subscription_id: &str,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "SELECT sqrl_remove_subscription($1, $2)",
        &[&client_id, &subscription_id],
      )
      .await?;
    Ok(())
  }

  async fn remove_client_filters(&self, client_id: Uuid) -> Result<u64, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_one("SELECT sqrl_remove_client_subscriptions($1)", &[&client_id])
      .await?;
    let count: i32 = row.get(0);
    Ok(count as u64)
  }

  // Rate limiting methods using PostgreSQL for distributed limiting
  async fn rate_limit_check(
    &self,
    ip: std::net::IpAddr,
    rate: u32,
    capacity: u32,
  ) -> Result<bool, anyhow::Error> {
    let ip_str = ip.to_string();
    let row = self
      .pool
      .get()
      .await?
      .query_one(
        "SELECT sqrl_rate_limit_check($1::inet, $2, $3)",
        &[&ip_str, &(rate as i32), &(capacity as i32)],
      )
      .await?;
    Ok(row.get(0))
  }

  async fn connection_acquire(
    &self,
    ip: std::net::IpAddr,
    max_connections: u32,
  ) -> Result<bool, anyhow::Error> {
    let ip_str = ip.to_string();
    let row = self
      .pool
      .get()
      .await?
      .query_one(
        "SELECT sqrl_connection_acquire($1::inet, $2)",
        &[&ip_str, &(max_connections as i32)],
      )
      .await?;
    Ok(row.get(0))
  }

  async fn connection_release(&self, ip: std::net::IpAddr) -> Result<(), anyhow::Error> {
    let ip_str = ip.to_string();
    self
      .pool
      .get()
      .await?
      .execute("SELECT sqrl_connection_release($1::inet)", &[&ip_str])
      .await?;
    Ok(())
  }

  // =========================================================================
  // S3 Storage Methods
  // =========================================================================

  async fn get_s3_access_key(
    &self,
    access_key_id: &str,
  ) -> Result<Option<(String, Option<Uuid>)>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT secret_access_key, owner_id FROM s3_access_keys WHERE access_key_id = $1",
        &[&access_key_id],
      )
      .await?;
    Ok(row.map(|r| (r.get(0), r.get(1))))
  }

  async fn create_s3_access_key(
    &self,
    access_key_id: &str,
    secret_key: &str,
    owner_id: Option<Uuid>,
    name: &str,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "INSERT INTO s3_access_keys (access_key_id, secret_access_key, owner_id, name) VALUES ($1, $2, $3, $4)",
        &[&access_key_id, &secret_key, &owner_id, &name],
      )
      .await?;
    Ok(())
  }

  async fn delete_s3_access_key(&self, access_key_id: &str) -> Result<bool, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute(
        "DELETE FROM s3_access_keys WHERE access_key_id = $1",
        &[&access_key_id],
      )
      .await?;
    Ok(result > 0)
  }

  async fn list_s3_access_keys(&self) -> Result<Vec<S3AccessKeyInfo>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT access_key_id, owner_id, name, created_at FROM s3_access_keys ORDER BY created_at DESC",
        &[],
      )
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| S3AccessKeyInfo {
          access_key_id: r.get(0),
          owner_id: r.get(1),
          name: r.get(2),
          created_at: r.get(3),
        })
        .collect(),
    )
  }

  async fn get_s3_bucket(&self, name: &str) -> Result<Option<S3Bucket>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT name, owner_id, versioning_enabled, acl, lifecycle_rules, quota_bytes, current_size, object_count, created_at FROM s3_buckets WHERE name = $1",
        &[&name],
      )
      .await?;
    Ok(row.map(|r| {
      S3Bucket {
        name: r.get(0),
        owner_id: r.get(1),
        versioning_enabled: r.get(2),
        acl: r
          .get::<_, serde_json::Value>(3)
          .pipe(|v| serde_json::from_value(v).unwrap_or_default()),
        lifecycle_rules: r
          .get::<_, serde_json::Value>(4)
          .pipe(|v| serde_json::from_value(v).unwrap_or_default()),
        quota_bytes: r.get(5),
        current_size: r.get(6),
        object_count: r.get(7),
        created_at: r.get(8),
      }
    }))
  }

  async fn create_s3_bucket(
    &self,
    name: &str,
    owner_id: Option<Uuid>,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "INSERT INTO s3_buckets (name, owner_id) VALUES ($1, $2)",
        &[&name, &owner_id],
      )
      .await?;
    Ok(())
  }

  async fn delete_s3_bucket(&self, name: &str) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute("DELETE FROM s3_buckets WHERE name = $1", &[&name])
      .await?;
    Ok(())
  }

  async fn list_s3_buckets(&self) -> Result<Vec<S3Bucket>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT name, owner_id, versioning_enabled, acl, lifecycle_rules, quota_bytes, current_size, object_count, created_at FROM s3_buckets ORDER BY name",
        &[],
      )
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| S3Bucket {
          name: r.get(0),
          owner_id: r.get(1),
          versioning_enabled: r.get(2),
          acl: r
            .get::<_, serde_json::Value>(3)
            .pipe(|v| serde_json::from_value(v).unwrap_or_default()),
          lifecycle_rules: r
            .get::<_, serde_json::Value>(4)
            .pipe(|v| serde_json::from_value(v).unwrap_or_default()),
          quota_bytes: r.get(5),
          current_size: r.get(6),
          object_count: r.get(7),
          created_at: r.get(8),
        })
        .collect(),
    )
  }

  async fn update_s3_bucket_stats(
    &self,
    bucket: &str,
    size_delta: i64,
    count_delta: i64,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "UPDATE s3_buckets SET current_size = current_size + $2, object_count = object_count + $3 WHERE name = $1",
        &[&bucket, &size_delta, &count_delta],
      )
      .await?;
    Ok(())
  }

  async fn get_s3_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Option<Uuid>,
  ) -> Result<Option<S3Object>, anyhow::Error> {
    let row = if let Some(vid) = version_id {
      self
        .pool
        .get()
        .await?
        .query_opt(
          "SELECT bucket, key, version_id, is_latest, etag, size, content_type, storage_path, metadata, acl, is_delete_marker, created_at FROM s3_objects WHERE bucket = $1 AND key = $2 AND version_id = $3",
          &[&bucket, &key, &vid],
        )
        .await?
    } else {
      self
        .pool
        .get()
        .await?
        .query_opt(
          "SELECT bucket, key, version_id, is_latest, etag, size, content_type, storage_path, metadata, acl, is_delete_marker, created_at FROM s3_objects WHERE bucket = $1 AND key = $2 AND is_latest = TRUE",
          &[&bucket, &key],
        )
        .await?
    };
    Ok(row.map(|r| {
      S3Object {
        bucket: r.get(0),
        key: r.get(1),
        version_id: r.get(2),
        is_latest: r.get(3),
        etag: r.get(4),
        size: r.get(5),
        content_type: r.get(6),
        storage_path: r.get(7),
        metadata: r.get(8),
        acl: r
          .get::<_, serde_json::Value>(9)
          .pipe(|v| serde_json::from_value(v).unwrap_or_default()),
        is_delete_marker: r.get(10),
        created_at: r.get(11),
      }
    }))
  }

  async fn create_s3_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    etag: &str,
    size: i64,
    content_type: &str,
    storage_path: &str,
    metadata: serde_json::Value,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "INSERT INTO s3_objects (bucket, key, version_id, etag, size, content_type, storage_path, metadata) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[&bucket, &key, &version_id, &etag, &size, &content_type, &storage_path, &metadata],
      )
      .await?;
    Ok(())
  }

  async fn delete_s3_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Option<Uuid>,
  ) -> Result<(), anyhow::Error> {
    if let Some(vid) = version_id {
      self
        .pool
        .get()
        .await?
        .execute(
          "DELETE FROM s3_objects WHERE bucket = $1 AND key = $2 AND version_id = $3",
          &[&bucket, &key, &vid],
        )
        .await?;
    } else {
      self
        .pool
        .get()
        .await?
        .execute(
          "DELETE FROM s3_objects WHERE bucket = $1 AND key = $2",
          &[&bucket, &key],
        )
        .await?;
    }
    Ok(())
  }

  async fn create_s3_delete_marker(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
  ) -> Result<(), anyhow::Error> {
    // First unset latest on existing versions
    self.unset_s3_object_latest(bucket, key).await?;
    // Create delete marker
    self
      .pool
      .get()
      .await?
      .execute(
        "INSERT INTO s3_objects (bucket, key, version_id, etag, size, is_delete_marker, is_latest) VALUES ($1, $2, $3, '', 0, TRUE, TRUE)",
        &[&bucket, &key, &version_id],
      )
      .await?;
    Ok(())
  }

  async fn unset_s3_object_latest(&self, bucket: &str, key: &str) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "UPDATE s3_objects SET is_latest = FALSE WHERE bucket = $1 AND key = $2",
        &[&bucket, &key],
      )
      .await?;
    Ok(())
  }

  async fn update_s3_object_acl(
    &self,
    bucket: &str,
    key: &str,
    acl: ObjectAcl,
  ) -> Result<(), anyhow::Error> {
    let acl_json = serde_json::to_value(acl)?;
    self
      .pool
      .get()
      .await?
      .execute(
        "UPDATE s3_objects SET acl = $3 WHERE bucket = $1 AND key = $2 AND is_latest = TRUE",
        &[&bucket, &key, &acl_json],
      )
      .await?;
    Ok(())
  }

  async fn list_s3_objects(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    _delimiter: Option<&str>,
    max_keys: i32,
    continuation_token: Option<&str>,
  ) -> Result<(Vec<S3Object>, bool, Option<String>), anyhow::Error> {
    let prefix_pattern = prefix
      .map(|p| format!("{}%", p))
      .unwrap_or_else(|| "%".to_string());
    let start_key = continuation_token.unwrap_or("");

    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT bucket, key, version_id, is_latest, etag, size, content_type, storage_path, metadata, acl, is_delete_marker, created_at
         FROM s3_objects
         WHERE bucket = $1 AND key LIKE $2 AND key > $3 AND is_latest = TRUE AND is_delete_marker = FALSE
         ORDER BY key
         LIMIT $4",
        &[&bucket, &prefix_pattern, &start_key, &(max_keys + 1)],
      )
      .await?;

    let is_truncated = rows.len() > max_keys as usize;
    let objects: Vec<S3Object> = rows
      .into_iter()
      .take(max_keys as usize)
      .map(|r| S3Object {
        bucket: r.get(0),
        key: r.get(1),
        version_id: r.get(2),
        is_latest: r.get(3),
        etag: r.get(4),
        size: r.get(5),
        content_type: r.get(6),
        storage_path: r.get(7),
        metadata: r.get(8),
        acl: r
          .get::<_, serde_json::Value>(9)
          .pipe(|v| serde_json::from_value(v).unwrap_or_default()),
        is_delete_marker: r.get(10),
        created_at: r.get(11),
      })
      .collect();

    let next_token = if is_truncated {
      objects.last().map(|o| o.key.clone())
    } else {
      None
    };

    Ok((objects, is_truncated, next_token))
  }

  async fn list_s3_common_prefixes(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    delimiter: Option<&str>,
  ) -> Result<Vec<String>, anyhow::Error> {
    let Some(delim) = delimiter else {
      return Ok(vec![]);
    };

    let prefix_str = prefix.unwrap_or("");
    let prefix_len = prefix_str.len() as i32;

    // Find distinct prefixes up to the next delimiter
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT DISTINCT SUBSTRING(key FROM 1 FOR POSITION($2 IN SUBSTRING(key FROM $3)) + $3 - 1) as common_prefix
         FROM s3_objects
         WHERE bucket = $1 AND key LIKE $4 AND POSITION($2 IN SUBSTRING(key FROM $3)) > 0 AND is_latest = TRUE
         ORDER BY common_prefix",
        &[&bucket, &delim, &(prefix_len + 1), &format!("{}%", prefix_str)],
      )
      .await?;

    Ok(rows.into_iter().map(|r| r.get(0)).collect())
  }

  async fn list_s3_object_versions(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    max_keys: i32,
  ) -> Result<(Vec<S3Object>, bool, Option<String>), anyhow::Error> {
    let prefix_pattern = prefix
      .map(|p| format!("{}%", p))
      .unwrap_or_else(|| "%".to_string());

    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT bucket, key, version_id, is_latest, etag, size, content_type, storage_path, metadata, acl, is_delete_marker, created_at
         FROM s3_objects
         WHERE bucket = $1 AND key LIKE $2
         ORDER BY key, created_at DESC
         LIMIT $3",
        &[&bucket, &prefix_pattern, &(max_keys + 1)],
      )
      .await?;

    let is_truncated = rows.len() > max_keys as usize;
    let objects: Vec<S3Object> = rows
      .into_iter()
      .take(max_keys as usize)
      .map(|r| S3Object {
        bucket: r.get(0),
        key: r.get(1),
        version_id: r.get(2),
        is_latest: r.get(3),
        etag: r.get(4),
        size: r.get(5),
        content_type: r.get(6),
        storage_path: r.get(7),
        metadata: r.get(8),
        acl: r
          .get::<_, serde_json::Value>(9)
          .pipe(|v| serde_json::from_value(v).unwrap_or_default()),
        is_delete_marker: r.get(10),
        created_at: r.get(11),
      })
      .collect();

    Ok((objects, is_truncated, None))
  }

  async fn get_s3_multipart_upload(
    &self,
    upload_id: Uuid,
  ) -> Result<Option<S3MultipartUpload>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT upload_id, bucket, key, content_type, metadata, initiated_at FROM s3_multipart_uploads WHERE upload_id = $1",
        &[&upload_id],
      )
      .await?;
    Ok(row.map(|r| S3MultipartUpload {
      upload_id: r.get(0),
      bucket: r.get(1),
      key: r.get(2),
      content_type: r.get(3),
      metadata: r.get(4),
      initiated_at: r.get(5),
    }))
  }

  async fn create_s3_multipart_upload(
    &self,
    upload_id: Uuid,
    bucket: &str,
    key: &str,
    content_type: Option<&str>,
    metadata: serde_json::Value,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "INSERT INTO s3_multipart_uploads (upload_id, bucket, key, content_type, metadata) VALUES ($1, $2, $3, $4, $5)",
        &[&upload_id, &bucket, &key, &content_type, &metadata],
      )
      .await?;
    Ok(())
  }

  async fn delete_s3_multipart_upload(&self, upload_id: Uuid) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "DELETE FROM s3_multipart_uploads WHERE upload_id = $1",
        &[&upload_id],
      )
      .await?;
    Ok(())
  }

  async fn list_s3_multipart_uploads(
    &self,
    bucket: &str,
    max_uploads: i32,
  ) -> Result<(Vec<S3MultipartUpload>, bool), anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT upload_id, bucket, key, content_type, metadata, initiated_at FROM s3_multipart_uploads WHERE bucket = $1 ORDER BY initiated_at LIMIT $2",
        &[&bucket, &(max_uploads + 1)],
      )
      .await?;

    let is_truncated = rows.len() > max_uploads as usize;
    let uploads: Vec<S3MultipartUpload> = rows
      .into_iter()
      .take(max_uploads as usize)
      .map(|r| S3MultipartUpload {
        upload_id: r.get(0),
        bucket: r.get(1),
        key: r.get(2),
        content_type: r.get(3),
        metadata: r.get(4),
        initiated_at: r.get(5),
      })
      .collect();

    Ok((uploads, is_truncated))
  }

  async fn get_s3_multipart_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
  ) -> Result<Option<S3Part>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT upload_id, part_number, etag, size, storage_path, created_at FROM s3_multipart_parts WHERE upload_id = $1 AND part_number = $2",
        &[&upload_id, &part_number],
      )
      .await?;
    Ok(row.map(|r| S3Part {
      upload_id: r.get(0),
      part_number: r.get(1),
      etag: r.get(2),
      size: r.get(3),
      storage_path: r.get(4),
      created_at: r.get(5),
    }))
  }

  async fn upsert_s3_multipart_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
    etag: &str,
    size: i64,
    storage_path: &str,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "INSERT INTO s3_multipart_parts (upload_id, part_number, etag, size, storage_path)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (upload_id, part_number) DO UPDATE SET etag = $3, size = $4, storage_path = $5",
        &[&upload_id, &part_number, &etag, &size, &storage_path],
      )
      .await?;
    Ok(())
  }

  async fn list_s3_multipart_parts(
    &self,
    upload_id: Uuid,
    max_parts: i32,
  ) -> Result<(Vec<S3Part>, bool), anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT upload_id, part_number, etag, size, storage_path, created_at FROM s3_multipart_parts WHERE upload_id = $1 ORDER BY part_number LIMIT $2",
        &[&upload_id, &(max_parts + 1)],
      )
      .await?;

    let is_truncated = rows.len() > max_parts as usize;
    let parts: Vec<S3Part> = rows
      .into_iter()
      .take(max_parts as usize)
      .map(|r| S3Part {
        upload_id: r.get(0),
        part_number: r.get(1),
        etag: r.get(2),
        size: r.get(3),
        storage_path: r.get(4),
        created_at: r.get(5),
      })
      .collect();

    Ok((parts, is_truncated))
  }

  // =========================================================================
  // Feature Settings Methods
  // =========================================================================

  async fn get_feature_settings(
    &self,
    name: &str,
  ) -> Result<Option<(bool, serde_json::Value)>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT enabled, settings FROM feature_settings WHERE feature_name = $1",
        &[&name],
      )
      .await?;
    Ok(row.map(|r| (r.get(0), r.get(1))))
  }

  async fn update_feature_settings(
    &self,
    name: &str,
    enabled: bool,
    settings: serde_json::Value,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "INSERT INTO feature_settings (feature_name, enabled, settings, updated_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (feature_name) DO UPDATE
         SET enabled = $2, settings = $3, updated_at = NOW()",
        &[&name, &enabled, &settings],
      )
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_schema_defines_uuid_function() {
    assert!(
      SCHEMA.contains("CREATE OR REPLACE FUNCTION uuid()"),
      "Schema must define uuid() function"
    );
    assert!(
      SCHEMA.contains("SELECT gen_random_uuid()"),
      "uuid() function must alias gen_random_uuid()"
    );
  }

  #[test]
  fn test_schema_uuid_function_defined_before_tables() {
    let uuid_fn_pos = SCHEMA
      .find("CREATE OR REPLACE FUNCTION uuid()")
      .expect("uuid() function not found");
    let documents_table_pos = SCHEMA
      .find("CREATE TABLE IF NOT EXISTS documents")
      .expect("documents table not found");
    let api_tokens_table_pos = SCHEMA
      .find("CREATE TABLE IF NOT EXISTS api_tokens")
      .expect("api_tokens table not found");

    assert!(
      uuid_fn_pos < documents_table_pos,
      "uuid() function must be defined before documents table"
    );
    assert!(
      uuid_fn_pos < api_tokens_table_pos,
      "uuid() function must be defined before api_tokens table"
    );
  }

  #[test]
  fn test_schema_documents_table_uses_uuid_default() {
    assert!(
      SCHEMA.contains("id UUID PRIMARY KEY DEFAULT uuid()"),
      "documents table must use uuid() as default for id"
    );
  }

  #[test]
  fn test_schema_api_tokens_table_uses_uuid_default() {
    let api_tokens_section = SCHEMA
      .find("CREATE TABLE IF NOT EXISTS api_tokens")
      .map(|start| &SCHEMA[start..])
      .and_then(|s| s.find(");").map(|end| &s[..end]))
      .expect("api_tokens table not found");

    assert!(
      api_tokens_section.contains("id UUID PRIMARY KEY DEFAULT uuid()"),
      "api_tokens table must use uuid() as default for id"
    );
  }

  #[test]
  fn test_schema_no_gen_random_uuid_in_table_defaults() {
    // Ensure we're using the uuid() alias, not gen_random_uuid() directly in table defaults
    let lines: Vec<&str> = SCHEMA.lines().collect();
    for line in lines {
      if line.contains("DEFAULT gen_random_uuid()") {
        panic!(
          "Table defaults should use uuid() alias, not gen_random_uuid() directly: {}",
          line
        );
      }
    }
  }
}
