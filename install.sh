#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="piki-multi-ai"
DEFAULT_DEST="$HOME/.local/bin"

usage() {
    echo "Usage: $0 [-d DEST_DIR] [-h]"
    echo ""
    echo "Build piki-multi-ai in release mode and install the binary."
    echo ""
    echo "Options:"
    echo "  -d DEST_DIR   Install directory (default: $DEFAULT_DEST)"
    echo "  -h            Show this help message"
}

DEST_DIR="$DEFAULT_DEST"

while getopts "d:h" opt; do
    case "$opt" in
        d) DEST_DIR="$OPTARG" ;;
        h) usage; exit 0 ;;
        *) usage; exit 1 ;;
    esac
done

echo "Building $BINARY_NAME in release mode..."
cargo build --release

mkdir -p "$DEST_DIR"
cp "target/release/$BINARY_NAME" "$DEST_DIR/$BINARY_NAME"

echo "Installed $BINARY_NAME to $DEST_DIR/$BINARY_NAME"

# Check if destination is in PATH
case ":$PATH:" in
    *":$DEST_DIR:"*) ;;
    *) echo "Warning: $DEST_DIR is not in your PATH" ;;
esac
