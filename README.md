# SquirrelDB

A real-time document database built in Rust with PostgreSQL and SQLite backends.

## Features

- **Real-time Subscriptions** - Subscribe to changes with live query updates
- **JavaScript Query Engine** - Write queries using familiar JavaScript syntax
- **Multiple Backends** - SQLite for development, PostgreSQL for production
- **WebSocket & TCP** - Connect via WebSocket or high-performance TCP protocol
- **Built-in Admin UI** - Monitor and manage your database

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
  websocket_port: 8080
  tcp_port: 9000
  admin_port: 8081

database:
  backend: sqlite
  path: "./data/squirreldb.db"

logging:
  level: info
  format: pretty
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
