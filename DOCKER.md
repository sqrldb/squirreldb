# SquirrelDB

Real-time document database with WebSocket subscriptions, S3-compatible storage, and Redis-compatible cache.

## Quick Start

```bash
docker run -d \
  --name squirreldb \
  -p 8080:8080 \
  -p 8081:8081 \
  -v squirreldb-data:/app/data \
  squirreldb/squirreldb
```

Then open http://localhost:8081 for the Admin UI.

## All Features

```bash
docker run -d \
  --name squirreldb \
  -p 8080:8080 \
  -p 8081:8081 \
  -p 8082:8082 \
  -p 8083:8083 \
  -p 9000:9000 \
  -p 6379:6379 \
  -v squirreldb-data:/app/data \
  -e SQRL_STORAGE_ENABLED=true \
  -e SQRL_CACHE_ENABLED=true \
  -e SQRL_MCP_ENABLED=true \
  squirreldb/squirreldb
```

## Ports

| Port | Service |
|------|---------|
| 8080 | HTTP/WebSocket API |
| 8081 | Admin UI |
| 8082 | TCP binary protocol |
| 8083 | MCP (AI assistants) |
| 9000 | S3-compatible storage |
| 6379 | Redis-compatible cache |

## Environment Variables

### Backend
- `SQRL_BACKEND` - `sqlite` (default) or `postgres`
- `DATABASE_URL` - PostgreSQL connection string
- `SQRL_SQLITE_PATH` - SQLite file path (default: `/app/data/squirreldb.db`)

### Storage (S3-compatible)
- `SQRL_STORAGE_ENABLED` - Enable object storage (default: `false`)
- `SQRL_STORAGE_PATH` - Local storage path (default: `/app/data/storage`)
- `SQRL_STORAGE_REGION` - S3 region (default: `us-east-1`)

### Cache (Redis-compatible)
- `SQRL_CACHE_ENABLED` - Enable cache (default: `false`)
- `SQRL_CACHE_PORT` - Cache port (default: `6379`)
- `SQRL_CACHE_MAX_MEMORY` - Max memory (default: `256mb`)

### MCP (AI Assistants)
- `SQRL_MCP_ENABLED` - Enable MCP protocol (default: `false`)

### Logging
- `SQRL_LOG_LEVEL` - Log level: `debug`, `info`, `warn`, `error`
- `RUST_LOG` - Rust log filter

## Docker Compose

```yaml
services:
  squirreldb:
    image: squirreldb/squirreldb:latest
    ports:
      - "8080:8080"
      - "8081:8081"
      - "9000:9000"
      - "6379:6379"
    volumes:
      - squirreldb_data:/app/data
    environment:
      SQRL_STORAGE_ENABLED: "true"
      SQRL_CACHE_ENABLED: "true"
    restart: unless-stopped

volumes:
  squirreldb_data:
```

## With PostgreSQL

```yaml
services:
  squirreldb:
    image: squirreldb/squirreldb:latest
    ports:
      - "8080:8080"
      - "8081:8081"
    environment:
      SQRL_BACKEND: postgres
      DATABASE_URL: postgres://squirrel:squirrel@postgres/squirreldb
    depends_on:
      postgres:
        condition: service_healthy

  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: squirrel
      POSTGRES_PASSWORD: squirrel
      POSTGRES_DB: squirreldb
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U squirrel"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  postgres_data:
```

## Links

- [Documentation](https://squirreldb.dev/docs)
- [GitHub](https://github.com/sqrldb/squirreldb)
- [SDKs](https://squirreldb.dev/docs/sdks)
