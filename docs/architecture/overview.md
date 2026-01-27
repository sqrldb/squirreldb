# Architecture Overview

SquirrelDB is a real-time document database built in Rust. This document describes its internal architecture, component interactions, and design decisions.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              Clients                                     │
│         TypeScript │ Python │ Ruby │ Elixir │ WebSocket │ REST          │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
         ┌─────────────────────┐         ┌─────────────────────┐
         │   WebSocket Server  │         │   Admin Server      │
         │   (Port 8080)       │         │   (Port 8081)       │
         │                     │         │                     │
         │  - Client Conn Mgmt │         │  - Leptos SSR UI    │
         │  - Message Routing  │         │  - REST API         │
         │  - Subscriptions    │         │  - Log Streaming    │
         └─────────────────────┘         └─────────────────────┘
                    │                               │
                    └───────────────┬───────────────┘
                                    ▼
                    ┌─────────────────────────────────┐
                    │        Message Handler          │
                    │                                 │
                    │  - Query parsing & validation   │
                    │  - Auth token verification      │
                    │  - Request routing              │
                    └─────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
         ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐
         │ Query Engine │  │ Subscription │  │ Document Store   │
         │              │  │ Manager      │  │                  │
         │ - JS Parsing │  │              │  │ - CRUD Ops       │
         │ - SQL Compile│  │ - Changefeed │  │ - Transactions   │
         │ - Execution  │  │ - Client Map │  │ - Filtering      │
         └──────────────┘  └──────────────┘  └──────────────────┘
                    │               │               │
                    └───────────────┴───────────────┘
                                    │
                    ┌─────────────────────────────────┐
                    │      Database Backend Trait     │
                    │                                 │
                    │  - Abstract storage interface   │
                    │  - Change notification          │
                    │  - Token management             │
                    └─────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
         ┌─────────────────────┐         ┌─────────────────────┐
         │   PostgreSQL        │         │   SQLite            │
         │   Backend           │         │   Backend           │
         │                     │         │                     │
         │  - LISTEN/NOTIFY    │         │  - File-based       │
         │  - Connection Pool  │         │  - In-memory option │
         │  - JSONB Storage    │         │  - JSON Functions   │
         └─────────────────────┘         └─────────────────────┘
```

## Core Components

### 1. Daemon (`src/server/daemon.rs`)

The main orchestrator that:

- Initializes the database backend
- Starts the change listener
- Spawns the WebSocket server
- Spawns the Admin server
- Handles graceful shutdown

```rust
pub struct Daemon {
  config: ServerConfig,
  backend: Arc<dyn DatabaseBackend>,
  subs: Arc<SubscriptionManager>,
  shutdown_tx: broadcast::Sender<()>,
}
```

### 2. WebSocket Server (`src/server/websocket.rs`)

Handles client connections:

- Accepts WebSocket upgrades
- Maintains client connection map
- Routes messages to handler
- Broadcasts subscription updates

### 3. Admin Server (`src/admin/api.rs`)

Provides HTTP interface:

- Leptos SSR for UI
- REST API endpoints
- Health checks
- Log streaming WebSocket
- Authentication middleware

### 4. Message Handler (`src/server/handler.rs`)

Processes client messages:

- Parses incoming JSON
- Validates requests
- Routes to appropriate handler
- Formats responses

Supported message types:
- `query` - Execute a query
- `subscribe` - Start changefeed
- `unsubscribe` - Stop changefeed
- `insert` - Insert document
- `update` - Update document
- `delete` - Delete document
- `list_collections` - List tables
- `ping` - Connectivity check

### 5. Query Engine (`src/query/engine.rs`)

Parses and executes queries:

- JavaScript parsing via rquickjs
- SQL compilation for simple filters
- Fallback to JS evaluation
- Query plan optimization

```rust
pub struct QueryEngine {
  runtime: Runtime,
  dialect: SqlDialect,
}

impl QueryEngine {
  pub fn parse_query(&self, query: &str) -> Result<QuerySpec>;
}
```

### 6. Subscription Manager (`src/subscriptions/manager.rs`)

Manages real-time subscriptions:

- Tracks active subscriptions per client
- Processes change events
- Matches changes to subscriptions
- Delivers updates to clients

```rust
pub struct SubscriptionManager {
  subscriptions: RwLock<HashMap<Uuid, Vec<Subscription>>>,
  change_tx: broadcast::Sender<Change>,
}
```

### 7. Database Backend Trait (`src/db/mod.rs`)

Abstract interface for storage:

```rust
#[async_trait]
pub trait DatabaseBackend: Send + Sync {
  fn dialect(&self) -> SqlDialect;
  async fn init_schema(&self) -> Result<()>;
  async fn insert(&self, collection: &str, data: Value) -> Result<Document>;
  async fn get(&self, collection: &str, id: Uuid) -> Result<Option<Document>>;
  async fn update(&self, collection: &str, id: Uuid, data: Value) -> Result<Option<Document>>;
  async fn delete(&self, collection: &str, id: Uuid) -> Result<Option<Document>>;
  async fn list(&self, collection: &str, filter: Option<&str>,
                order: Option<&OrderBy>, limit: Option<usize>) -> Result<Vec<Document>>;
  async fn list_collections(&self) -> Result<Vec<String>>;
  async fn start_change_listener(&self) -> Result<()>;
  fn subscribe_changes(&self) -> broadcast::Receiver<Change>;

  // Token management
  async fn create_token(&self, name: &str, hash: &str) -> Result<ApiTokenInfo>;
  async fn delete_token(&self, id: Uuid) -> Result<bool>;
  async fn list_tokens(&self) -> Result<Vec<ApiTokenInfo>>;
  async fn validate_token(&self, hash: &str) -> Result<bool>;
}
```

## Data Flow

### Query Execution

```
Client Query Message
        │
        ▼
┌───────────────────┐
│  Message Handler  │
│  - Parse JSON     │
│  - Extract query  │
└───────────────────┘
        │
        ▼
┌───────────────────┐
│  Query Engine     │
│  - Parse JS       │
│  - Analyze AST    │
│  - Compile to SQL │
└───────────────────┘
        │
        ▼
┌───────────────────┐
│  Database Backend │
│  - Execute query  │
│  - Return results │
└───────────────────┘
        │
        ▼
   Client Response
```

### Subscription Flow

```
Subscribe Request                     Data Change
        │                                  │
        ▼                                  ▼
┌───────────────────┐           ┌───────────────────┐
│  Subscription     │           │  Change Listener  │
│  Manager          │◄──────────│  (LISTEN/NOTIFY)  │
│  - Register sub   │           └───────────────────┘
│  - Store filters  │                    │
└───────────────────┘                    │
        │                                ▼
        │                       ┌───────────────────┐
        │                       │  Match Changes    │
        │                       │  - Filter match   │
        │                       │  - Client lookup  │
        │                       └───────────────────┘
        │                                │
        └──────────────────┬─────────────┘
                           ▼
                  ┌───────────────────┐
                  │  Broadcast to     │
                  │  Subscribed       │
                  │  Clients          │
                  └───────────────────┘
```

## Module Structure

```
src/
├── lib.rs                    # Library entry, re-exports
├── bin/
│   ├── sqrld.rs              # Server binary
│   └── sqrl.rs               # CLI binary
├── types/
│   ├── mod.rs                # Type module
│   ├── document.rs           # Document struct
│   ├── change.rs             # Change, ChangeOperation
│   ├── query.rs              # QuerySpec, FilterSpec
│   └── protocol.rs           # ClientMessage, ServerMessage
├── db/
│   ├── mod.rs                # DatabaseBackend trait
│   ├── postgres.rs           # PostgreSQL implementation
│   ├── sqlite.rs             # SQLite implementation
│   ├── schema.rs             # Schema initialization
│   └── changes.rs            # Change listener
├── server/
│   ├── mod.rs                # Server module
│   ├── config.rs             # Configuration structs
│   ├── daemon.rs             # Main daemon
│   ├── handler.rs            # Message handler
│   └── websocket.rs          # WebSocket server
├── query/
│   ├── mod.rs                # Query module
│   ├── engine.rs             # Query engine
│   └── compiler.rs           # JS to SQL compiler
├── subscriptions/
│   ├── mod.rs                # Subscriptions module
│   └── manager.rs            # Subscription manager
├── admin/
│   ├── mod.rs                # Admin module
│   ├── api.rs                # REST API, auth
│   ├── app.rs                # Leptos UI components
│   ├── client.js             # Client-side JavaScript
│   └── styles.css            # CSS styles
└── client/
    ├── mod.rs                # Client module
    ├── connection.rs         # WebSocket client
    ├── repl.rs               # Interactive REPL
    └── commands.rs           # CLI commands
```

## Configuration System

Configuration is loaded from YAML:

```rust
#[derive(Deserialize)]
pub struct ServerConfig {
  pub server: ServerSection,
  pub database: DatabaseSection,
  pub auth: AuthSection,
}

#[derive(Deserialize)]
pub struct ServerSection {
  pub host: String,
  pub port: u16,
  pub admin_port: u16,
  pub protocols: ProtocolsSection,
}
```

Priority:
1. Command-line arguments
2. Environment variables
3. Config file
4. Defaults

## Error Handling

Errors use `anyhow` for context:

```rust
use anyhow::{Context, Result};

async fn insert(&self, collection: &str, data: Value) -> Result<Document> {
  self.pool
    .execute(query)
    .await
    .context("Failed to insert document")?;
  Ok(document)
}
```

API errors map to HTTP status codes:

```rust
enum AppError {
  Internal(anyhow::Error),  // 500
  NotFound,                  // 404
  BadRequest(String),        // 400
}
```

## Concurrency Model

### Async Runtime

Uses Tokio for async I/O:

```rust
#[tokio::main]
async fn main() -> Result<()> {
  let daemon = Daemon::new(config, backend);
  daemon.run().await
}
```

### Shared State

State is shared via `Arc`:

```rust
pub struct AppState {
  pub backend: Arc<dyn DatabaseBackend>,
  pub subs: Arc<SubscriptionManager>,
  pub engine: Arc<Mutex<QueryEngine>>,
}
```

### Synchronization

- `parking_lot::Mutex` for query engine (sync)
- `tokio::sync::RwLock` for subscription state
- `broadcast` channels for events

## Security Architecture

### Authentication Flow

```
Request with Token
        │
        ▼
┌───────────────────┐
│  Extract Token    │
│  - Header         │
│  - Query param    │
└───────────────────┘
        │
        ▼
┌───────────────────┐
│  Hash Token       │
│  (SHA-256)        │
└───────────────────┘
        │
        ▼
┌───────────────────┐
│  Validate in DB   │
│  - Compare hash   │
│  - Return bool    │
└───────────────────┘
        │
        ▼
   Allow/Deny
```

### Token Storage

```sql
CREATE TABLE api_tokens (
  id UUID PRIMARY KEY DEFAULT uuid(),
  name TEXT NOT NULL UNIQUE,
  token_hash TEXT NOT NULL,  -- SHA-256
  created_at TIMESTAMP
);
```

Note: The `uuid()` function is a built-in alias for `gen_random_uuid()` that provides a JavaScript-friendly name.

## Performance Considerations

### Query Optimization

1. **SQL Compilation**: Simple filters compile to SQL for database-level execution
2. **JS Fallback**: Complex expressions use embedded JavaScript
3. **Indexing**: JSONB GIN indexes for PostgreSQL

### Connection Pooling

PostgreSQL uses `deadpool-postgres`:

```rust
let pool = Pool::builder(manager)
  .max_size(16)
  .build()?;
```

### Change Notification

PostgreSQL uses LISTEN/NOTIFY for efficient change detection:

```sql
CREATE OR REPLACE FUNCTION notify_change()
RETURNS TRIGGER AS $$
BEGIN
  PERFORM pg_notify('doc_changes', ...);
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

## Testing Strategy

### Unit Tests

In `tests/` directory:

- `types.rs` - Type serialization
- `protocol.rs` - Message format
- `compiler.rs` - SQL compilation
- `query_engine.rs` - Query parsing
- `config.rs` - Configuration
- `sqlite_backend.rs` - SQLite operations

### Integration Tests

- Full request/response cycles
- Database operations
- WebSocket communication

### Benchmarks

Planned for critical paths:

- Query parsing
- SQL compilation
- Document serialization

## Extension Points

### Custom Backends

Implement `DatabaseBackend` trait:

```rust
pub struct MyBackend { /* ... */ }

#[async_trait]
impl DatabaseBackend for MyBackend {
  // Implement all methods
}
```

### Query Functions

Add to query engine:

```rust
// In query/engine.rs
ctx.globals().set("myFunction", js_function)?;
```

### Middleware

Add Axum middleware:

```rust
app = app.layer(my_middleware_layer);
```

## Future Architecture

### Planned Improvements

1. **Clustering**: Multi-node support with distributed subscriptions
2. **Caching**: Query result caching layer
3. **Streaming**: SSE support for one-way updates
4. **Plugins**: Dynamic plugin loading
5. **Metrics**: Prometheus metrics export
