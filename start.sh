#!/bin/bash
set -e

# Check for trunk, install if missing
if ! command -v trunk &> /dev/null; then
  echo "Installing trunk..."
  cargo install trunk
fi

# Add wasm32 target if missing
if ! rustup target list --installed | grep -q wasm32-unknown-unknown; then
  echo "Adding wasm32-unknown-unknown target..."
  rustup target add wasm32-unknown-unknown
fi

echo "Building WASM admin panel..."
trunk build

echo ""
echo "Starting SquirrelDB server..."
echo "Admin UI: http://localhost:8080"
cargo run --bin sqrld
