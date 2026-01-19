# Backup & Restore

This guide covers backup and restore strategies for SquirrelDB.

## PostgreSQL Backups

### pg_dump (Recommended)

Use PostgreSQL's built-in backup tools:

```bash
# Full backup
pg_dump -h localhost -U postgres squirreldb > backup.sql

# Compressed backup
pg_dump -h localhost -U postgres squirreldb | gzip > backup.sql.gz

# Custom format (for parallel restore)
pg_dump -h localhost -U postgres -Fc squirreldb > backup.dump
```

### Restore from pg_dump

```bash
# From SQL file
psql -h localhost -U postgres squirreldb < backup.sql

# From compressed file
gunzip -c backup.sql.gz | psql -h localhost -U postgres squirreldb

# From custom format
pg_restore -h localhost -U postgres -d squirreldb backup.dump
```

### Continuous Archiving (WAL)

For point-in-time recovery, configure WAL archiving:

```ini
# postgresql.conf
archive_mode = on
archive_command = 'cp %p /backup/wal/%f'
```

### Automated Backups

```bash
#!/bin/bash
# /opt/scripts/backup.sh

DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR=/backup/squirreldb
RETENTION_DAYS=30

# Create backup
pg_dump -h localhost -U postgres squirreldb | gzip > "$BACKUP_DIR/backup_$DATE.sql.gz"

# Remove old backups
find "$BACKUP_DIR" -name "backup_*.sql.gz" -mtime +$RETENTION_DAYS -delete

# Log
echo "$(date): Backup completed: backup_$DATE.sql.gz"
```

Add to crontab:
```
0 2 * * * /opt/scripts/backup.sh >> /var/log/backup.log 2>&1
```

## SQLite Backups

### File Copy

The simplest method (requires stopping the server):

```bash
# Stop server
systemctl stop squirreldb

# Copy database
cp /var/lib/squirreldb/data.db /backup/data.db

# Start server
systemctl start squirreldb
```

### SQLite Backup API

For online backups without stopping the server:

```bash
sqlite3 /var/lib/squirreldb/data.db ".backup /backup/data.db"
```

### Automated SQLite Backups

```bash
#!/bin/bash
# /opt/scripts/backup-sqlite.sh

DATE=$(date +%Y%m%d_%H%M%S)
DB_PATH=/var/lib/squirreldb/data.db
BACKUP_DIR=/backup/squirreldb
RETENTION_DAYS=30

# Create backup using SQLite backup API
sqlite3 "$DB_PATH" ".backup $BACKUP_DIR/backup_$DATE.db"

# Compress
gzip "$BACKUP_DIR/backup_$DATE.db"

# Remove old backups
find "$BACKUP_DIR" -name "backup_*.db.gz" -mtime +$RETENTION_DAYS -delete

echo "$(date): Backup completed: backup_$DATE.db.gz"
```

## Cloud Storage

### AWS S3

```bash
#!/bin/bash
DATE=$(date +%Y%m%d_%H%M%S)
BUCKET=my-squirreldb-backups

# PostgreSQL backup to S3
pg_dump -h localhost -U postgres squirreldb | gzip | \
  aws s3 cp - "s3://$BUCKET/backup_$DATE.sql.gz"

# SQLite backup to S3
sqlite3 /var/lib/squirreldb/data.db ".backup /tmp/backup.db"
gzip /tmp/backup.db
aws s3 cp /tmp/backup.db.gz "s3://$BUCKET/backup_$DATE.db.gz"
rm /tmp/backup.db.gz
```

### Google Cloud Storage

```bash
#!/bin/bash
DATE=$(date +%Y%m%d_%H%M%S)
BUCKET=gs://my-squirreldb-backups

# PostgreSQL backup to GCS
pg_dump -h localhost -U postgres squirreldb | gzip | \
  gsutil cp - "$BUCKET/backup_$DATE.sql.gz"
```

## Restore Procedures

### PostgreSQL Restore

1. **Stop the application** using SquirrelDB
2. **Drop existing database** (if doing full restore):
   ```bash
   psql -h localhost -U postgres -c "DROP DATABASE squirreldb;"
   psql -h localhost -U postgres -c "CREATE DATABASE squirreldb;"
   ```
3. **Restore backup**:
   ```bash
   gunzip -c backup.sql.gz | psql -h localhost -U postgres squirreldb
   ```
4. **Restart SquirrelDB**:
   ```bash
   systemctl restart squirreldb
   ```

### SQLite Restore

1. **Stop SquirrelDB**:
   ```bash
   systemctl stop squirreldb
   ```
2. **Replace database file**:
   ```bash
   gunzip -c backup.db.gz > /var/lib/squirreldb/data.db
   ```
3. **Start SquirrelDB**:
   ```bash
   systemctl start squirreldb
   ```

## Backup Verification

Always verify backups:

### PostgreSQL

```bash
# Create test database
createdb -h localhost -U postgres test_restore

# Restore to test database
pg_restore -h localhost -U postgres -d test_restore backup.dump

# Verify data
psql -h localhost -U postgres test_restore -c "SELECT COUNT(*) FROM documents;"

# Cleanup
dropdb -h localhost -U postgres test_restore
```

### SQLite

```bash
# Restore to temporary file
gunzip -c backup.db.gz > /tmp/test_restore.db

# Verify integrity
sqlite3 /tmp/test_restore.db "PRAGMA integrity_check;"

# Verify data
sqlite3 /tmp/test_restore.db "SELECT COUNT(*) FROM documents;"

# Cleanup
rm /tmp/test_restore.db
```

## Disaster Recovery

### Recovery Point Objective (RPO)

How much data can you afford to lose?

| Backup Frequency | Max Data Loss |
|-----------------|---------------|
| Daily | Up to 24 hours |
| Hourly | Up to 1 hour |
| Continuous (WAL) | Minutes |

### Recovery Time Objective (RTO)

How quickly must you recover?

| Strategy | Recovery Time |
|----------|--------------|
| Hot standby | Minutes |
| Restore from backup | Hours |
| Restore from offsite | Hours to days |

### High Availability Setup

For minimal downtime:

1. **Primary PostgreSQL** with streaming replication
2. **Standby PostgreSQL** for failover
3. **Multiple SquirrelDB instances** behind load balancer

```yaml
# docker-compose.yml for HA
services:
  postgres-primary:
    image: postgres:16
    environment:
      - POSTGRES_USER=postgres
      - POSTGRES_PASSWORD=secret

  postgres-standby:
    image: postgres:16
    environment:
      - POSTGRES_USER=postgres
      - POSTGRES_PASSWORD=secret
    depends_on:
      - postgres-primary

  squirreldb-1:
    image: squirreldb/squirreldb
    environment:
      - DATABASE_URL=postgres://postgres:secret@postgres-primary/squirreldb

  squirreldb-2:
    image: squirreldb/squirreldb
    environment:
      - DATABASE_URL=postgres://postgres:secret@postgres-primary/squirreldb
```

## Best Practices

1. **Test restores regularly**
   - Don't assume backups work until tested
   - Practice the full restore procedure

2. **Use multiple backup locations**
   - Local storage for fast recovery
   - Offsite/cloud for disaster recovery

3. **Monitor backup jobs**
   - Alert on backup failures
   - Track backup sizes over time

4. **Document procedures**
   - Step-by-step restore instructions
   - Contact information for emergencies

5. **Encrypt sensitive backups**
   ```bash
   pg_dump ... | gzip | gpg -c > backup.sql.gz.gpg
   ```

6. **Retain multiple generations**
   - Daily backups for 7 days
   - Weekly backups for 4 weeks
   - Monthly backups for 12 months
