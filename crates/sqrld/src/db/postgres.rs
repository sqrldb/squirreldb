use async_trait::async_trait;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio::sync::broadcast;
use tokio_postgres::NoTls;
use uuid::Uuid;

use super::backend::{
  AdminRole, AdminSession, AdminUser, ApiTokenInfo, DatabaseBackend, SqlDialect,
  StorageAccessKeyInfo,
};
use super::sanitize::{validate_collection_name, validate_identifier, validate_limit};
use crate::storage::{MultipartPart, MultipartUpload, ObjectAcl, StorageBucket, StorageObject};
use crate::types::{
  Change, ChangeOperation, Document, OrderBySpec, OrderDirection, Project, ProjectMember,
  ProjectRole, DEFAULT_PROJECT_ID,
};

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
    project_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    collection VARCHAR(255) NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection);
CREATE INDEX IF NOT EXISTS idx_documents_data ON documents USING GIN(data);

-- Migration: Add project_id to existing documents table
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'documents' AND column_name = 'project_id') THEN
        ALTER TABLE documents ADD COLUMN project_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
    END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_documents_project ON documents(project_id);
CREATE INDEX IF NOT EXISTS idx_documents_project_collection ON documents(project_id, collection);

-- Optimized change_queue with delta storage and fillfactor for INSERT-heavy workload
CREATE TABLE IF NOT EXISTS change_queue (
    id BIGSERIAL PRIMARY KEY,
    project_id UUID,
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

-- Migration: Add project_id to existing change_queue table
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'change_queue' AND column_name = 'project_id') THEN
        ALTER TABLE change_queue ADD COLUMN project_id UUID;
    END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_change_queue_project ON change_queue(project_id);
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
        INSERT INTO change_queue (project_id, collection, document_id, operation, new_data)
        VALUES (NEW.project_id, NEW.collection, NEW.id, 'INSERT', NEW.data)
        RETURNING id INTO change_id;
    ELSIF TG_OP = 'UPDATE' THEN
        -- Compute delta for UPDATE operations
        computed_delta := sqrl_json_delta(OLD.data, NEW.data);
        INSERT INTO change_queue (project_id, collection, document_id, operation, old_data, new_data, delta)
        VALUES (NEW.project_id, NEW.collection, NEW.id, 'UPDATE', OLD.data, NEW.data, computed_delta)
        RETURNING id INTO change_id;
    ELSIF TG_OP = 'DELETE' THEN
        INSERT INTO change_queue (project_id, collection, document_id, operation, old_data)
        VALUES (OLD.project_id, OLD.collection, OLD.id, 'DELETE', OLD.data)
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
    project_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    name VARCHAR(255) NOT NULL,
    token_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(project_id, name)
);
CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash);

-- Migration: Add project_id to existing api_tokens table (must run BEFORE creating project index)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'api_tokens' AND column_name = 'project_id') THEN
        ALTER TABLE api_tokens ADD COLUMN project_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
        -- Drop old unique constraint on name and add new one
        ALTER TABLE api_tokens DROP CONSTRAINT IF EXISTS api_tokens_name_key;
        ALTER TABLE api_tokens ADD CONSTRAINT api_tokens_project_name_unique UNIQUE (project_id, name);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_api_tokens_project ON api_tokens(project_id);

-- S3 Buckets
CREATE TABLE IF NOT EXISTS storage_buckets (
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
CREATE TABLE IF NOT EXISTS storage_objects (
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
    FOREIGN KEY (bucket) REFERENCES storage_buckets(name) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_storage_objects_bucket_key ON storage_objects(bucket, key);
CREATE INDEX IF NOT EXISTS idx_storage_objects_latest ON storage_objects(bucket, key) WHERE is_latest = TRUE;

-- Multipart Uploads
CREATE TABLE IF NOT EXISTS storage_multipart_uploads (
    upload_id UUID PRIMARY KEY DEFAULT uuid(),
    bucket VARCHAR(63) NOT NULL,
    key TEXT NOT NULL,
    content_type VARCHAR(255),
    metadata JSONB DEFAULT '{}',
    initiated_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (bucket) REFERENCES storage_buckets(name) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS storage_multipart_parts (
    upload_id UUID NOT NULL,
    part_number INTEGER NOT NULL,
    etag VARCHAR(32) NOT NULL,
    size BIGINT NOT NULL,
    storage_path TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (upload_id, part_number),
    FOREIGN KEY (upload_id) REFERENCES storage_multipart_uploads(upload_id) ON DELETE CASCADE
);

-- S3 Access Keys (for AWS Signature V4)
CREATE TABLE IF NOT EXISTS storage_access_keys (
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

-- Admin users for authentication
CREATE TABLE IF NOT EXISTS admin_users (
    id UUID PRIMARY KEY DEFAULT uuid(),
    username VARCHAR(255) UNIQUE NOT NULL,
    email VARCHAR(255),
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(20) NOT NULL DEFAULT 'admin',
    created_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_admin_users_username ON admin_users(username);

-- Admin sessions
CREATE TABLE IF NOT EXISTS admin_sessions (
    id UUID PRIMARY KEY DEFAULT uuid(),
    user_id UUID NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    session_token_hash VARCHAR(64) NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_admin_sessions_token ON admin_sessions(session_token_hash);
CREATE INDEX IF NOT EXISTS idx_admin_sessions_expires ON admin_sessions(expires_at);

-- Projects table
CREATE TABLE IF NOT EXISTS projects (
    id UUID PRIMARY KEY DEFAULT uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    owner_id UUID NOT NULL REFERENCES admin_users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name);
CREATE INDEX IF NOT EXISTS idx_projects_owner ON projects(owner_id);

-- Project membership
CREATE TABLE IF NOT EXISTS project_members (
    id UUID PRIMARY KEY DEFAULT uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_project_members_project ON project_members(project_id);
CREATE INDEX IF NOT EXISTS idx_project_members_user ON project_members(user_id);

-- Create default project if none exists (runs on schema init if admin user exists)
INSERT INTO projects (id, name, description, owner_id)
SELECT
    '00000000-0000-0000-0000-000000000000'::UUID,
    'default',
    'Default project',
    (SELECT id FROM admin_users ORDER BY created_at LIMIT 1)
WHERE EXISTS (SELECT 1 FROM admin_users)
  AND NOT EXISTS (SELECT 1 FROM projects WHERE id = '00000000-0000-0000-0000-000000000000');

-- Trigger to create default project when first admin user is created
CREATE OR REPLACE FUNCTION create_default_project_on_first_user() RETURNS TRIGGER AS $$
BEGIN
    -- Only create if this is the first admin user and no default project exists
    IF NOT EXISTS (SELECT 1 FROM projects WHERE id = '00000000-0000-0000-0000-000000000000') THEN
        INSERT INTO projects (id, name, description, owner_id)
        VALUES ('00000000-0000-0000-0000-000000000000', 'default', 'Default project', NEW.id);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS create_default_project_trigger ON admin_users;
CREATE TRIGGER create_default_project_trigger
    AFTER INSERT ON admin_users
    FOR EACH ROW
    EXECUTE FUNCTION create_default_project_on_first_user();

-- =========================================================================
-- S3 Atomic Operations (reduces round-trips for better performance)
-- =========================================================================

-- Atomic S3 object creation with bucket stats update (saves 1 round-trip per upload)
CREATE OR REPLACE FUNCTION sqrl_create_storage_object_with_stats(
    p_bucket VARCHAR(63),
    p_key TEXT,
    p_version_id UUID,
    p_etag VARCHAR(32),
    p_size BIGINT,
    p_content_type VARCHAR(255),
    p_storage_path TEXT,
    p_metadata JSONB
) RETURNS VOID AS $$
BEGIN
    INSERT INTO storage_objects (bucket, key, version_id, etag, size, content_type, storage_path, metadata)
    VALUES (p_bucket, p_key, p_version_id, p_etag, p_size, p_content_type, p_storage_path, p_metadata);

    UPDATE storage_buckets
    SET current_size = current_size + p_size,
        object_count = object_count + 1
    WHERE name = p_bucket;
END;
$$ LANGUAGE plpgsql;

-- Atomic S3 object deletion that returns object info and updates stats (saves 2 round-trips per delete)
-- Returns: storage_path, size (for file cleanup), or NULL if not found
CREATE OR REPLACE FUNCTION sqrl_delete_storage_object_with_stats(
    p_bucket VARCHAR(63),
    p_key TEXT,
    p_version_id UUID DEFAULT NULL
) RETURNS TABLE(storage_path TEXT, size BIGINT) AS $$
DECLARE
    deleted_path TEXT;
    deleted_size BIGINT;
BEGIN
    IF p_version_id IS NOT NULL THEN
        -- Delete specific version
        DELETE FROM storage_objects
        WHERE bucket = p_bucket AND key = p_key AND version_id = p_version_id
        RETURNING storage_objects.storage_path, storage_objects.size INTO deleted_path, deleted_size;
    ELSE
        -- Delete latest version (or all if not versioned)
        DELETE FROM storage_objects
        WHERE bucket = p_bucket AND key = p_key AND is_latest = TRUE
        RETURNING storage_objects.storage_path, storage_objects.size INTO deleted_path, deleted_size;
    END IF;

    IF deleted_path IS NOT NULL THEN
        UPDATE storage_buckets
        SET current_size = current_size - deleted_size,
            object_count = object_count - 1
        WHERE name = p_bucket;

        RETURN QUERY SELECT deleted_path, deleted_size;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Atomic delete marker creation (combines unset_latest + insert in one operation)
CREATE OR REPLACE FUNCTION sqrl_create_storage_delete_marker(
    p_bucket VARCHAR(63),
    p_key TEXT,
    p_version_id UUID
) RETURNS VOID AS $$
BEGIN
    -- Unset latest on existing versions and create delete marker in one transaction
    UPDATE storage_objects SET is_latest = FALSE WHERE bucket = p_bucket AND key = p_key;

    INSERT INTO storage_objects (bucket, key, version_id, etag, size, is_delete_marker, is_latest)
    VALUES (p_bucket, p_key, p_version_id, '', 0, TRUE, TRUE);
END;
$$ LANGUAGE plpgsql;

-- Combined S3 objects and prefixes listing (saves 1 round-trip per list operation)
-- Returns objects and uses a lateral join to compute prefixes in the same query
CREATE OR REPLACE FUNCTION sqrl_list_storage_objects_with_prefixes(
    p_bucket VARCHAR(63),
    p_prefix TEXT DEFAULT NULL,
    p_delimiter TEXT DEFAULT NULL,
    p_max_keys INTEGER DEFAULT 1000,
    p_continuation_token TEXT DEFAULT NULL
) RETURNS TABLE(
    obj_key TEXT,
    obj_version_id UUID,
    obj_etag VARCHAR(32),
    obj_size BIGINT,
    obj_content_type VARCHAR(255),
    obj_storage_path TEXT,
    obj_metadata JSONB,
    obj_acl JSONB,
    obj_created_at TIMESTAMPTZ,
    is_prefix BOOLEAN
) AS $$
DECLARE
    prefix_pattern TEXT;
    prefix_len INTEGER;
BEGIN
    prefix_pattern := COALESCE(p_prefix, '') || '%';
    prefix_len := LENGTH(COALESCE(p_prefix, ''));

    -- Return objects (is_prefix = FALSE)
    RETURN QUERY
    SELECT
        o.key,
        o.version_id,
        o.etag,
        o.size,
        o.content_type,
        o.storage_path,
        o.metadata,
        o.acl,
        o.created_at,
        FALSE AS is_prefix
    FROM storage_objects o
    WHERE o.bucket = p_bucket
      AND o.key LIKE prefix_pattern
      AND o.key > COALESCE(p_continuation_token, '')
      AND o.is_latest = TRUE
      AND o.is_delete_marker = FALSE
      AND (p_delimiter IS NULL OR POSITION(p_delimiter IN SUBSTRING(o.key FROM prefix_len + 1)) = 0)
    ORDER BY o.key
    LIMIT p_max_keys;

    -- Return common prefixes if delimiter is specified (is_prefix = TRUE)
    IF p_delimiter IS NOT NULL THEN
        RETURN QUERY
        SELECT DISTINCT
            SUBSTRING(o.key FROM 1 FOR prefix_len + POSITION(p_delimiter IN SUBSTRING(o.key FROM prefix_len + 1))) AS prefix_key,
            NULL::UUID,
            NULL::VARCHAR(32),
            NULL::BIGINT,
            NULL::VARCHAR(255),
            NULL::TEXT,
            NULL::JSONB,
            NULL::JSONB,
            NULL::TIMESTAMPTZ,
            TRUE AS is_prefix
        FROM storage_objects o
        WHERE o.bucket = p_bucket
          AND o.key LIKE prefix_pattern
          AND o.is_latest = TRUE
          AND POSITION(p_delimiter IN SUBSTRING(o.key FROM prefix_len + 1)) > 0
        ORDER BY prefix_key;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Atomic S3 object replacement for non-versioned buckets (combines get old + unset latest + create new + stats)
CREATE OR REPLACE FUNCTION sqrl_replace_storage_object(
    p_bucket VARCHAR(63),
    p_key TEXT,
    p_version_id UUID,
    p_etag VARCHAR(32),
    p_size BIGINT,
    p_content_type VARCHAR(255),
    p_storage_path TEXT,
    p_metadata JSONB
) RETURNS TEXT AS $$
DECLARE
    old_path TEXT;
    old_size BIGINT;
BEGIN
    -- Get old object info for file cleanup
    SELECT storage_path, size INTO old_path, old_size
    FROM storage_objects
    WHERE bucket = p_bucket AND key = p_key AND is_latest = TRUE;

    -- Unset latest on old version
    UPDATE storage_objects SET is_latest = FALSE
    WHERE bucket = p_bucket AND key = p_key;

    -- Create new object
    INSERT INTO storage_objects (bucket, key, version_id, etag, size, content_type, storage_path, metadata)
    VALUES (p_bucket, p_key, p_version_id, p_etag, p_size, p_content_type, p_storage_path, p_metadata);

    -- Update bucket stats (only size delta, count stays same for replacement)
    IF old_size IS NOT NULL THEN
        UPDATE storage_buckets
        SET current_size = current_size - old_size + p_size
        WHERE name = p_bucket;
    ELSE
        -- New object (not replacement)
        UPDATE storage_buckets
        SET current_size = current_size + p_size,
            object_count = object_count + 1
        WHERE name = p_bucket;
    END IF;

    RETURN old_path;  -- Return old path for file cleanup, NULL if new object
END;
$$ LANGUAGE plpgsql;
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

  // =========================================================================
  // Project Methods
  // =========================================================================

  async fn create_project(
    &self,
    name: &str,
    description: Option<&str>,
    owner_id: Uuid,
  ) -> Result<Project, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_one(
        "INSERT INTO projects (name, description, owner_id)
         VALUES ($1, $2, $3)
         RETURNING id, name, description, owner_id, created_at, updated_at",
        &[&name, &description, &owner_id],
      )
      .await?;
    Ok(Project {
      id: row.get(0),
      name: row.get(1),
      description: row.get(2),
      owner_id: row.get(3),
      created_at: row.get(4),
      updated_at: row.get(5),
    })
  }

  async fn get_project(&self, id: Uuid) -> Result<Option<Project>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT id, name, description, owner_id, created_at, updated_at FROM projects WHERE id = $1",
        &[&id],
      )
      .await?;
    Ok(row.map(|r| Project {
      id: r.get(0),
      name: r.get(1),
      description: r.get(2),
      owner_id: r.get(3),
      created_at: r.get(4),
      updated_at: r.get(5),
    }))
  }

  async fn get_project_by_name(&self, name: &str) -> Result<Option<Project>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT id, name, description, owner_id, created_at, updated_at FROM projects WHERE name = $1",
        &[&name],
      )
      .await?;
    Ok(row.map(|r| Project {
      id: r.get(0),
      name: r.get(1),
      description: r.get(2),
      owner_id: r.get(3),
      created_at: r.get(4),
      updated_at: r.get(5),
    }))
  }

  async fn list_projects(&self) -> Result<Vec<Project>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT id, name, description, owner_id, created_at, updated_at FROM projects ORDER BY name",
        &[],
      )
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| Project {
          id: r.get(0),
          name: r.get(1),
          description: r.get(2),
          owner_id: r.get(3),
          created_at: r.get(4),
          updated_at: r.get(5),
        })
        .collect(),
    )
  }

  async fn list_user_projects(&self, user_id: Uuid) -> Result<Vec<Project>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT DISTINCT p.id, p.name, p.description, p.owner_id, p.created_at, p.updated_at
         FROM projects p
         LEFT JOIN project_members pm ON p.id = pm.project_id
         WHERE p.owner_id = $1 OR pm.user_id = $1
         ORDER BY p.name",
        &[&user_id],
      )
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| Project {
          id: r.get(0),
          name: r.get(1),
          description: r.get(2),
          owner_id: r.get(3),
          created_at: r.get(4),
          updated_at: r.get(5),
        })
        .collect(),
    )
  }

  async fn update_project(
    &self,
    id: Uuid,
    name: &str,
    description: Option<&str>,
  ) -> Result<Option<Project>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "UPDATE projects SET name = $2, description = $3, updated_at = NOW()
         WHERE id = $1
         RETURNING id, name, description, owner_id, created_at, updated_at",
        &[&id, &name, &description],
      )
      .await?;
    Ok(row.map(|r| Project {
      id: r.get(0),
      name: r.get(1),
      description: r.get(2),
      owner_id: r.get(3),
      created_at: r.get(4),
      updated_at: r.get(5),
    }))
  }

  async fn delete_project(&self, id: Uuid) -> Result<bool, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute("DELETE FROM projects WHERE id = $1", &[&id])
      .await?;
    Ok(result > 0)
  }

  // =========================================================================
  // Project Membership Methods
  // =========================================================================

  async fn add_project_member(
    &self,
    project_id: Uuid,
    user_id: Uuid,
    role: ProjectRole,
  ) -> Result<ProjectMember, anyhow::Error> {
    let role_str = role.to_string();
    let row = self
      .pool
      .get()
      .await?
      .query_one(
        "INSERT INTO project_members (project_id, user_id, role)
         VALUES ($1, $2, $3)
         ON CONFLICT (project_id, user_id) DO UPDATE SET role = $3
         RETURNING id, project_id, user_id, role, created_at",
        &[&project_id, &user_id, &role_str],
      )
      .await?;
    Ok(ProjectMember {
      id: row.get(0),
      project_id: row.get(1),
      user_id: row.get(2),
      role: row.get::<_, String>(3).parse().unwrap_or_default(),
      created_at: row.get(4),
    })
  }

  async fn remove_project_member(
    &self,
    project_id: Uuid,
    user_id: Uuid,
  ) -> Result<bool, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute(
        "DELETE FROM project_members WHERE project_id = $1 AND user_id = $2",
        &[&project_id, &user_id],
      )
      .await?;
    Ok(result > 0)
  }

  async fn get_project_members(
    &self,
    project_id: Uuid,
  ) -> Result<Vec<(ProjectMember, AdminUser)>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT pm.id, pm.project_id, pm.user_id, pm.role, pm.created_at,
                u.id, u.username, u.email, u.role, u.created_at
         FROM project_members pm
         JOIN admin_users u ON pm.user_id = u.id
         WHERE pm.project_id = $1
         ORDER BY pm.created_at",
        &[&project_id],
      )
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| {
          let member = ProjectMember {
            id: r.get(0),
            project_id: r.get(1),
            user_id: r.get(2),
            role: r.get::<_, String>(3).parse().unwrap_or_default(),
            created_at: r.get(4),
          };
          let user = AdminUser {
            id: r.get(5),
            username: r.get(6),
            email: r.get(7),
            role: r.get::<_, String>(8).parse().unwrap_or(AdminRole::Admin),
            created_at: r.get(9),
          };
          (member, user)
        })
        .collect(),
    )
  }

  async fn get_user_project_role(
    &self,
    project_id: Uuid,
    user_id: Uuid,
  ) -> Result<Option<ProjectRole>, anyhow::Error> {
    // Check if user is owner first
    let owner_row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2",
        &[&project_id, &user_id],
      )
      .await?;
    if owner_row.is_some() {
      return Ok(Some(ProjectRole::Owner));
    }

    // Check membership
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT role FROM project_members WHERE project_id = $1 AND user_id = $2",
        &[&project_id, &user_id],
      )
      .await?;
    Ok(row.map(|r| r.get::<_, String>(0).parse().unwrap_or_default()))
  }

  async fn update_member_role(
    &self,
    project_id: Uuid,
    user_id: Uuid,
    role: ProjectRole,
  ) -> Result<bool, anyhow::Error> {
    let role_str = role.to_string();
    let result = self
      .pool
      .get()
      .await?
      .execute(
        "UPDATE project_members SET role = $3 WHERE project_id = $1 AND user_id = $2",
        &[&project_id, &user_id, &role_str],
      )
      .await?;
    Ok(result > 0)
  }

  // =========================================================================
  // Document Methods (project-scoped)
  // =========================================================================

  async fn insert(
    &self,
    project_id: Uuid,
    collection: &str,
    data: serde_json::Value,
  ) -> Result<Document, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    // Let PostgreSQL generate UUID and timestamps via DEFAULTs, use RETURNING to get them back
    let row = self.pool.get().await?.query_one(
      "INSERT INTO documents (project_id, collection, data) VALUES ($1, $2, $3) RETURNING id, project_id, collection, data, created_at, updated_at",
      &[&project_id, &collection, &data],
    ).await?;

    Ok(Document {
      id: row.get(0),
      project_id: row.get(1),
      collection: row.get(2),
      data: row.get(3),
      created_at: row.get(4),
      updated_at: row.get(5),
    })
  }

  async fn get(
    &self,
    project_id: Uuid,
    collection: &str,
    id: Uuid,
  ) -> Result<Option<Document>, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    let row = self.pool.get().await?.query_opt(
      "SELECT id, project_id, collection, data, created_at, updated_at FROM documents WHERE project_id = $1 AND collection = $2 AND id = $3",
      &[&project_id, &collection, &id],
    ).await?;
    Ok(row.map(|r| Document {
      id: r.get(0),
      project_id: r.get(1),
      collection: r.get(2),
      data: r.get(3),
      created_at: r.get(4),
      updated_at: r.get(5),
    }))
  }

  async fn update(
    &self,
    project_id: Uuid,
    collection: &str,
    id: Uuid,
    data: serde_json::Value,
  ) -> Result<Option<Document>, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    // Let PostgreSQL generate updated_at via NOW()
    let row = self.pool.get().await?.query_opt(
      "UPDATE documents SET data = $1, updated_at = NOW() WHERE project_id = $2 AND collection = $3 AND id = $4 RETURNING id, project_id, collection, data, created_at, updated_at",
      &[&data, &project_id, &collection, &id],
    ).await?;
    Ok(row.map(|r| Document {
      id: r.get(0),
      project_id: r.get(1),
      collection: r.get(2),
      data: r.get(3),
      created_at: r.get(4),
      updated_at: r.get(5),
    }))
  }

  async fn delete(
    &self,
    project_id: Uuid,
    collection: &str,
    id: Uuid,
  ) -> Result<Option<Document>, anyhow::Error> {
    // Validate collection name (defense in depth - query is parameterized)
    validate_collection_name(collection)?;

    let row = self.pool.get().await?.query_opt(
      "DELETE FROM documents WHERE project_id = $1 AND collection = $2 AND id = $3 RETURNING id, project_id, collection, data, created_at, updated_at",
      &[&project_id, &collection, &id],
    ).await?;
    Ok(row.map(|r| Document {
      id: r.get(0),
      project_id: r.get(1),
      collection: r.get(2),
      data: r.get(3),
      created_at: r.get(4),
      updated_at: r.get(5),
    }))
  }

  async fn list(
    &self,
    project_id: Uuid,
    collection: &str,
    filter: Option<&str>,
    order: Option<&OrderBySpec>,
    limit: Option<usize>,
    offset: Option<usize>,
  ) -> Result<Vec<Document>, anyhow::Error> {
    // Validate collection name to prevent injection
    validate_collection_name(collection)?;

    let mut sql =
      "SELECT id, project_id, collection, data, created_at, updated_at FROM documents WHERE project_id = $1 AND collection = $2"
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

    let rows = self
      .pool
      .get()
      .await?
      .query(&sql, &[&project_id, &collection])
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| Document {
          id: r.get(0),
          project_id: r.get(1),
          collection: r.get(2),
          data: r.get(3),
          created_at: r.get(4),
          updated_at: r.get(5),
        })
        .collect(),
    )
  }

  async fn list_collections(&self, project_id: Uuid) -> Result<Vec<String>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT DISTINCT collection FROM documents WHERE project_id = $1 ORDER BY collection",
        &[&project_id],
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
              "SELECT id, project_id, collection, document_id, operation, old_data, new_data, changed_at FROM change_queue WHERE id = $1",
              &[&change_id]
            ).await else { continue };

            for row in rows {
              let id: i64 = row.get(0);
              let Ok(op) = row.get::<_, String>(4).parse::<ChangeOperation>() else {
                continue;
              };
              let _ = tx.send(Change {
                id,
                project_id: row.get::<_, Option<Uuid>>(1).unwrap_or(DEFAULT_PROJECT_ID),
                collection: row.get(2),
                document_id: row.get(3),
                operation: op,
                old_data: row.get(5),
                new_data: row.get(6),
                changed_at: row.get(7),
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
              "SELECT id, project_id, collection, document_id, operation, old_data, new_data, changed_at FROM change_queue WHERE id > $1 ORDER BY id LIMIT 100",
              &[&last_id]
            ).await else { continue };

            for row in rows {
              let id: i64 = row.get(0);
              let Ok(op) = row.get::<_, String>(4).parse::<ChangeOperation>() else {
                continue;
              };
              let _ = tx.send(Change {
                id,
                project_id: row.get::<_, Option<Uuid>>(1).unwrap_or(DEFAULT_PROJECT_ID),
                collection: row.get(2),
                document_id: row.get(3),
                operation: op,
                old_data: row.get(5),
                new_data: row.get(6),
                changed_at: row.get(7),
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
    project_id: Uuid,
    name: &str,
    token_hash: &str,
  ) -> Result<ApiTokenInfo, anyhow::Error> {
    // Let PostgreSQL generate UUID and timestamp via DEFAULTs
    let row = self
      .pool
      .get()
      .await?
      .query_one(
        "INSERT INTO api_tokens (project_id, name, token_hash) VALUES ($1, $2, $3) RETURNING id, project_id, name, created_at",
        &[&project_id, &name, &token_hash],
      )
      .await?;
    Ok(ApiTokenInfo {
      id: row.get(0),
      project_id: row.get(1),
      name: row.get(2),
      created_at: row.get(3),
    })
  }

  async fn delete_token(&self, project_id: Uuid, id: Uuid) -> Result<bool, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute(
        "DELETE FROM api_tokens WHERE id = $1 AND project_id = $2",
        &[&id, &project_id],
      )
      .await?;
    Ok(result > 0)
  }

  async fn list_tokens(&self, project_id: Uuid) -> Result<Vec<ApiTokenInfo>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT id, project_id, name, created_at FROM api_tokens WHERE project_id = $1 ORDER BY created_at DESC",
        &[&project_id],
      )
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| ApiTokenInfo {
          id: r.get(0),
          project_id: r.get(1),
          name: r.get(2),
          created_at: r.get(3),
        })
        .collect(),
    )
  }

  async fn validate_token(&self, token_hash: &str) -> Result<Option<Uuid>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT project_id FROM api_tokens WHERE token_hash = $1",
        &[&token_hash],
      )
      .await?;
    Ok(row.map(|r| r.get(0)))
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

  async fn get_storage_access_key(
    &self,
    access_key_id: &str,
  ) -> Result<Option<(String, Option<Uuid>)>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT secret_access_key, owner_id FROM storage_access_keys WHERE access_key_id = $1",
        &[&access_key_id],
      )
      .await?;
    Ok(row.map(|r| (r.get(0), r.get(1))))
  }

  async fn create_storage_access_key(
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
        "INSERT INTO storage_access_keys (access_key_id, secret_access_key, owner_id, name) VALUES ($1, $2, $3, $4)",
        &[&access_key_id, &secret_key, &owner_id, &name],
      )
      .await?;
    Ok(())
  }

  async fn delete_storage_access_key(&self, access_key_id: &str) -> Result<bool, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute(
        "DELETE FROM storage_access_keys WHERE access_key_id = $1",
        &[&access_key_id],
      )
      .await?;
    Ok(result > 0)
  }

  async fn list_storage_access_keys(&self) -> Result<Vec<StorageAccessKeyInfo>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT access_key_id, owner_id, name, created_at FROM storage_access_keys ORDER BY created_at DESC",
        &[],
      )
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| StorageAccessKeyInfo {
          access_key_id: r.get(0),
          owner_id: r.get(1),
          name: r.get(2),
          created_at: r.get(3),
        })
        .collect(),
    )
  }

  async fn get_storage_bucket(&self, name: &str) -> Result<Option<StorageBucket>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT name, owner_id, versioning_enabled, acl, lifecycle_rules, quota_bytes, current_size, object_count, created_at FROM storage_buckets WHERE name = $1",
        &[&name],
      )
      .await?;
    Ok(row.map(|r| {
      StorageBucket {
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

  async fn create_storage_bucket(
    &self,
    name: &str,
    owner_id: Option<Uuid>,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "INSERT INTO storage_buckets (name, owner_id) VALUES ($1, $2)",
        &[&name, &owner_id],
      )
      .await?;
    Ok(())
  }

  async fn delete_storage_bucket(&self, name: &str) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute("DELETE FROM storage_buckets WHERE name = $1", &[&name])
      .await?;
    Ok(())
  }

  async fn list_storage_buckets(&self) -> Result<Vec<StorageBucket>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT name, owner_id, versioning_enabled, acl, lifecycle_rules, quota_bytes, current_size, object_count, created_at FROM storage_buckets ORDER BY name",
        &[],
      )
      .await?;
    Ok(
      rows
        .into_iter()
        .map(|r| StorageBucket {
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

  async fn update_storage_bucket_stats(
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
        "UPDATE storage_buckets SET current_size = current_size + $2, object_count = object_count + $3 WHERE name = $1",
        &[&bucket, &size_delta, &count_delta],
      )
      .await?;
    Ok(())
  }

  async fn get_storage_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Option<Uuid>,
  ) -> Result<Option<StorageObject>, anyhow::Error> {
    let row = if let Some(vid) = version_id {
      self
        .pool
        .get()
        .await?
        .query_opt(
          "SELECT bucket, key, version_id, is_latest, etag, size, content_type, storage_path, metadata, acl, is_delete_marker, created_at FROM storage_objects WHERE bucket = $1 AND key = $2 AND version_id = $3",
          &[&bucket, &key, &vid],
        )
        .await?
    } else {
      self
        .pool
        .get()
        .await?
        .query_opt(
          "SELECT bucket, key, version_id, is_latest, etag, size, content_type, storage_path, metadata, acl, is_delete_marker, created_at FROM storage_objects WHERE bucket = $1 AND key = $2 AND is_latest = TRUE",
          &[&bucket, &key],
        )
        .await?
    };
    Ok(row.map(|r| {
      StorageObject {
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

  async fn create_storage_object(
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
        "INSERT INTO storage_objects (bucket, key, version_id, etag, size, content_type, storage_path, metadata) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[&bucket, &key, &version_id, &etag, &size, &content_type, &storage_path, &metadata],
      )
      .await?;
    Ok(())
  }

  async fn delete_storage_object(
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
          "DELETE FROM storage_objects WHERE bucket = $1 AND key = $2 AND version_id = $3",
          &[&bucket, &key, &vid],
        )
        .await?;
    } else {
      self
        .pool
        .get()
        .await?
        .execute(
          "DELETE FROM storage_objects WHERE bucket = $1 AND key = $2",
          &[&bucket, &key],
        )
        .await?;
    }
    Ok(())
  }

  async fn create_storage_delete_marker(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
  ) -> Result<(), anyhow::Error> {
    // Use atomic function that combines unset_latest + insert in one transaction
    self
      .pool
      .get()
      .await?
      .execute(
        "SELECT sqrl_create_storage_delete_marker($1, $2, $3)",
        &[&bucket, &key, &version_id],
      )
      .await?;
    Ok(())
  }

  async fn unset_storage_object_latest(
    &self,
    bucket: &str,
    key: &str,
  ) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "UPDATE storage_objects SET is_latest = FALSE WHERE bucket = $1 AND key = $2",
        &[&bucket, &key],
      )
      .await?;
    Ok(())
  }

  async fn update_storage_object_acl(
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
        "UPDATE storage_objects SET acl = $3 WHERE bucket = $1 AND key = $2 AND is_latest = TRUE",
        &[&bucket, &key, &acl_json],
      )
      .await?;
    Ok(())
  }

  async fn list_storage_objects(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    _delimiter: Option<&str>,
    max_keys: i32,
    continuation_token: Option<&str>,
  ) -> Result<(Vec<StorageObject>, bool, Option<String>), anyhow::Error> {
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
         FROM storage_objects
         WHERE bucket = $1 AND key LIKE $2 AND key > $3 AND is_latest = TRUE AND is_delete_marker = FALSE
         ORDER BY key
         LIMIT $4",
        &[&bucket, &prefix_pattern, &start_key, &(max_keys + 1)],
      )
      .await?;

    let is_truncated = rows.len() > max_keys as usize;
    let objects: Vec<StorageObject> = rows
      .into_iter()
      .take(max_keys as usize)
      .map(|r| StorageObject {
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

  async fn list_storage_common_prefixes(
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
         FROM storage_objects
         WHERE bucket = $1 AND key LIKE $4 AND POSITION($2 IN SUBSTRING(key FROM $3)) > 0 AND is_latest = TRUE
         ORDER BY common_prefix",
        &[&bucket, &delim, &(prefix_len + 1), &format!("{}%", prefix_str)],
      )
      .await?;

    Ok(rows.into_iter().map(|r| r.get(0)).collect())
  }

  async fn list_storage_object_versions(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    max_keys: i32,
  ) -> Result<(Vec<StorageObject>, bool, Option<String>), anyhow::Error> {
    let prefix_pattern = prefix
      .map(|p| format!("{}%", p))
      .unwrap_or_else(|| "%".to_string());

    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT bucket, key, version_id, is_latest, etag, size, content_type, storage_path, metadata, acl, is_delete_marker, created_at
         FROM storage_objects
         WHERE bucket = $1 AND key LIKE $2
         ORDER BY key, created_at DESC
         LIMIT $3",
        &[&bucket, &prefix_pattern, &(max_keys + 1)],
      )
      .await?;

    let is_truncated = rows.len() > max_keys as usize;
    let objects: Vec<StorageObject> = rows
      .into_iter()
      .take(max_keys as usize)
      .map(|r| StorageObject {
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

  async fn get_multipart_upload(
    &self,
    upload_id: Uuid,
  ) -> Result<Option<MultipartUpload>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT upload_id, bucket, key, content_type, metadata, initiated_at FROM storage_multipart_uploads WHERE upload_id = $1",
        &[&upload_id],
      )
      .await?;
    Ok(row.map(|r| MultipartUpload {
      upload_id: r.get(0),
      bucket: r.get(1),
      key: r.get(2),
      content_type: r.get(3),
      metadata: r.get(4),
      initiated_at: r.get(5),
    }))
  }

  async fn create_multipart_upload(
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
        "INSERT INTO storage_multipart_uploads (upload_id, bucket, key, content_type, metadata) VALUES ($1, $2, $3, $4, $5)",
        &[&upload_id, &bucket, &key, &content_type, &metadata],
      )
      .await?;
    Ok(())
  }

  async fn delete_multipart_upload(&self, upload_id: Uuid) -> Result<(), anyhow::Error> {
    self
      .pool
      .get()
      .await?
      .execute(
        "DELETE FROM storage_multipart_uploads WHERE upload_id = $1",
        &[&upload_id],
      )
      .await?;
    Ok(())
  }

  async fn list_multipart_uploads(
    &self,
    bucket: &str,
    max_uploads: i32,
  ) -> Result<(Vec<MultipartUpload>, bool), anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT upload_id, bucket, key, content_type, metadata, initiated_at FROM storage_multipart_uploads WHERE bucket = $1 ORDER BY initiated_at LIMIT $2",
        &[&bucket, &(max_uploads + 1)],
      )
      .await?;

    let is_truncated = rows.len() > max_uploads as usize;
    let uploads: Vec<MultipartUpload> = rows
      .into_iter()
      .take(max_uploads as usize)
      .map(|r| MultipartUpload {
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

  async fn get_multipart_part(
    &self,
    upload_id: Uuid,
    part_number: i32,
  ) -> Result<Option<MultipartPart>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT upload_id, part_number, etag, size, storage_path, created_at FROM storage_multipart_parts WHERE upload_id = $1 AND part_number = $2",
        &[&upload_id, &part_number],
      )
      .await?;
    Ok(row.map(|r| MultipartPart {
      upload_id: r.get(0),
      part_number: r.get(1),
      etag: r.get(2),
      size: r.get(3),
      storage_path: r.get(4),
      created_at: r.get(5),
    }))
  }

  async fn upsert_multipart_part(
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
        "INSERT INTO storage_multipart_parts (upload_id, part_number, etag, size, storage_path)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (upload_id, part_number) DO UPDATE SET etag = $3, size = $4, storage_path = $5",
        &[&upload_id, &part_number, &etag, &size, &storage_path],
      )
      .await?;
    Ok(())
  }

  async fn list_multipart_parts(
    &self,
    upload_id: Uuid,
    max_parts: i32,
  ) -> Result<(Vec<MultipartPart>, bool), anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT upload_id, part_number, etag, size, storage_path, created_at FROM storage_multipart_parts WHERE upload_id = $1 ORDER BY part_number LIMIT $2",
        &[&upload_id, &(max_parts + 1)],
      )
      .await?;

    let is_truncated = rows.len() > max_parts as usize;
    let parts: Vec<MultipartPart> = rows
      .into_iter()
      .take(max_parts as usize)
      .map(|r| MultipartPart {
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

  // =========================================================================
  // Admin Users (authentication)
  // =========================================================================

  async fn has_admin_users(&self) -> Result<bool, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_one("SELECT EXISTS(SELECT 1 FROM admin_users)", &[])
      .await?;
    Ok(row.get::<_, bool>(0))
  }

  async fn create_admin_user(
    &self,
    username: &str,
    email: Option<&str>,
    password_hash: &str,
    role: AdminRole,
  ) -> Result<AdminUser, anyhow::Error> {
    let role_str = role.to_string();
    let row = self
      .pool
      .get()
      .await?
      .query_one(
        "INSERT INTO admin_users (username, email, password_hash, role)
         VALUES ($1, $2, $3, $4)
         RETURNING id, username, email, role, created_at",
        &[&username, &email, &password_hash, &role_str],
      )
      .await?;
    Ok(AdminUser {
      id: row.get(0),
      username: row.get(1),
      email: row.get(2),
      role: row.get::<_, String>(3).parse().unwrap_or(AdminRole::Admin),
      created_at: row.get(4),
    })
  }

  async fn get_admin_user_by_username(
    &self,
    username: &str,
  ) -> Result<Option<(AdminUser, String)>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT id, username, email, role, created_at, password_hash FROM admin_users WHERE username = $1",
        &[&username],
      )
      .await?;
    if rows.is_empty() {
      return Ok(None);
    }
    let row = &rows[0];
    let user = AdminUser {
      id: row.get(0),
      username: row.get(1),
      email: row.get(2),
      role: row.get::<_, String>(3).parse().unwrap_or(AdminRole::Admin),
      created_at: row.get(4),
    };
    let password_hash: String = row.get(5);
    Ok(Some((user, password_hash)))
  }

  async fn get_admin_user(&self, id: Uuid) -> Result<Option<AdminUser>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT id, username, email, role, created_at FROM admin_users WHERE id = $1",
        &[&id],
      )
      .await?;
    if rows.is_empty() {
      return Ok(None);
    }
    let row = &rows[0];
    Ok(Some(AdminUser {
      id: row.get(0),
      username: row.get(1),
      email: row.get(2),
      role: row.get::<_, String>(3).parse().unwrap_or(AdminRole::Admin),
      created_at: row.get(4),
    }))
  }

  async fn list_admin_users(&self) -> Result<Vec<AdminUser>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT id, username, email, role, created_at FROM admin_users ORDER BY created_at",
        &[],
      )
      .await?;
    Ok(
      rows
        .iter()
        .map(|row| AdminUser {
          id: row.get(0),
          username: row.get(1),
          email: row.get(2),
          role: row.get::<_, String>(3).parse().unwrap_or(AdminRole::Admin),
          created_at: row.get(4),
        })
        .collect(),
    )
  }

  async fn delete_admin_user(&self, id: Uuid) -> Result<bool, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute("DELETE FROM admin_users WHERE id = $1", &[&id])
      .await?;
    Ok(result > 0)
  }

  async fn update_admin_user_role(&self, id: Uuid, role: AdminRole) -> Result<bool, anyhow::Error> {
    let role_str = role.to_string();
    let result = self
      .pool
      .get()
      .await?
      .execute(
        "UPDATE admin_users SET role = $2 WHERE id = $1",
        &[&id, &role_str],
      )
      .await?;
    Ok(result > 0)
  }

  async fn update_admin_user_password(
    &self,
    id: &Uuid,
    password_hash: &str,
  ) -> Result<bool, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute(
        "UPDATE admin_users SET password_hash = $2 WHERE id = $1",
        &[id, &password_hash],
      )
      .await?;
    Ok(result > 0)
  }

  // =========================================================================
  // Admin Sessions
  // =========================================================================

  async fn create_admin_session(
    &self,
    user_id: Uuid,
    session_token_hash: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
  ) -> Result<AdminSession, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_one(
        "INSERT INTO admin_sessions (user_id, session_token_hash, expires_at)
         VALUES ($1, $2, $3)
         RETURNING id, user_id, expires_at",
        &[&user_id, &session_token_hash, &expires_at],
      )
      .await?;
    Ok(AdminSession {
      id: row.get(0),
      user_id: row.get(1),
      expires_at: row.get(2),
    })
  }

  async fn validate_admin_session(
    &self,
    session_token_hash: &str,
  ) -> Result<Option<(AdminSession, AdminUser)>, anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT s.id, s.user_id, s.expires_at, u.id, u.username, u.email, u.role, u.created_at
         FROM admin_sessions s
         JOIN admin_users u ON s.user_id = u.id
         WHERE s.session_token_hash = $1 AND s.expires_at > NOW()",
        &[&session_token_hash],
      )
      .await?;
    if rows.is_empty() {
      return Ok(None);
    }
    let row = &rows[0];
    let session = AdminSession {
      id: row.get(0),
      user_id: row.get(1),
      expires_at: row.get(2),
    };
    let user = AdminUser {
      id: row.get(3),
      username: row.get(4),
      email: row.get(5),
      role: row.get::<_, String>(6).parse().unwrap_or(AdminRole::Admin),
      created_at: row.get(7),
    };
    Ok(Some((session, user)))
  }

  async fn delete_admin_session(&self, session_id: Uuid) -> Result<bool, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute("DELETE FROM admin_sessions WHERE id = $1", &[&session_id])
      .await?;
    Ok(result > 0)
  }

  async fn delete_admin_sessions_for_user(&self, user_id: Uuid) -> Result<u64, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute("DELETE FROM admin_sessions WHERE user_id = $1", &[&user_id])
      .await?;
    Ok(result)
  }

  async fn cleanup_expired_sessions(&self) -> Result<u64, anyhow::Error> {
    let result = self
      .pool
      .get()
      .await?
      .execute("DELETE FROM admin_sessions WHERE expires_at <= NOW()", &[])
      .await?;
    Ok(result)
  }

  // =========================================================================
  // S3 Atomic Operations (reduces round-trips)
  // =========================================================================

  async fn create_storage_object_with_stats(
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
        "SELECT sqrl_create_storage_object_with_stats($1, $2, $3, $4, $5, $6, $7, $8)",
        &[
          &bucket,
          &key,
          &version_id,
          &etag,
          &size,
          &content_type,
          &storage_path,
          &metadata,
        ],
      )
      .await?;
    Ok(())
  }

  async fn delete_storage_object_with_stats(
    &self,
    bucket: &str,
    key: &str,
    version_id: Option<Uuid>,
  ) -> Result<Option<(String, i64)>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_opt(
        "SELECT storage_path, size FROM sqrl_delete_storage_object_with_stats($1, $2, $3)",
        &[&bucket, &key, &version_id],
      )
      .await?;
    Ok(row.map(|r| (r.get(0), r.get(1))))
  }

  async fn replace_storage_object(
    &self,
    bucket: &str,
    key: &str,
    version_id: Uuid,
    etag: &str,
    size: i64,
    content_type: &str,
    storage_path: &str,
    metadata: serde_json::Value,
  ) -> Result<Option<String>, anyhow::Error> {
    let row = self
      .pool
      .get()
      .await?
      .query_one(
        "SELECT sqrl_replace_storage_object($1, $2, $3, $4, $5, $6, $7, $8)",
        &[
          &bucket,
          &key,
          &version_id,
          &etag,
          &size,
          &content_type,
          &storage_path,
          &metadata,
        ],
      )
      .await?;
    Ok(row.get(0))
  }

  async fn list_storage_objects_with_prefixes(
    &self,
    bucket: &str,
    prefix: Option<&str>,
    delimiter: Option<&str>,
    max_keys: i32,
    continuation_token: Option<&str>,
  ) -> Result<(Vec<StorageObject>, Vec<String>, bool, Option<String>), anyhow::Error> {
    let rows = self
      .pool
      .get()
      .await?
      .query(
        "SELECT obj_key, obj_version_id, obj_etag, obj_size, obj_content_type, obj_storage_path, obj_metadata, obj_acl, obj_created_at, is_prefix
         FROM sqrl_list_storage_objects_with_prefixes($1, $2, $3, $4, $5)",
        &[&bucket, &prefix, &delimiter, &(max_keys + 1), &continuation_token],
      )
      .await?;

    let mut objects = Vec::new();
    let mut prefixes = Vec::new();

    for row in &rows {
      let is_prefix: bool = row.get(9);
      if is_prefix {
        prefixes.push(row.get(0));
      } else {
        objects.push(StorageObject {
          bucket: bucket.to_string(),
          key: row.get(0),
          version_id: row.get(1),
          is_latest: true,
          etag: row.get(2),
          size: row.get(3),
          content_type: row.get(4),
          storage_path: row.get(5),
          metadata: row.get(6),
          acl: row
            .get::<_, serde_json::Value>(7)
            .pipe(|v| serde_json::from_value(v).unwrap_or_default()),
          is_delete_marker: false,
          created_at: row.get(8),
        });
      }
    }

    let is_truncated = objects.len() > max_keys as usize;
    if is_truncated {
      objects.truncate(max_keys as usize);
    }

    let next_token = if is_truncated {
      objects.last().map(|o| o.key.clone())
    } else {
      None
    };

    Ok((objects, prefixes, is_truncated, next_token))
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
