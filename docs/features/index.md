# Features

SquirrelDB includes optional features that can be enabled individually:

## Available Features

| Feature | Description | Default |
|---------|-------------|---------|
| [Storage](./storage.md) | S3-compatible object storage | Disabled |
| [Caching](./caching.md) | Redis-compatible in-memory cache | Disabled |
| [Backup](./backup.md) | Automatic database backups | Disabled |

## Enabling Features

Enable features in your `squirreldb.yaml`:

```yaml
features:
  storage: true    # S3-compatible object storage
  caching: true    # Redis-compatible cache
  backup: true     # Automatic database backups
```

Or via environment variables:

```bash
SQRL_STORAGE_ENABLED=true
SQRL_CACHE_ENABLED=true
SQRL_BACKUP_ENABLED=true
```

## Feature Modes

Storage and caching features support two operating modes:

### Built-in Mode

Uses local resources (filesystem for storage, memory for cache). Ideal for:
- Development and testing
- Single-server deployments
- Self-contained installations

### Proxy Mode

Connects to external services (S3 providers, Redis servers). Ideal for:
- Multi-instance deployments
- Cloud-native infrastructure
- Production environments with existing services

## Feature Dependencies

The Backup feature can optionally use the Storage feature:

- **Backup + Storage**: Backups stored to S3 (`s3://backups/`)
- **Backup only**: Backups stored locally (`./backup/`)

## Configuration Example

Full configuration with all features:

```yaml
features:
  storage: true
  caching: true
  backup: true

storage:
  mode: proxy
  proxy:
    endpoint: "https://s3.amazonaws.com"
    region: "us-west-2"
    access_key_id: "AKIAIOSFODNN7EXAMPLE"
    secret_access_key: "your-secret-key"

caching:
  mode: proxy
  proxy:
    host: "redis.example.com"
    port: 6379
    password: "your-redis-password"
    tls_enabled: true

backup:
  interval: 3600    # Every hour
  retention: 7      # Keep 7 backups
```

## Admin UI Configuration

The Admin UI itself can be disabled for production deployments:

```yaml
server:
  admin: false
```

Or via environment variable:

```bash
SQRL_ADMIN_ENABLED=false
```

## Configuring Features via Admin UI

When enabled, all features can be configured through the Admin UI:

### Storage & Caching

1. Navigate to **Settings**
2. Select **Storage** or **Caching** tab
3. Toggle between Built-in and Proxy modes
4. Configure provider settings
5. Test connection
6. Save changes

### Backup

1. Navigate to **Settings > General**
2. Find the **Database Backups** card
3. Toggle **Enable Automatic Backups**
4. View backup interval, retention, and status

Changes take effect immediately without server restart.
