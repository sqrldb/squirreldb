# Build stage - using Alpine for better Docker Desktop SSL compatibility
FROM rust:1.84-alpine AS builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl \
    git \
    ca-certificates

# Workaround for Docker Desktop SSL issues
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV CARGO_HTTP_CHECK_REVOKE=false
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

# Copy manifests (excluding rust-toolchain.toml to use image's Rust version)
COPY Cargo.toml Cargo.lock* ./

# Create dummy src and benches to cache dependencies
RUN mkdir -p src/bin src/admin/static benches && \
    echo "fn main() {}" > src/bin/sqrld.rs && \
    echo "fn main() {}" > src/bin/sqrl.rs && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > benches/database.rs && \
    echo "fn main() {}" > benches/query_engine.rs

# Build dependencies only (keep dummy benches for final build)
RUN cargo build --release && rm -rf src

# Copy actual source
COPY src src
COPY migrations migrations

# Touch to invalidate cache for main files
RUN touch src/lib.rs src/bin/sqrld.rs src/bin/sqrl.rs

# Build release binaries
RUN cargo build --release

# Runtime stage
FROM alpine:3.19

RUN apk add --no-cache ca-certificates libgcc

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /app/target/release/sqrld /usr/local/bin/
COPY --from=builder /app/target/release/sqrl /usr/local/bin/

# Copy example config and migrations
COPY squirreldb.example.yaml /app/squirreldb.example.yaml
COPY migrations /app/migrations

# Create non-root user
RUN adduser -D -s /bin/false squirrel && \
    mkdir -p /app/data && \
    chown -R squirrel:squirrel /app

USER squirrel

# WebSocket port
EXPOSE 8080
# Admin UI port
EXPOSE 8081

ENV RUST_LOG=info

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD sqrl status || exit 1

CMD ["sqrld"]
