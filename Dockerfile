FROM rust:1.90-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
  pkg-config \
  libssl-dev \
  ca-certificates \
  curl \
  && update-ca-certificates \
  && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests
COPY Cargo.toml Cargo.lock ./
COPY crates/types/Cargo.toml crates/types/Cargo.toml
COPY crates/client/Cargo.toml crates/client/Cargo.toml
COPY crates/sqrl/Cargo.toml crates/sqrl/Cargo.toml
COPY crates/sqrld/Cargo.toml crates/sqrld/Cargo.toml

# Create dummy sources to cache dependencies
RUN mkdir -p crates/types/src crates/client/src crates/sqrl/src crates/sqrld/src && \
  echo "pub fn dummy() {}" > crates/types/src/lib.rs && \
  echo "pub fn dummy() {}" > crates/client/src/lib.rs && \
  echo "fn main() {}" > crates/sqrl/src/main.rs && \
  echo "fn main() {}" > crates/sqrld/src/main.rs && \
  echo "pub fn dummy() {}" > crates/sqrld/src/lib.rs

# Build dependencies only
RUN cargo build --release -p sqrld -p sqrl || true
RUN rm -rf crates/*/src

# Copy actual source
COPY crates crates
COPY migrations migrations

# Build release binaries
RUN cargo build --release -p sqrld -p sqrl

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
  ca-certificates \
  libssl3 \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/sqrld /usr/local/bin/
COPY --from=builder /app/target/release/sqrl /usr/local/bin/
COPY squirreldb.docker.yaml /app/squirreldb.yaml
COPY migrations /app/migrations

RUN useradd -r -s /bin/false squirrel && \
  mkdir -p /app/data /app/data/storage && \
  chown -R squirrel:squirrel /app

USER squirrel

ENV SQRL_BACKEND=sqlite
ENV SQRL_HOST=0.0.0.0
ENV SQRL_PORT_HTTP=8080
ENV SQRL_PORT_ADMIN=8081
ENV SQRL_PORT_TCP=8082
ENV SQRL_PORT_MCP=8083
ENV SQRL_PORT_STORAGE=9000
ENV DATABASE_URL=postgres://postgres:postgres@localhost/squirreldb
ENV SQRL_PG_MAX_CONNECTIONS=20
ENV SQRL_SQLITE_PATH=/app/data/squirreldb.db
ENV SQRL_AUTH_ENABLED=false
ENV SQRL_ADMIN_TOKEN=
ENV SQRL_MCP_ENABLED=false
ENV SQRL_LIMIT_CONNECTIONS_PER_IP=100
ENV SQRL_LIMIT_RPS=100
ENV SQRL_LIMIT_BURST=50
ENV SQRL_LIMIT_QUERY_TIMEOUT=30000
ENV SQRL_LIMIT_CONCURRENT_QUERIES=10
ENV SQRL_LIMIT_MESSAGE_SIZE=16777216
ENV SQRL_STORAGE_ENABLED=false
ENV SQRL_STORAGE_PATH=/app/data/storage
ENV SQRL_STORAGE_REGION=us-east-1
ENV SQRL_CACHE_ENABLED=false
ENV SQRL_CACHE_PORT=6379
ENV SQRL_CACHE_MAX_MEMORY=256mb
ENV SQRL_CACHE_EVICTION=lru
ENV SQRL_CACHE_DEFAULT_TTL=0
ENV SQRL_CACHE_SNAPSHOT_ENABLED=false
ENV SQRL_CACHE_SNAPSHOT_PATH=/app/data/cache.snapshot
ENV SQRL_CACHE_SNAPSHOT_INTERVAL=300
ENV SQRL_LOG_LEVEL=info
ENV RUST_LOG=info

EXPOSE 8080 8081 8082 8083 9000 6379

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD sqrl status || exit 1

CMD ["sqrld"]
