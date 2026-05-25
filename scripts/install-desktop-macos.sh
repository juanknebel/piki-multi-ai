#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Piki Desktop"
APP_BUNDLE="Piki Desktop.app"
APP_DEST="/Applications"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/piki-multi"
DESKTOP_THEMES_DIR="$CONFIG_DIR/desktop-themes"

# ── Guard: macOS + Apple Silicon only ────────────────────────────────
if [ "$(uname -s)" != "Darwin" ]; then
    echo "Error: this script only runs on macOS." >&2
    exit 1
fi

ARCH="$(uname -m)"
if [ "$ARCH" != "arm64" ]; then
    echo "Error: this script requires Apple Silicon (M1 or later). Detected: $ARCH" >&2
    exit 1
fi

# ── Paths ────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DESKTOP_DIR="$PROJECT_ROOT/crates/desktop"
FRONTEND_DIR="$DESKTOP_DIR/frontend"
THEME_SRC="$PROJECT_ROOT/themes"

# ── Step 1: Install frontend dependencies ────────────────────────────
echo "Installing frontend dependencies..."
cd "$FRONTEND_DIR"
npm install --silent
cd "$DESKTOP_DIR"

# ── Step 2: Build release .app bundle ────────────────────────────────
echo "Building $APP_NAME in release mode..."
cargo tauri build --bundles app

# Locate the generated .app bundle
APP_SRC="$PROJECT_ROOT/target/release/bundle/macos/$APP_BUNDLE"
if [ ! -d "$APP_SRC" ]; then
    echo "Error: build succeeded but $APP_SRC was not found." >&2
    exit 1
fi

# ── Step 3: Install to /Applications ─────────────────────────────────
echo "Installing $APP_BUNDLE to $APP_DEST..."
if [ -d "$APP_DEST/$APP_BUNDLE" ]; then
    rm -rf "$APP_DEST/$APP_BUNDLE"
fi
cp -R "$APP_SRC" "$APP_DEST/$APP_BUNDLE"

# ── Step 4: Remove Apple quarantine attribute ────────────────────────
echo "Removing quarantine attribute..."
xattr -cr "$APP_DEST/$APP_BUNDLE"

# ── Step 5: Install custom desktop themes (JSON, scanned at startup) ──
if [ -d "$THEME_SRC" ]; then
    mkdir -p "$DESKTOP_THEMES_DIR"
    shopt -s nullglob
    for theme_file in "$THEME_SRC"/*.desktop.json; do
        name="$(basename "$theme_file")"
        dest="$DESKTOP_THEMES_DIR/$name"
        if [ -f "$dest" ]; then
            echo "  Desktop theme '$name' already exists, skipping (delete to reinstall)"
        else
            cp "$theme_file" "$dest"
            echo "  Installed desktop theme: $name"
        fi
    done
    shopt -u nullglob
fi

echo ""
echo "Done! $APP_NAME is now installed."
echo "  App: $APP_DEST/$APP_BUNDLE"
echo "  Open from Spotlight or run: open \"$APP_DEST/$APP_BUNDLE\""
