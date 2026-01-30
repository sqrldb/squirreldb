# Database Backups

SquirrelDB includes a built-in automatic backup system that can store backups to S3 Storage (when enabled) or to the local filesystem.

## Enabling Backups

Enable automatic backups in your `squirreldb.yaml`:

```yaml
features:
  backup: true

backup:
  interval: 3600      # Backup every hour (in seconds)
  retention: 7        # Keep last 7 backups
  local_path: "./backup"        # Local storage path
  storage_path: "backups"       # S3 storage path (when storage enabled)
```

Or via environment variable:

```bash
SQRL_BACKUP_ENABLED=true sqrld
```

Or enable via the Admin UI:

1. Navigate to **Settings > General**
2. Find the **Database Backups** card
3. Toggle **Enable Automatic Backups**
4. Configure interval and retention as needed

## Storage Locations

### When Storage is Enabled

If you have the Storage feature enabled, backups are automatically stored to S3:

- **Location**: `s3://backups/{storage_path}/`
- **Benefit**: Offsite backup, accessible via S3 API
- **Use case**: Production deployments, cloud-native setups

### When Storage is Disabled

If Storage is not enabled, backups are stored locally:

- **Location**: `{local_path}/` (default: `./backup/`)
- **Benefit**: Simple setup, no additional configuration
- **Use case**: Development, single-server deployments

## Backup Format

Backups are SQL files containing:

- All project data
- All collections and documents
- Timestamps for point-in-time reference

**Filename format**: `squirreldb_backup_YYYYMMDD_HHMMSS_XXXXXXXX.sql`

Example: `squirreldb_backup_20240115_143022_a1b2c3d4.sql`

## Configuration Options

| Option | Description | Default |
|--------|-------------|---------|
| `interval` | Seconds between backups | `3600` (1 hour) |
| `retention` | Number of backups to keep | `7` |
| `local_path` | Local backup directory | `./backup` |
| `storage_path` | S3 path prefix | `backups` |

### Interval Examples

```yaml
backup:
  interval: 300       # Every 5 minutes (testing)
  interval: 3600      # Every hour (default)
  interval: 21600     # Every 6 hours
  interval: 86400     # Daily
```

## Admin UI

The Admin UI provides backup management through **Settings > General**:

### Backup Status

When backups are enabled, you'll see:

- **Backup Interval**: How often backups run
- **Retention**: How many backups are kept
- **Storage**: Where backups are stored
- **Last Backup**: When the last backup completed
- **Next Backup**: When the next backup is scheduled

### Manual Backups

You can create a backup manually via the API:

```bash
curl -X POST http://localhost:8081/api/backup/create \
  -H "Authorization: Bearer YOUR_TOKEN"
```

### Listing Backups

```bash
curl http://localhost:8081/api/backup/list \
  -H "Authorization: Bearer YOUR_TOKEN"
```

Response:

```json
[
  {
    "id": "a1b2c3d4",
    "filename": "squirreldb_backup_20240115_143022_a1b2c3d4.sql",
    "size": 1048576,
    "created_at": "2024-01-15T14:30:22Z",
    "backend": "postgres",
    "location": "./backup/squirreldb_backup_20240115_143022_a1b2c3d4.sql"
  }
]
```

### Deleting Backups

```bash
curl -X DELETE http://localhost:8081/api/backup/a1b2c3d4 \
  -H "Authorization: Bearer YOUR_TOKEN"
```

## API Reference

### Get Backup Settings

```
GET /api/backup/settings
```

Response:

```json
{
  "enabled": true,
  "interval": 3600,
  "retention": 7,
  "local_path": "./backup",
  "storage_path": "backups",
  "last_backup": "2024-01-15T14:30:22Z",
  "next_backup": "2024-01-15T15:30:22Z",
  "storage_enabled": false
}
```

### Update Backup Settings

```
PUT /api/backup/settings
Content-Type: application/json

{
  "interval": 7200,
  "retention": 14
}
```

### List Backups

```
GET /api/backup/list
```

### Create Backup

```
POST /api/backup/create
```

### Delete Backup

```
DELETE /api/backup/{id}
```

## Restore from Backup

Backup files are standard SQL that can be restored using your database client.

### PostgreSQL

```bash
# Download backup from S3 (if using storage)
aws s3 cp s3://backups/squirreldb_backup_20240115_143022_a1b2c3d4.sql backup.sql

# Restore
psql -h localhost -U postgres squirreldb < backup.sql
```

### SQLite

For SQLite, the backup contains INSERT statements. To restore:

1. Stop SquirrelDB
2. Create a fresh database
3. Execute the backup SQL
4. Start SquirrelDB

```bash
# Fresh database
rm squirreldb.db
sqlite3 squirreldb.db < backup.sql
```

## Best Practices

### 1. Match Interval to RPO

Your backup interval should match your Recovery Point Objective (RPO):

| RPO | Recommended Interval |
|-----|---------------------|
| 1 hour | `3600` |
| 6 hours | `21600` |
| 24 hours | `86400` |

### 2. Use Storage for Production

Enable Storage feature for production backups:

```yaml
features:
  storage: true
  backup: true
```

This provides:
- Offsite backup storage
- S3 API access to backups
- Better disaster recovery

### 3. Monitor Backup Status

Check backup status via the Admin UI or API to ensure backups are running:

```bash
# Check last backup time
curl http://localhost:8081/api/backup/settings | jq '.last_backup'
```

### 4. Test Restores

Regularly test that backups can be restored:

1. Download a recent backup
2. Restore to a test database
3. Verify data integrity
4. Document the process

### 5. Secure Backup Storage

When using S3 storage:
- Enable encryption at rest
- Use IAM roles with minimal permissions
- Enable versioning on the bucket
- Consider cross-region replication

## Combining with External Backups

The built-in backup feature complements but doesn't replace database-level backups.

For production, consider:

1. **Built-in backups** for quick recovery and portability
2. **pg_dump/sqlite3** for full database dumps
3. **WAL archiving** (PostgreSQL) for point-in-time recovery
4. **Volume snapshots** for infrastructure-level backup

See [Manual Backup & Restore](../operations/backup.md) for database-level backup strategies.
