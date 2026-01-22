# SquirrelDB

A real-time document database built in Rust with PostgreSQL and SQLite backends.

## Features

- **Real-time Subscriptions** - Subscribe to changes with live query updates
- **JavaScript Query Engine** - Write queries using familiar JavaScript syntax
- **Multiple Backends** - SQLite for development, PostgreSQL for production
- **WebSocket & TCP** - Connect via WebSocket or high-performance TCP protocol
- **Built-in Admin UI** - Monitor and manage your database
- **MCP Server** - Model Context Protocol integration for AI assistants like Claude

## Quick Start

### Using Docker

```bash
docker run -p 8080:8080 -p 8081:8081 sqrldb/squirreldb:latest
```

### From Source

```bash
git clone https://github.com/sqrldb/squirreldb.git
cd squirreldb
cargo build --release
./target/release/sqrld
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
  protocols:
    websocket: true
    tcp: true
    mcp: false      # Enable for MCP SSE

backend: sqlite     # or postgres

sqlite:
  path: "./data/squirreldb.db"

postgres:
  url: "postgres://localhost/squirreldb"
  max_connections: 20

logging:
  level: info
```

## MCP Server (Model Context Protocol)

SquirrelDB includes a built-in MCP server for AI assistant integration.

### Tools Available

| Tool | Description |
|------|-------------|
| `query` | Execute JavaScript queries |
| `insert` | Insert a document into a collection |
| `update` | Update a document by ID |
| `delete` | Delete a document by ID |
| `list_collections` | List all collections |

### stdio Mode (Claude Desktop)

Run the MCP server over stdio for Claude Desktop integration:

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

### SSE Mode (HTTP)

Enable in config and access via HTTP SSE at `http://localhost:8083/mcp`:

```yaml
server:
  protocols:
    mcp: true
```

## Documentation

Visit [squirreldb.com](https://squirreldb.com) for full documentation.

## Client SDKs

- [TypeScript/JavaScript](https://github.com/sqrldb/sdk-typescript)
- [Python](https://github.com/sqrldb/sdk-python)
- [Go](https://github.com/sqrldb/sdk-go)
- [Rust](https://github.com/sqrldb/sdk-rust)

## License

Apache License 2.0 - see [LICENSE](LICENSE) for details.
