# SquirrelDB Documentation

**SquirrelDB** is a real-time document database inspired by RethinkDB. It provides a familiar query language, real-time change feeds, and supports both PostgreSQL and SQLite backends.

## Features

- **Real-time subscriptions** - Subscribe to changes on any query and receive updates instantly
- **Familiar query language** - RethinkDB-inspired chainable query syntax
- **Multiple backends** - Use PostgreSQL for production or SQLite for development/embedded use
- **WebSocket protocol** - Simple JSON-based protocol over WebSocket
- **Admin UI** - Built-in web interface for database management
- **Multi-language SDKs** - Official clients for TypeScript, Python, Ruby, and Elixir

## Quick Example

```javascript
import { SquirrelDB } from "squirreldb";

// Connect to the database
const db = await SquirrelDB.connect("localhost:8080");

// Insert a document
const user = await db.insert("users", { name: "Alice", age: 30 });

// Query documents
const users = await db.query('db.table("users").filter(r => r.age > 25).run()');

// Subscribe to changes
await db.subscribe('db.table("users").changes()', (change) => {
  console.log("User changed:", change);
});
```

## Documentation

### Getting Started

- [Installation](./getting-started/installation.md) - Install and run SquirrelDB
- [Quick Start](./getting-started/quickstart.md) - Your first queries in 5 minutes
- [Concepts](./getting-started/concepts.md) - Core concepts and terminology

### Configuration

- [Server Configuration](./configuration/server.md) - Configure the SquirrelDB server
- [PostgreSQL Backend](./configuration/postgres.md) - PostgreSQL-specific settings
- [SQLite Backend](./configuration/sqlite.md) - SQLite-specific settings
- [Authentication](./configuration/authentication.md) - API token authentication
- [Protocols](./configuration/protocols.md) - REST, WebSocket, and SSE configuration

### Query Language

- [Query Overview](./queries/overview.md) - Introduction to the query language
- [Reading Data](./queries/reading.md) - Selecting and filtering documents
- [Writing Data](./queries/writing.md) - Insert, update, and delete operations
- [Subscriptions](./queries/subscriptions.md) - Real-time change feeds

### SDKs

- [TypeScript/JavaScript](./sdks/typescript.md) - Official TypeScript SDK
- [Python](./sdks/python.md) - Official Python SDK
- [Ruby](./sdks/ruby.md) - Official Ruby SDK
- [Elixir](./sdks/elixir.md) - Official Elixir SDK

### Operations

- [Admin UI](./operations/admin-ui.md) - Using the web administration interface
- [Console (REPL)](./operations/console.md) - Interactive query console
- [Live Logs](./operations/logs.md) - Real-time log streaming
- [Settings](./operations/settings.md) - Configuration and token management
- [Deployment](./operations/deployment.md) - Production deployment guide
- [Monitoring](./operations/monitoring.md) - Health checks and observability
- [Backup & Restore](./operations/backup.md) - Data backup strategies

### Reference

- [WebSocket Protocol](./reference/protocol.md) - Low-level protocol specification
- [REST API](./reference/rest-api.md) - Admin HTTP API reference
- [CLI Reference](./reference/cli.md) - Command-line tools

### Architecture

- [Architecture Overview](./architecture/overview.md) - Internal architecture and design

### Development

- [Development Guide](./development/guide.md) - Contributing and development setup

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Clients                               │
│  (TypeScript, Python, Ruby, Elixir, or raw WebSocket)       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    SquirrelDB Server                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  WebSocket  │  │  Admin UI   │  │  Subscription       │  │
│  │  Server     │  │  (HTTP)     │  │  Manager            │  │
│  │  :8080      │  │  :8081      │  │                     │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
│                              │                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                   Query Engine                         │  │
│  │  (JavaScript evaluation + SQL compilation)            │  │
│  └───────────────────────────────────────────────────────┘  │
│                              │                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │               Database Backend (Trait)                 │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
┌─────────────────────────┐     ┌─────────────────────────┐
│       PostgreSQL        │     │         SQLite          │
│   (Production/Scale)    │     │   (Dev/Embedded)        │
└─────────────────────────┘     └─────────────────────────┘
```

## Source Code

SquirrelDB is open source. Visit the [GitHub repository](https://github.com/squirreldb/squirreldb) for source code, issues, and contributions.

## License

MIT License
