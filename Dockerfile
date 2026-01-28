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

# Copy example config and migrations
COPY squirreldb.example.yaml /app/squirreldb.example.yaml
COPY migrations /app/migrations

# Create non-root user
RUN useradd -r -s /bin/false squirrel && \
  mkdir -p /app/data && \
  chown -R squirrel:squirrel /app

USER squirrel

# WebSocket port
EXPOSE 8080
# Admin UI port
EXPOSE 8081
# Storage port
EXPOSE 9000

ENV RUST_LOG=info

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD sqrl status || exit 1

CMD ["sqrld"]
