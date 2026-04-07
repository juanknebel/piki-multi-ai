#!/usr/bin/env bash
set -euo pipefail

DATA_DIR=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --data-dir)
            DATA_DIR="$2"
            shift 2
            ;;
        --data-dir=*)
            DATA_DIR="${1#*=}"
            shift
            ;;
        *)
            echo "Usage: $0 [--data-dir <path>]"
            exit 1
            ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DESKTOP_DIR="$PROJECT_ROOT/crates/desktop"
FRONTEND_DIR="$DESKTOP_DIR/frontend"

# Install frontend dependencies if needed
if [ ! -d "$FRONTEND_DIR/node_modules" ]; then
    echo "Installing frontend dependencies..."
    cd "$FRONTEND_DIR"
    npm install --silent
fi

# Type-check frontend before launching
echo "Type-checking frontend..."
cd "$FRONTEND_DIR"
npx tsc --noEmit

# Run Tauri dev mode
echo "Starting Piki Desktop in dev mode..."
cd "$DESKTOP_DIR"

if [ -n "$DATA_DIR" ]; then
    cargo tauri dev -- -- --data-dir "$DATA_DIR"
else
    cargo tauri dev
fi
