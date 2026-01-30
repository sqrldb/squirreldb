# Features

SquirrelDB includes optional features that can be enabled individually:

## Available Features

| Feature | Description | Default |
|---------|-------------|---------|
| [Storage](./storage.md) | S3-compatible object storage | Disabled |
| [Caching](./caching.md) | Redis-compatible in-memory cache | Disabled |

## Enabling Features

Enable features in your `squirreldb.yaml`:

```yaml
features:
  storage: true    # S3-compatible object storage
  caching: true    # Redis-compatible cache
```

Or via environment variables:

```bash
SQRL_STORAGE_ENABLED=true
SQRL_CACHE_ENABLED=true
```

## Feature Modes

Both storage and caching features support two operating modes:

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

## Configuration Example

Full configuration with both features in proxy mode:

```yaml
features:
  storage: true
  caching: true

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
```

## Admin UI

Both features can be configured through the Admin UI:

1. Navigate to **Settings**
2. Select **Storage** or **Caching** tab
3. Toggle between Built-in and Proxy modes
4. Configure provider settings
5. Test connection
6. Save changes

Changes take effect immediately without server restart.
