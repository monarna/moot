#!/bin/bash
# Moot server startup script

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR" || exit 1

# Build if needed
if [ ! -f target/release/moot ]; then
    echo "Building Moot in release mode..."
    cargo build --release
fi

# Run the server in the background
echo "Starting Moot server..."
nohup ./target/release/moot > moot.log 2>&1 &

# Save PID
echo $! > moot.pid

echo "Moot server started (PID: $(cat moot.pid))"
echo "Server running at http://127.0.0.1:8080"
echo "Logs: moot.log"
