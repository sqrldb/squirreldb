# Installation

This guide covers how to install and run SquirrelDB.

## Requirements

- **PostgreSQL 14+** or **SQLite 3.35+** (database backend)
- **Rust 1.75+** (only if building from source)

## Installation Methods

### From crates.io (Recommended)

```bash
# Install the server
cargo install sqrld

# Install the CLI client
cargo install sqrl
```

### From Source

```bash
# Clone the repository
git clone https://github.com/squirreldb/squirreldb.git
cd squirreldb

# Build release binaries
cargo build --release

# Binaries are in target/release/
./target/release/sqrld --help
./target/release/sqrl --help
```

### Using Docker

```bash
# Pull the image
docker pull squirreldb/squirreldb:latest

# Run with SQLite (for testing)
docker run -p 8080:8080 -p 8081:8081 squirreldb/squirreldb

# Run with PostgreSQL
docker run -p 8080:8080 -p 8081:8081 \
  -e DATABASE_URL=postgres://user:pass@host/dbname \
  squirreldb/squirreldb
```

### Docker Compose

```yaml
version: '3.8'

services:
  squirreldb:
    image: squirreldb/squirreldb:latest
    ports:
      - "8080:8080"   # WebSocket
      - "8081:8081"   # Admin UI
    environment:
      - DATABASE_URL=postgres://postgres:postgres@db/squirreldb
    depends_on:
      - db

  db:
    image: postgres:16-alpine
    environment:
      - POSTGRES_USER=postgres
      - POSTGRES_PASSWORD=postgres
      - POSTGRES_DB=squirreldb
    volumes:
      - pgdata:/var/lib/postgresql/data

volumes:
  pgdata:
```

## Binaries

SquirrelDB provides two binaries:

| Binary | Description |
|--------|-------------|
| `sqrld` | The SquirrelDB server daemon |
| `sqrl` | Command-line client and REPL |

## Verifying Installation

Start the server:

```bash
# With SQLite (default, creates squirreldb.db)
sqrld

# With PostgreSQL
sqrld --pg-url postgres://localhost/squirreldb
```

You should see:

```
INFO squirreldb: SQLite schema initialized
INFO squirreldb: Admin UI at http://0.0.0.0:8081
INFO squirreldb: SquirrelDB WebSocket on 0.0.0.0:8080
```

Open http://localhost:8081 in your browser to access the Admin UI.

## Installing SDKs

### TypeScript/JavaScript

```bash
# Using npm
npm install squirreldb-sdk

# Using bun
bun add squirreldb-sdk

# Using yarn
yarn add squirreldb-sdk
```

### Python

```bash
pip install squirreldb-sdk
```

### Rust

```bash
cargo add squirreldb-sdk
```

### Go

```bash
go get github.com/squirreldb/squirreldb-sdk-go
```

### Ruby

```bash
gem install squirreldb-sdk
```

### Elixir

Add to your `mix.exs`:

```elixir
defp deps do
  [
    {:squirreldb_sdk, "~> 0.1.0"}
  ]
end
```

## Next Steps

- [Quick Start Guide](./quickstart.md) - Run your first queries
- [Server Configuration](../configuration/server.md) - Configure SquirrelDB for your needs
