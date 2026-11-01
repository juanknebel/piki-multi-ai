#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="piki-multi-ai"
CTL_BINARY_NAME="piki-ctl"
DEFAULT_DEST="$HOME/.local/bin"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/piki-multi"
THEMES_DIR="$CONFIG_DIR/themes"

usage() {
    echo "Usage: $0 [-d DEST_DIR] [-h]"
    echo ""
    echo "Build piki-multi-ai in release mode and install the binary."
    echo "Also installs $CTL_BINARY_NAME (the agent-agnostic UI-action CLI any"
    echo "coding agent in a tab can run — see AGENTS.md) and default themes to"
    echo "$THEMES_DIR."
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

# Anchor to the project root so build/cp/theme paths are cwd-independent.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

echo "Building $BINARY_NAME in release mode..."
cargo build --release -p agent-multi

echo "Building $CTL_BINARY_NAME in release mode..."
cargo build --release -p piki-ctl

mkdir -p "$DEST_DIR"
# Atomic replace via a temp file + rename, NOT an in-place `cp`. The persistent
# session daemon is this same binary re-invoked as `serve`; it keeps running
# after the app closes (that's the point — sessions survive), so the installed
# file is often still in use and an in-place overwrite fails with ETXTBSY
# ("Text file busy"). Renaming onto the path swaps in a fresh inode: the running
# daemon keeps its old (now-unlinked) inode, the next launch picks up the new
# binary, and no session has to be killed to upgrade. `piki-ctl` doesn't have
# that daemon problem, but installing it the same way keeps both binaries
# consistent and cheap to reason about.
install_binary() {
    local name="$1"
    local tmp="$DEST_DIR/.$name.new.$$"
    cp "target/release/$name" "$tmp"
    chmod +x "$tmp"
    mv -f "$tmp" "$DEST_DIR/$name"
    echo "Installed $name to $DEST_DIR/$name"
}
install_binary "$BINARY_NAME"
install_binary "$CTL_BINARY_NAME"

# Check if destination is in PATH
case ":$PATH:" in
    *":$DEST_DIR:"*) ;;
    *) echo "Warning: $DEST_DIR is not in your PATH" ;;
esac

# Install themes
mkdir -p "$THEMES_DIR"

THEME_SRC="$PROJECT_ROOT/themes"

if [ -d "$THEME_SRC" ]; then
    for theme_file in "$THEME_SRC"/*.toml; do
        [ -e "$theme_file" ] || continue
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

# Create default config if it doesn't exist, from the documented example.
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    CONFIG_EXAMPLE="$PROJECT_ROOT/config.example.toml"
    if [ -f "$CONFIG_EXAMPLE" ]; then
        cp "$CONFIG_EXAMPLE" "$CONFIG_DIR/config.toml"
    else
        # Fallback: the example is missing, write a minimal config.
        echo 'theme = "default"' > "$CONFIG_DIR/config.toml"
    fi
    echo "  Created $CONFIG_DIR/config.toml"
fi

echo "Done. Available themes: $(ls "$THEMES_DIR"/*.toml 2>/dev/null | xargs -I{} basename {} .toml | tr '\n' ' ')"
echo "Set theme in $CONFIG_DIR/config.toml (e.g. theme = \"nord\")"
