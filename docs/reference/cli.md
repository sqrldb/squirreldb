# CLI Reference

SquirrelDB provides two command-line tools: `sqrld` (server) and `sqrl` (client).

## sqrld - Server

The SquirrelDB server daemon.

### Usage

```bash
sqrld [OPTIONS]
```

### Options

| Option | Environment Variable | Description |
|--------|---------------------|-------------|
| `--pg-url <URL>` | `SQUIRRELDB_PG_URL` | PostgreSQL connection URL |
| `--sqlite <PATH>` | `SQUIRRELDB_SQLITE_PATH` | SQLite database path |
| `-p, --port <PORT>` | | WebSocket server port |
| `--host <HOST>` | | Bind address |
| `-c, --config <PATH>` | | Config file path |
| `--log-level <LEVEL>` | `RUST_LOG` | Log level (debug, info, warn, error) |
| `-h, --help` | | Print help |
| `-V, --version` | | Print version |

### Examples

```bash
# Start with defaults (SQLite, port 8080)
sqrld

# Use PostgreSQL
sqrld --pg-url postgres://user:pass@localhost/mydb

# Use SQLite with custom path
sqrld --sqlite /var/lib/squirreldb/data.db

# Custom port
sqrld --port 9000

# Verbose logging
sqrld --log-level debug

# Use specific config file
sqrld --config /etc/squirreldb/config.yaml

# Combine options
sqrld --pg-url postgres://localhost/mydb --port 9000 --log-level info
```

### Configuration File

sqrld looks for configuration in:
1. Path specified by `--config`
2. `squirreldb.yaml` in current directory
3. `squirreldb.yml` in current directory

See [Server Configuration](../configuration/server.md) for file format.

### Signals

| Signal | Action |
|--------|--------|
| `SIGINT` (Ctrl+C) | Graceful shutdown |
| `SIGTERM` | Graceful shutdown |

### Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1 | Error (config, database, etc.) |

---

## sqrl - Client

Command-line client and REPL for SquirrelDB.

### Usage

```bash
sqrl [OPTIONS] [COMMAND]
```

### Global Options

| Option | Description |
|--------|-------------|
| `--host <HOST>` | Server address (default: localhost:8080) |
| `-f, --file <PATH>` | Execute queries from file |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

### Commands

#### init

Initialize the database schema.

```bash
sqrl init [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--pg-url <URL>` | PostgreSQL URL |
| `--sqlite <PATH>` | SQLite path |

```bash
# Initialize PostgreSQL
sqrl init --pg-url postgres://localhost/mydb

# Initialize SQLite
sqrl init --sqlite ./mydata.db
```

#### status

Check server status.

```bash
sqrl status
```

Output:
```
SquirrelDB v0.0.1
Backend: Postgres
Uptime: 3h 25m
```

#### listcollections

List all collections.

```bash
sqrl listcollections
```

Output:
```
users (150 documents)
posts (500 documents)
comments (1200 documents)
```

#### users

Manage PostgreSQL database users. This command provides a simple interface to create, list, and remove PostgreSQL users.

```bash
sqrl users --pg-url <URL> <COMMAND>
```

| Option | Environment Variable | Description |
|--------|---------------------|-------------|
| `--pg-url <URL>` | `DATABASE_URL` | PostgreSQL connection URL (required) |

##### users list

List all database users.

```bash
sqrl users --pg-url postgres://localhost/mydb list
```

Output:
```
USERNAME             SUPERUSER   CREATEDB      LOGIN
------------------------------------------------------
admin                      yes        yes        yes
app_user                    no         no        yes
postgres                   yes        yes        yes
```

##### users add

Add a new database user.

```bash
sqrl users --pg-url <URL> add <USERNAME> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-p, --password <PASSWORD>` | Password (prompts if not provided) |
| `--superuser` | Grant superuser privileges |
| `--createdb` | Allow user to create databases |

Examples:
```bash
# Add user (prompts for password)
sqrl users --pg-url postgres://localhost/mydb add myuser

# Add user with password
sqrl users --pg-url postgres://localhost/mydb add myuser -p secretpass

# Add user with createdb permission
sqrl users --pg-url postgres://localhost/mydb add myuser --createdb

# Add superuser
sqrl users --pg-url postgres://localhost/mydb add admin --superuser
```

##### users remove

Remove a database user.

```bash
sqrl users --pg-url <URL> remove <USERNAME>
```

Example:
```bash
sqrl users --pg-url postgres://localhost/mydb remove myuser
```

##### users passwd

Change a user's password.

```bash
sqrl users --pg-url <URL> passwd <USERNAME> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-p, --password <PASSWORD>` | New password (prompts if not provided) |

Example:
```bash
# Change password (prompts for new password)
sqrl users --pg-url postgres://localhost/mydb passwd myuser

# Change password directly
sqrl users --pg-url postgres://localhost/mydb passwd myuser -p newpassword
```

### Interactive REPL

Start without a command to enter interactive mode:

```bash
sqrl
```

```
SquirrelDB Shell (v0.0.1)
Connected to localhost:8080
Type .help for commands

squirreldb>
```

### REPL Commands

| Command | Description |
|---------|-------------|
| `.help` | Show help |
| `.exit` | Exit the REPL |
| `.quit` | Exit the REPL |
| `.collections` | List collections |
| `.clear` | Clear screen |

### Query Syntax

In the REPL, enter queries directly:

```
squirreldb> db.table("users").run()
┌──────────────────────────────────────┬────────────┬──────────────────────┐
│ id                                   │ collection │ data                 │
├──────────────────────────────────────┼────────────┼──────────────────────┤
│ 550e8400-e29b-41d4-a716-446655440000 │ users      │ {"name":"Alice",...} │
└──────────────────────────────────────┴────────────┴──────────────────────┘

squirreldb> db.table("users").filter(r => r.age > 25).run()
...

squirreldb> db.table("users").insert({ name: "Bob", age: 30 }).run()
...
```

### File Execution

Execute queries from a file:

```bash
sqrl --file queries.txt
```

queries.txt:
```javascript
db.table("users").run()
db.table("posts").filter(r => r.published == true).run()
```

Lines starting with `//` are treated as comments.

### Output Formats

The REPL displays results in a formatted table. JSON output is syntax-highlighted.

### Examples

```bash
# Connect and run a query
sqrl --host localhost:8080
squirreldb> db.table("users").run()

# Execute file against remote server
sqrl --host db.example.com:8080 --file queries.txt

# Check status of remote server
sqrl --host db.example.com:8080 status

# Initialize a new database
sqrl init --pg-url postgres://user:pass@localhost/newdb
```

---

## Environment Variables

Both tools respect these environment variables:

| Variable | Description |
|----------|-------------|
| `SQUIRRELDB_PG_URL` | Default PostgreSQL URL |
| `SQUIRRELDB_SQLITE_PATH` | Default SQLite path |
| `DATABASE_URL` | PostgreSQL URL for `sqrl users` command |
| `RUST_LOG` | Log level (debug, info, warn, error) |

---

## Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1 | General error |
| 2 | Connection error |
| 3 | Query error |

---

## Shell Completion

Generate shell completions:

```bash
# Bash
sqrld --generate-completion bash > /etc/bash_completion.d/sqrld

# Zsh
sqrld --generate-completion zsh > ~/.zsh/completions/_sqrld

# Fish
sqrld --generate-completion fish > ~/.config/fish/completions/sqrld.fish
```

(Note: Shell completion generation may be added in a future version)
