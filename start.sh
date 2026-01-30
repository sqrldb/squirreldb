#!/bin/bash

DEV_MODE=false

# Handle Ctrl+C - exit immediately
trap 'echo ""; echo "Shutting down..."; exit 0' INT TERM

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --dev)
      DEV_MODE=true
      shift
      ;;
    *)
      shift
      ;;
  esac
done

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
echo ""
echo "  WebSocket API:  http://localhost:8080"
echo "  Admin UI:       http://localhost:8081"
echo "  TCP Protocol:   localhost:8082"

if [ "$DEV_MODE" = true ]; then
  echo "  S3 Storage:     http://localhost:9000"
  echo "  Redis Cache:    localhost:6379"
  echo ""
  echo "Dev mode: storage and caching enabled"

  # Restart loop for dev mode
  while true; do
    SQRL_STORAGE_ENABLED=true SQRL_CACHE_ENABLED=true SQRL_LOG_LEVEL=debug cargo run --bin sqrld || true
    EXIT_CODE=${PIPESTATUS[0]}

    # Check if we should restart (exit code 0 = graceful shutdown for restart)
    if [ "$EXIT_CODE" -eq 0 ]; then
      echo ""
      echo "Server exited normally, restarting..."
      echo ""
      sleep 1
    else
      echo ""
      echo "Server exited with code $EXIT_CODE"
      exit $EXIT_CODE
    fi
  done
else
  echo ""

  # Restart loop for production mode
  while true; do
    cargo run --bin sqrld || true
    EXIT_CODE=${PIPESTATUS[0]}

    # Check if we should restart (exit code 0 = graceful shutdown for restart)
    if [ "$EXIT_CODE" -eq 0 ]; then
      echo ""
      echo "Server exited normally, restarting..."
      echo ""
      sleep 1
    else
      echo ""
      echo "Server exited with code $EXIT_CODE"
      exit $EXIT_CODE
    fi
  done
fi
