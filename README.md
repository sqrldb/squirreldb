# SquirrelDB

A real-time document database built in Rust with PostgreSQL and SQLite backends, featuring S3-compatible object storage and Redis-compatible caching.

## Features

- **Real-time Subscriptions** - Subscribe to changes with live query updates
- **Structured Queries** - Type-safe query builder that compiles directly to SQL
- **Multiple Backends** - SQLite for development, PostgreSQL for production
- **WebSocket & TCP** - Connect via WebSocket or high-performance TCP protocol
- **Built-in Admin UI** - Monitor, browse data, manage storage, and configure settings
- **S3-Compatible Storage** - Object storage with file browser and external S3 proxy support
- **Redis-Compatible Cache** - In-memory caching with external Redis proxy support
- **MCP Server** - Model Context Protocol integration for AI assistants

## Quick Start

### Using Docker

```bash
docker run -p 8080:8080 -p 8081:8081 -p 6379:6379 -p 9000:9000 sqrldb/squirreldb:latest
```

### From Source

```bash
git clone https://github.com/sqrldb/squirreldb.git
cd squirreldb
cargo build --release
./target/release/sqrld
```

### Connect

```typescript
import { SquirrelDB } from "squirreldb-sdk"

const db = await SquirrelDB.connect("localhost:8080")

// Insert a document
const user = await db.insert("users", { name: "Alice", age: 30 })

// Query with fluent API
const adults = await db.table("users")
  .find(doc => doc.age.gte(18))
  .sort("name")
  .run()

// Subscribe to changes
await db.subscribe("users")
  .changes(change => console.log("Changed:", change))
```

## Configuration

Create a `squirreldb.yaml` file:

```yaml
server:
  host: "0.0.0.0"
  ports:
    http: 8080      # WebSocket/REST
    admin: 8081     # Admin UI
    tcp: 8082       # TCP wire protocol
    mcp: 8083       # MCP SSE server

backend: sqlite     # or postgres

sqlite:
  path: "./data/squirreldb.db"

postgres:
  url: "postgres://localhost/squirreldb"
  max_connections: 20

# Optional features
features:
  storage: true     # S3-compatible object storage
  caching: true     # Redis-compatible cache

# Storage configuration
storage:
  mode: builtin     # builtin or proxy
  port: 9000
  data_path: "./storage"
  # Proxy mode (connect to external S3)
  # proxy:
  #   endpoint: "https://s3.amazonaws.com"
  #   region: "us-west-2"
  #   access_key_id: "AKIAIOSFODNN7EXAMPLE"
  #   secret_access_key: "your-secret-key"

# Cache configuration
caching:
  mode: builtin     # builtin or proxy
  port: 6379
  max_memory: "256mb"
  eviction: "lru"
  # Proxy mode (connect to external Redis)
  # proxy:
  #   host: "redis.example.com"
  #   port: 6379
  #   password: "secret"
  #   tls_enabled: true

logging:
  level: info
```

## Admin UI

Access the admin panel at `http://localhost:8081`:

- **Dashboard** - Server stats, connection counts, query metrics
- **Explorer** - Browse collections and documents
- **Console** - Execute queries interactively
- **Storage** - Browse, upload, download, and manage files
- **Settings** - Configure storage/cache modes, manage tokens

## Object Storage

S3-compatible storage with built-in file browser:

```bash
# Use AWS CLI
aws configure set default.s3.endpoint_url http://localhost:9000
aws s3 mb s3://my-bucket
aws s3 cp myfile.txt s3://my-bucket/

# Or any S3 SDK
```

**Proxy Mode**: Connect to AWS S3, MinIO, DigitalOcean Spaces, or any S3-compatible provider.

## Caching

Redis-compatible cache layer:

```bash
# Use redis-cli
redis-cli -p 6379
> SET user:1 '{"name":"Alice"}'
> GET user:1
> KEYS user:*
```

**Proxy Mode**: Connect to AWS ElastiCache, Redis Cloud, or any Redis server.

## MCP Server (Model Context Protocol)

Built-in MCP server for AI assistant integration.

### Tools Available

| Tool | Description |
|------|-------------|
| `query` | Execute structured queries |
| `insert` | Insert a document into a collection |
| `update` | Update a document by ID |
| `delete` | Delete a document by ID |
| `list_collections` | List all collections |

### Claude Desktop Integration

```bash
sqrl mcp
```

Add to Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "squirreldb": {
      "command": "/path/to/sqrl",
      "args": ["mcp"]
    }
  }
}
```

## Client SDKs

| Language | Package | Install |
|----------|---------|---------|
| TypeScript/JS | [sdk-typescript](https://github.com/sqrldb/sdk-typescript) | `bun add squirreldb-sdk` |
| Python | [sdk-python](https://github.com/sqrldb/sdk-python) | `pip install squirreldb-sdk` |
| Go | [sdk-go](https://github.com/sqrldb/sdk-go) | `go get github.com/sqrldb/sdk-go` |
| Rust | [sdk-rust](https://github.com/sqrldb/sdk-rust) | `cargo add squirreldb-sdk` |
| Ruby | [sdk-ruby](https://github.com/sqrldb/sdk-ruby) | `gem install squirreldb-sdk` |
| Elixir | [sdk-elixir](https://github.com/sqrldb/sdk-elixir) | `{:squirreldb, "~> 1.0"}` |
| C | [sdk-c](https://github.com/sqrldb/sdk-c) | [Releases](https://github.com/sqrldb/sdk-c/releases) |

## Documentation

Visit [squirreldb.com](https://squirreldb.com) for full documentation including:

- [Getting Started](https://squirreldb.com/docs)
- [Query Language](https://squirreldb.com/docs/queries)
- [Real-time Subscriptions](https://squirreldb.com/docs/realtime)
- [Storage & Caching](https://squirreldb.com/docs/storage)
- [API Reference](https://squirreldb.com/docs/api)
- [SDK Guides](https://squirreldb.com/docs/sdks)

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Clients                               │
│  (TypeScript, Python, Go, Rust, Ruby, Elixir, C)            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    SquirrelDB Server                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  WebSocket  │  │  Admin UI   │  │  Subscription       │  │
│  │  :8080      │  │  :8081      │  │  Manager            │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
│                                                              │
│  ┌────────────────────────┐  ┌────────────────────────┐    │
│  │  Storage (S3 API)      │  │  Cache (Redis API)     │    │
│  │  :9000                 │  │  :6379                 │    │
│  │  Built-in / S3 Proxy   │  │  Built-in / Redis Proxy│    │
│  └────────────────────────┘  └────────────────────────┘    │
│                              │                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │               Database Backend                         │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
┌─────────────────────────┐     ┌─────────────────────────┐
│       PostgreSQL        │     │         SQLite          │
│   (Production)          │     │   (Development)         │
└─────────────────────────┘     └─────────────────────────┘
```

## License

Apache License 2.0 - see [LICENSE](LICENSE) for details.
