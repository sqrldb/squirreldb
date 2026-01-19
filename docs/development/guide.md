# Development Guide

This guide covers setting up a development environment, coding conventions, testing, and contributing to SquirrelDB.

## Prerequisites

### Required Tools

- **Rust** 1.90.0+ (see `rust-toolchain.toml`)
- **PostgreSQL** 15+ (for testing with Postgres backend)
- **SQLite** 3.35+ (included in most systems)

### Recommended Tools

- **rust-analyzer** - IDE support
- **cargo-watch** - Auto-rebuild on changes
- **cargo-nextest** - Faster test runner

## Getting Started

### Clone Repository

```bash
git clone https://github.com/squirreldb/squirreldb.git
cd squirreldb
```

### Install Rust Toolchain

```bash
# Uses version from rust-toolchain.toml
rustup show
```

### Build Project

```bash
# Debug build
cargo build

# Release build
cargo build --release
```

### Run Server

```bash
# With SQLite (default)
cargo run --bin sqrld

# With custom config
cargo run --bin sqrld -- --config squirreldb.yaml
```

### Run Tests

```bash
# All tests
cargo test

# Specific test file
cargo test --test sqlite_backend

# With output
cargo test -- --nocapture
```

## Project Structure

```
squirreldb/
├── src/
│   ├── lib.rs              # Library crate
│   ├── bin/
│   │   ├── sqrld.rs        # Server binary
│   │   └── sqrl.rs         # CLI binary
│   ├── types/              # Core data types
│   ├── db/                 # Database backends
│   ├── server/             # Server components
│   ├── query/              # Query engine
│   ├── subscriptions/      # Real-time subscriptions
│   ├── admin/              # Admin UI & API
│   └── client/             # CLI client
├── tests/                  # Integration tests
├── docs/                   # Documentation
├── migrations/             # SQL migrations
└── Cargo.toml              # Dependencies
```

## Code Style

### Formatting

Code is formatted with `rustfmt`:

```bash
cargo fmt
```

Configuration in `rustfmt.toml`:

```toml
edition = "2024"
tab_spaces = 2
use_field_init_shorthand = true
use_try_shorthand = true
```

### Linting

Use `clippy` for linting:

```bash
cargo clippy
```

### Style Guidelines

1. **Functional Style**: Prefer functions over classes
2. **Small Files**: Keep modules focused
3. **Field Init Shorthand**: Use `Struct { field }` not `Struct { field: field }`
4. **Try Shorthand**: Use `?` operator
5. **2-Space Indent**: Consistent indentation

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Functions | snake_case | `parse_query` |
| Types | PascalCase | `QuerySpec` |
| Constants | SCREAMING_SNAKE | `MAX_RESULTS` |
| Modules | snake_case | `query_engine` |

## Adding Features

### 1. New Query Function

Add to `src/query/engine.rs`:

```rust
// Register new function
ctx.globals().set("myFunction", Func::new(|value: Value| {
  // Implementation
  Ok(result)
}))?;
```

Add tests to `tests/query_engine.rs`:

```rust
#[test]
fn test_my_function() {
  let engine = QueryEngine::new(SqlDialect::Sqlite);
  let spec = engine.parse_query("db.table(\"t\").myFunction()").unwrap();
  // Assertions
}
```

### 2. New API Endpoint

Add to `src/admin/api.rs`:

```rust
// Handler function
async fn my_endpoint(
  State(state): State<AppState>,
) -> Result<Json<Response>, AppError> {
  // Implementation
}

// Register in router
app = app.route("/api/my-endpoint", get(my_endpoint));
```

Add tests to `tests/api.rs`:

```rust
#[tokio::test]
async fn test_my_endpoint() {
  let client = TestClient::new();
  let response = client.get("/api/my-endpoint").await;
  assert_eq!(response.status(), 200);
}
```

### 3. New Database Backend

Create `src/db/mybackend.rs`:

```rust
pub struct MyBackend {
  // Fields
}

#[async_trait]
impl DatabaseBackend for MyBackend {
  fn dialect(&self) -> SqlDialect {
    SqlDialect::Custom
  }

  async fn init_schema(&self) -> Result<()> {
    // Implementation
  }

  // Implement all trait methods
}
```

Register in `src/db/mod.rs`:

```rust
mod mybackend;
pub use mybackend::MyBackend;
```

### 4. New UI Page

Add component to `src/admin/app.rs`:

```rust
#[component]
fn MyPage() -> impl IntoView {
  view! {
    <section id="my-page" class="page">
      <div class="page-header">
        <h2>"My Page"</h2>
      </div>
      // Content
    </section>
  }
}
```

Add navigation link:

```rust
<a href="#my-page" class="nav-link" onclick="showPage('my-page')">
  // Icon
  "My Page"
</a>
```

Add JavaScript in `src/admin/client.js`:

```javascript
// Page-specific functionality
function myPageFunction() {
  // Implementation
}
```

Add styles in `src/admin/styles.css`:

```css
#my-page .my-element {
  /* Styles */
}
```

## Testing

### Unit Tests

Located in same file or `tests/`:

```rust
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_something() {
    assert_eq!(2 + 2, 4);
  }
}
```

### Integration Tests

In `tests/` directory:

```rust
// tests/myfeature.rs
use squirreldb::db::SqliteBackend;

#[tokio::test]
async fn test_integration() {
  let backend = SqliteBackend::in_memory().await.unwrap();
  backend.init_schema().await.unwrap();
  // Test
}
```

### Test Conventions

1. **Test Names**: `test_<feature>_<scenario>`
2. **One Assert Per Test**: Ideally
3. **Use Fixtures**: Create helper functions
4. **Async Tests**: Use `#[tokio::test]`

### Running Specific Tests

```bash
# By name pattern
cargo test test_sqlite

# By file
cargo test --test sqlite_backend

# Single test
cargo test test_sqlite_backend_insert

# Show output
cargo test -- --nocapture

# Run ignored tests
cargo test -- --ignored
```

### Test Coverage

```bash
# Install coverage tool
cargo install cargo-tarpaulin

# Run coverage
cargo tarpaulin --out Html
```

## Debugging

### Logging

Use `tracing` for logging:

```rust
use tracing::{info, debug, error};

info!("Server starting on {}", addr);
debug!("Query parsed: {:?}", spec);
error!("Failed to connect: {}", e);
```

Run with logging:

```bash
RUST_LOG=debug cargo run --bin sqrld
RUST_LOG=squirreldb=trace cargo run --bin sqrld
```

### Debugging with lldb/gdb

```bash
# Build with debug info
cargo build

# Debug
lldb target/debug/sqrld
```

### VS Code Launch Config

`.vscode/launch.json`:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug sqrld",
      "cargo": {
        "args": ["build", "--bin=sqrld"]
      },
      "args": [],
      "cwd": "${workspaceFolder}"
    }
  ]
}
```

## Documentation

### Code Documentation

Use rustdoc comments:

```rust
/// Parses a query string into a QuerySpec.
///
/// # Arguments
///
/// * `query` - JavaScript query string
///
/// # Returns
///
/// Returns `Ok(QuerySpec)` on success or an error.
///
/// # Example
///
/// ```
/// let spec = engine.parse_query("db.table(\"users\").run()")?;
/// ```
pub fn parse_query(&self, query: &str) -> Result<QuerySpec> {
  // Implementation
}
```

Generate docs:

```bash
cargo doc --open
```

### User Documentation

In `docs/` directory using Markdown.

### Documentation Conventions

1. Include examples
2. Document errors
3. Cross-reference related items
4. Keep up-to-date with code

## Contributing

### Workflow

1. Fork repository
2. Create feature branch
3. Write code and tests
4. Run `cargo fmt` and `cargo clippy`
5. Submit pull request

### Commit Messages

Format:

```
type: short description

Longer description if needed.

Fixes #123
```

Types:
- `feat` - New feature
- `fix` - Bug fix
- `docs` - Documentation
- `test` - Tests
- `refactor` - Refactoring
- `chore` - Maintenance

### Pull Request Checklist

- [ ] Code compiles without warnings
- [ ] Tests pass
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings
- [ ] Documentation updated
- [ ] Commit messages follow convention

## Release Process

### Version Bump

Update `Cargo.toml`:

```toml
[package]
version = "X.Y.Z"
```

### Changelog

Update `CHANGELOG.md`:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- Feature description

### Fixed
- Bug description
```

### Tag Release

```bash
git tag -a vX.Y.Z -m "Release X.Y.Z"
git push origin vX.Y.Z
```

## Troubleshooting Development

### Build Errors

```bash
# Clean build
cargo clean
cargo build

# Update dependencies
cargo update
```

### Test Failures

```bash
# Run single failing test with output
cargo test test_name -- --nocapture

# Check for race conditions
cargo test -- --test-threads=1
```

### Performance Issues

```bash
# Build with optimizations
cargo build --release

# Profile
cargo flamegraph --bin sqrld
```

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Axum Guide](https://docs.rs/axum)
- [SQLx Documentation](https://docs.rs/sqlx)
