# Server Configuration

SquirrelDB can be configured via YAML file, environment variables, or command-line arguments.

## Configuration File

Create a `squirreldb.yaml` file in your working directory:

```yaml
# Backend: postgres or sqlite
backend: postgres

server:
  host: "0.0.0.0"
  port: 8080
  admin_port: 8081

postgres:
  url: "postgres://localhost/squirreldb"
  max_connections: 20

sqlite:
  path: "squirreldb.db"

logging:
  level: "info"
```

SquirrelDB automatically looks for:
1. `squirreldb.yaml`
2. `squirreldb.yml`

Or specify a path explicitly: `sqrld --config /path/to/config.yaml`

## Environment Variables

Use `$VAR` or `${VAR}` syntax in your config file to reference environment variables:

```yaml
backend: postgres

postgres:
  url: $DATABASE_URL
  max_connections: ${POOL_SIZE}

logging:
  level: $LOG_LEVEL
```

This is the recommended approach for production deployments where secrets should not be in config files.

## Configuration Options

### Server Section

| Option | Default | Description |
|--------|---------|-------------|
| `server.host` | `0.0.0.0` | Bind address for both servers |
| `server.port` | `8080` | WebSocket server port |
| `server.admin_port` | `8081` | Admin UI HTTP port |

### Backend Selection

| Option | Default | Description |
|--------|---------|-------------|
| `backend` | `postgres` | Backend type: `postgres` or `sqlite` |

### PostgreSQL Section

| Option | Default | Description |
|--------|---------|-------------|
| `postgres.url` | `postgres://localhost/squirreldb` | PostgreSQL connection URL |
| `postgres.max_connections` | `20` | Connection pool size |

Connection URL format:
```
postgres://[user[:password]@][host][:port][/database][?param=value]
```

Examples:
```yaml
postgres:
  # Local development
  url: "postgres://localhost/squirreldb"

  # With credentials
  url: "postgres://myuser:mypass@localhost/squirreldb"

  # Remote server with SSL
  url: "postgres://user:pass@db.example.com:5432/squirreldb?sslmode=require"
```

### SQLite Section

| Option | Default | Description |
|--------|---------|-------------|
| `sqlite.path` | `squirreldb.db` | Path to SQLite database file |

Examples:
```yaml
sqlite:
  # Relative path
  path: "squirreldb.db"

  # Absolute path
  path: "/var/lib/squirreldb/data.db"

  # In-memory (for testing)
  path: ":memory:"
```

### Logging Section

| Option | Default | Description |
|--------|---------|-------------|
| `logging.level` | `info` | Log level: `debug`, `info`, `warn`, `error` |

## Command-Line Arguments

CLI arguments override config file settings:

```bash
sqrld [OPTIONS]

Options:
      --pg-url <URL>       PostgreSQL connection URL
      --sqlite <PATH>      SQLite database path
  -p, --port <PORT>        WebSocket server port
      --host <HOST>        Bind address
  -c, --config <PATH>      Config file path
      --log-level <LEVEL>  Log level (debug, info, warn, error)
  -h, --help               Print help
  -V, --version            Print version
```

### Examples

```bash
# Use SQLite
sqrld --sqlite ./mydata.db

# Use PostgreSQL
sqrld --pg-url postgres://user:pass@localhost/mydb

# Custom ports
sqrld --port 9000

# Verbose logging
sqrld --log-level debug

# Combine options
sqrld --sqlite ./data.db --port 9000 --log-level debug
```

## Environment Variable Overrides

Some settings can be set directly via environment variables:

| Variable | Description |
|----------|-------------|
| `SQUIRRELDB_PG_URL` | PostgreSQL connection URL |
| `SQUIRRELDB_SQLITE_PATH` | SQLite database path |
| `RUST_LOG` | Log level (standard Rust logging) |

## Configuration Precedence

Settings are applied in this order (later overrides earlier):

1. **Default values**
2. **Config file** (`squirreldb.yaml`)
3. **Environment variables** (in config file)
4. **Command-line arguments**

## Example Configurations

### Development (SQLite)

```yaml
backend: sqlite

server:
  host: "127.0.0.1"
  port: 8080
  admin_port: 8081

sqlite:
  path: "dev.db"

logging:
  level: "debug"
```

### Production (PostgreSQL)

```yaml
backend: postgres

server:
  host: "0.0.0.0"
  port: 8080
  admin_port: 8081

postgres:
  url: $DATABASE_URL
  max_connections: 50

logging:
  level: "info"
```

### Docker/Kubernetes

```yaml
backend: postgres

server:
  host: "0.0.0.0"
  port: 8080
  admin_port: 8081

postgres:
  url: $DATABASE_URL
  max_connections: ${POOL_SIZE}

logging:
  level: $LOG_LEVEL
```

## Validating Configuration

Start the server with debug logging to see the loaded configuration:

```bash
sqrld --log-level debug
```

Look for:
```
DEBUG squirreldb: Loading config from squirreldb.yaml
DEBUG squirreldb: Backend: Postgres
DEBUG squirreldb: WebSocket port: 8080
DEBUG squirreldb: Admin port: 8081
```
