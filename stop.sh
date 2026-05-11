#!/bin/bash
# Stop Moot server

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR" || exit 1

if [ -f moot.pid ]; then
    PID=$(cat moot.pid)
    if ps -p "$PID" > /dev/null 2>&1; then
        echo "Stopping Moot server (PID: $PID)..."
        kill "$PID"
        rm moot.pid
        echo "Server stopped"
    else
        echo "Server not running (stale PID file)"
        rm moot.pid
    fi
else
    echo "No PID file found. Trying to find and kill moot process..."
    pkill -f "target/release/moot" && echo "Server stopped" || echo "Server not running"
fi
