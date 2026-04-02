#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="piki-multi-ai"
DEFAULT_DEST="$HOME/.local/bin"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/piki-multi"
THEMES_DIR="$CONFIG_DIR/themes"

usage() {
    echo "Usage: $0 [-d DEST_DIR] [-h]"
    echo ""
    echo "Build piki-multi-ai in release mode and install the binary."
    echo "Also installs default themes to $THEMES_DIR."
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

# Install themes
mkdir -p "$THEMES_DIR"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THEME_SRC="$SCRIPT_DIR/themes"

if [ -d "$THEME_SRC" ]; then
    for theme_file in "$THEME_SRC"/*.toml; do
        name="$(basename "$theme_file")"
        dest="$THEMES_DIR/$name"
        if [ -f "$dest" ]; then
            echo "  Theme '$name' already exists, skipping (delete to reinstall)"
        else
            cp "$theme_file" "$dest"
            echo "  Installed theme: $name"
        fi
    done
else
    echo "Warning: themes/ directory not found, skipping theme install"
fi

# Create default config if it doesn't exist
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    echo 'theme = "default"' > "$CONFIG_DIR/config.toml"
    echo "  Created $CONFIG_DIR/config.toml"
fi

echo "Done. Available themes: $(ls "$THEMES_DIR"/*.toml 2>/dev/null | xargs -I{} basename {} .toml | tr '\n' ' ')"
echo "Set theme in $CONFIG_DIR/config.toml (e.g. theme = \"nord\")"
