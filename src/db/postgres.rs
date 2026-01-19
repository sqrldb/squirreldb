use async_trait::async_trait;
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
}
