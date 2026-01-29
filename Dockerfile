FROM rust:1.90-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies and update CA certificates
RUN apt-get update && apt-get install -y --no-install-recommends \
  pkg-config \
  libssl-dev \
  ca-certificates \
  curl \
  && update-ca-certificates \
  && rm -rf /var/lib/apt/lists/*

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock* ./

# Create dummy src and benches to cache dependencies
RUN mkdir -p src/bin src/admin/static benches && \
  echo "fn main() {}" > src/bin/sqrld.rs && \
  echo "fn main() {}" > src/bin/sqrl.rs && \
  echo "pub fn dummy() {}" > src/lib.rs && \
  echo "fn main() {}" > benches/database.rs && \
  echo "fn main() {}" > benches/query_engine.rs

# Build dependencies only (this layer is cached unless Cargo.toml changes)
RUN cargo build --release && rm -rf src

# Copy actual source
COPY src src
COPY migrations migrations

# Touch to invalidate cache for main files
RUN touch src/lib.rs src/bin/sqrld.rs src/bin/sqrl.rs

# Build release binaries
RUN cargo build --release

# Runtime stage - use Debian slim for glibc compatibility
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
  ca-certificates \
  libssl3 \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /app/target/release/sqrld /usr/local/bin/
COPY --from=builder /app/target/release/sqrl /usr/local/bin/

# Copy Docker config and migrations
COPY squirreldb.docker.yaml /app/squirreldb.yaml
COPY migrations /app/migrations

# Create non-root user and data directories
RUN useradd -r -s /bin/false squirrel && \
  mkdir -p /app/data /app/data/storage && \
  chown -R squirrel:squirrel /app

USER squirrel

# =============================================================================
# Environment Variables - Override these when running the container
# =============================================================================

# Backend: postgres or sqlite
ENV SQRL_BACKEND=sqlite

# Server binding
ENV SQRL_HOST=0.0.0.0

# Ports
ENV SQRL_PORT_HTTP=8080
ENV SQRL_PORT_ADMIN=8081
ENV SQRL_PORT_TCP=8082
ENV SQRL_PORT_MCP=8083
ENV SQRL_PORT_STORAGE=9000

# PostgreSQL (when SQRL_BACKEND=postgres)
ENV DATABASE_URL=postgres://postgres:postgres@localhost/squirreldb
ENV SQRL_PG_MAX_CONNECTIONS=20

# SQLite (when SQRL_BACKEND=sqlite)
ENV SQRL_SQLITE_PATH=/app/data/squirreldb.db

# Authentication
ENV SQRL_AUTH_ENABLED=false
ENV SQRL_ADMIN_TOKEN=

# MCP protocol
ENV SQRL_MCP_ENABLED=false

# Rate limits
ENV SQRL_LIMIT_CONNECTIONS_PER_IP=100
ENV SQRL_LIMIT_RPS=100
ENV SQRL_LIMIT_BURST=50
ENV SQRL_LIMIT_QUERY_TIMEOUT=30000
ENV SQRL_LIMIT_CONCURRENT_QUERIES=10
ENV SQRL_LIMIT_MESSAGE_SIZE=16777216

# Object Storage (S3-compatible)
ENV SQRL_STORAGE_ENABLED=false
ENV SQRL_STORAGE_PATH=/app/data/storage
ENV SQRL_STORAGE_REGION=us-east-1

# Logging
ENV SQRL_LOG_LEVEL=info
ENV RUST_LOG=info

# =============================================================================
# Exposed Ports
# =============================================================================
# HTTP/WebSocket API
EXPOSE 8080
# Admin UI
EXPOSE 8081
# TCP protocol
EXPOSE 8082
# MCP protocol
EXPOSE 8083
# S3-compatible storage
EXPOSE 9000

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD sqrl status || exit 1

CMD ["sqrld"]
