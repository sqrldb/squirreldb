# SQLite Backend

SQLite is perfect for development, testing, and embedded deployments.

## When to Use SQLite

**Good for:**
- Local development
- Testing and CI/CD
- Embedded applications
- Single-server deployments
- Small to medium datasets (< 10GB)

**Not recommended for:**
- High-concurrency workloads (> 100 concurrent writes)
- Multiple server instances (no shared access)
- Very large datasets (> 100GB)

## Configuration

```yaml
backend: sqlite

sqlite:
  path: "squirreldb.db"
```

### Path Options

```yaml
sqlite:
  # Relative path (from working directory)
  path: "squirreldb.db"

  # Absolute path
  path: "/var/lib/squirreldb/data.db"

  # User home directory
  path: "~/.squirreldb/data.db"

  # In-memory database (testing only)
  path: ":memory:"
```

## Performance Optimizations

SquirrelDB automatically applies these optimizations:

```sql
-- Write-Ahead Logging for better concurrency
PRAGMA journal_mode = WAL;

-- Sync less frequently (safe with WAL)
PRAGMA synchronous = NORMAL;

-- 64MB cache
PRAGMA cache_size = -64000;

-- Store temp tables in memory
PRAGMA temp_store = MEMORY;

-- Memory-map 256MB for faster reads
PRAGMA mmap_size = 268435456;
```

### WAL Mode

WAL (Write-Ahead Logging) mode allows concurrent reads while writing. The WAL file (`squirreldb.db-wal`) should be on the same filesystem as the main database.

## Schema

SquirrelDB creates this schema automatically:

```sql
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    collection TEXT NOT NULL,
    data TEXT NOT NULL,  -- JSON stored as TEXT
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection);

CREATE TABLE IF NOT EXISTS change_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    collection TEXT NOT NULL,
    document_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    old_data TEXT,
    new_data TEXT,
    changed_at TEXT DEFAULT (datetime('now'))
);
```

**Note:** The `WITHOUT ROWID` optimization improves performance for UUID primary keys.

## Concurrency

SQLite has limited write concurrency:

- **Reads:** Unlimited concurrent readers
- **Writes:** One writer at a time (with WAL mode, readers don't block)

For high-write workloads, consider PostgreSQL instead.

### Busy Timeout

SquirrelDB sets a busy timeout to handle write contention:

```sql
PRAGMA busy_timeout = 5000;  -- 5 second timeout
```

## Backup

### File Copy

The simplest backup is copying the database file:

```bash
# Stop SquirrelDB first, or use SQLite backup API
cp squirreldb.db squirreldb.db.backup
```

### SQLite Backup Command

For live backups:

```bash
sqlite3 squirreldb.db ".backup backup.db"
```

### Automated Backups

```bash
#!/bin/bash
# backup.sh
DATE=$(date +%Y%m%d_%H%M%S)
sqlite3 /var/lib/squirreldb/data.db ".backup /backups/squirreldb_$DATE.db"

# Keep last 7 days
find /backups -name "squirreldb_*.db" -mtime +7 -delete
```

## File Locations

### Database Files

SQLite creates these files:

| File | Description |
|------|-------------|
| `squirreldb.db` | Main database |
| `squirreldb.db-wal` | Write-ahead log |
| `squirreldb.db-shm` | Shared memory file |

All three files must be present for the database to work. When backing up, include all files or use the SQLite backup API.

### Recommended Directories

```yaml
# Development
sqlite:
  path: "./squirreldb.db"

# Linux production
sqlite:
  path: "/var/lib/squirreldb/data.db"

# macOS
sqlite:
  path: "~/Library/Application Support/SquirrelDB/data.db"
```

## Docker Considerations

When using SQLite with Docker, mount a volume for persistence:

```yaml
# docker-compose.yml
services:
  squirreldb:
    image: squirreldb/squirreldb
    volumes:
      - ./data:/data
    environment:
      - SQUIRRELDB_SQLITE_PATH=/data/squirreldb.db
```

```bash
# Or with docker run
docker run -v $(pwd)/data:/data \
  -e SQUIRRELDB_SQLITE_PATH=/data/squirreldb.db \
  squirreldb/squirreldb
```

## Limitations

### No Network Access

SQLite databases cannot be shared over a network. Each SquirrelDB instance needs its own database file.

### Single Writer

Only one write operation can happen at a time. This is usually fine for most workloads, but PostgreSQL is better for write-heavy applications.

### JSON Functions

SQLite's JSON functions are less powerful than PostgreSQL's JSONB. Complex JSON queries may be slower.

## Troubleshooting

### Database is Locked

```
Error: database is locked
```

- Another process has the database open
- Check for multiple SquirrelDB instances
- Increase busy timeout

### Read-Only Database

```
Error: attempt to write a readonly database
```

- Check file permissions
- Ensure directory is writable
- Check disk space

### Corrupt Database

```
Error: database disk image is malformed
```

Try to recover:
```bash
sqlite3 squirreldb.db ".recover" | sqlite3 recovered.db
```

### Missing WAL File

If the WAL file is missing, SQLite may not be able to open the database:

```bash
# Force checkpoint to merge WAL into main database
sqlite3 squirreldb.db "PRAGMA wal_checkpoint(TRUNCATE);"
```

## Migrating to PostgreSQL

As your application grows, you may want to migrate to PostgreSQL:

```bash
# Export from SQLite
sqlite3 squirreldb.db ".dump documents" > export.sql

# Adjust SQL syntax for PostgreSQL
# - Change TEXT to appropriate types
# - Adjust datetime functions

# Import to PostgreSQL
psql squirreldb < export.sql
```

Or use SquirrelDB's export/import (when available):

```bash
sqrl export --sqlite squirreldb.db --output backup.json
sqrl import --pg-url postgres://localhost/squirreldb --input backup.json
```
