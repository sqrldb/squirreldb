# PostgreSQL Backend

PostgreSQL is the recommended backend for production deployments.

## Requirements

- PostgreSQL 14 or higher
- A database with appropriate permissions

## Connection URL

The connection URL follows the standard PostgreSQL format:

```
postgres://[user[:password]@][host][:port][/database][?param=value&...]
```

### Examples

```yaml
postgres:
  # Local with default user
  url: "postgres://localhost/squirreldb"

  # With credentials
  url: "postgres://myuser:mypassword@localhost/squirreldb"

  # Remote server
  url: "postgres://user:pass@db.example.com:5432/squirreldb"

  # With SSL
  url: "postgres://user:pass@db.example.com/squirreldb?sslmode=require"

  # Multiple hosts (failover)
  url: "postgres://user:pass@host1,host2/squirreldb?target_session_attrs=read-write"
```

### Connection Parameters

| Parameter | Values | Description |
|-----------|--------|-------------|
| `sslmode` | `disable`, `prefer`, `require`, `verify-ca`, `verify-full` | SSL mode |
| `connect_timeout` | seconds | Connection timeout |
| `application_name` | string | Application name for monitoring |
| `target_session_attrs` | `read-write`, `read-only`, `any` | Session attributes |

## Schema

SquirrelDB automatically creates the required schema on first run:

```sql
-- JavaScript-friendly UUID alias
CREATE OR REPLACE FUNCTION uuid() RETURNS UUID AS $$
  SELECT gen_random_uuid();
$$ LANGUAGE SQL;

-- Documents table
CREATE TABLE IF NOT EXISTS documents (
    id UUID PRIMARY KEY DEFAULT uuid(),
    collection VARCHAR(255) NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection);
CREATE INDEX IF NOT EXISTS idx_documents_data ON documents USING GIN(data);

-- Change tracking
CREATE TABLE IF NOT EXISTS change_queue (
    id BIGSERIAL PRIMARY KEY,
    collection VARCHAR(255) NOT NULL,
    document_id UUID NOT NULL,
    operation VARCHAR(10) NOT NULL,
    old_data JSONB,
    new_data JSONB,
    changed_at TIMESTAMPTZ DEFAULT NOW()
);

-- Trigger for change capture
CREATE OR REPLACE FUNCTION capture_document_changes() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO change_queue (collection, document_id, operation, new_data)
        VALUES (NEW.collection, NEW.id, 'INSERT', NEW.data);
    ELSIF TG_OP = 'UPDATE' THEN
        INSERT INTO change_queue (collection, document_id, operation, old_data, new_data)
        VALUES (NEW.collection, NEW.id, 'UPDATE', OLD.data, NEW.data);
    ELSIF TG_OP = 'DELETE' THEN
        INSERT INTO change_queue (collection, document_id, operation, old_data)
        VALUES (OLD.collection, OLD.id, 'DELETE', OLD.data);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER document_changes_trigger
AFTER INSERT OR UPDATE OR DELETE ON documents
FOR EACH ROW EXECUTE FUNCTION capture_document_changes();
```

### Built-in Functions

SquirrelDB provides a `uuid()` function as a JavaScript-friendly alias for PostgreSQL's `gen_random_uuid()`. This function is used as the default for all document primary keys and can be used in custom queries:

```sql
SELECT uuid();  -- Returns a new random UUID
```

## Connection Pooling

SquirrelDB uses connection pooling internally. Configure the pool size based on your workload:

```yaml
postgres:
  url: $DATABASE_URL
  max_connections: 20  # Default
```

### Sizing Guidelines

| Workload | Recommended Pool Size |
|----------|----------------------|
| Light (< 100 req/s) | 10-20 |
| Medium (100-1000 req/s) | 20-50 |
| Heavy (> 1000 req/s) | 50-100 |

**Note:** Each connection consumes memory on both SquirrelDB and PostgreSQL. Don't over-provision.

## Performance Tuning

### PostgreSQL Configuration

For optimal SquirrelDB performance, consider these PostgreSQL settings:

```ini
# postgresql.conf

# Memory
shared_buffers = 256MB          # 25% of RAM for dedicated DB server
effective_cache_size = 768MB    # 75% of RAM
work_mem = 16MB

# Write performance
wal_buffers = 16MB
checkpoint_completion_target = 0.9

# JSONB operations
enable_seqscan = off           # Prefer index scans for JSONB
```

### Indexes

SquirrelDB creates a GIN index on the `data` column for JSONB queries. For frequently filtered fields, consider additional indexes:

```sql
-- Index on specific JSON field
CREATE INDEX idx_users_email ON documents ((data->>'email'))
WHERE collection = 'users';

-- Index on nested field
CREATE INDEX idx_orders_status ON documents ((data->'status'))
WHERE collection = 'orders';
```

## High Availability

### Read Replicas

For read-heavy workloads, use PostgreSQL read replicas with connection routing:

```yaml
postgres:
  url: "postgres://user:pass@primary,replica1,replica2/squirreldb?target_session_attrs=any"
```

### Failover

Use a connection pooler like PgBouncer or built-in failover:

```yaml
postgres:
  url: "postgres://user:pass@pgbouncer:6432/squirreldb"
```

## Managed PostgreSQL

SquirrelDB works with managed PostgreSQL services:

### AWS RDS

```yaml
postgres:
  url: "postgres://user:pass@mydb.abc123.us-east-1.rds.amazonaws.com:5432/squirreldb?sslmode=require"
```

### Google Cloud SQL

```yaml
postgres:
  url: "postgres://user:pass@/squirreldb?host=/cloudsql/project:region:instance"
```

### Heroku Postgres

```yaml
postgres:
  url: $DATABASE_URL  # Heroku sets this automatically
```

### Supabase

```yaml
postgres:
  url: "postgres://postgres:[password]@db.[ref].supabase.co:5432/postgres"
```

## Troubleshooting

### Connection Refused

```
Error: Connection refused (os error 111)
```

- Check PostgreSQL is running: `pg_isready`
- Verify host and port in connection URL
- Check `pg_hba.conf` allows connections from SquirrelDB host

### Authentication Failed

```
Error: password authentication failed for user "myuser"
```

- Verify username and password
- Check user exists: `\du` in psql
- Verify `pg_hba.conf` authentication method

### Permission Denied

```
Error: permission denied for table documents
```

Grant required permissions:
```sql
GRANT ALL ON ALL TABLES IN SCHEMA public TO myuser;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO myuser;
```

### SSL Required

```
Error: SSL connection required
```

Add `sslmode=require` to connection URL:
```yaml
postgres:
  url: "postgres://user:pass@host/db?sslmode=require"
```
