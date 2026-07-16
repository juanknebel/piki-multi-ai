#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/piki-multi"
THEMES_DIR="$CONFIG_DIR/themes"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo ""
echo "  Piki Installer"
echo "  ==============="
echo ""
echo "  This script will:"
echo ""
echo "    1. Create $INSTALL_DIR/ (if it doesn't exist)"
echo "    2. Copy piki-multi-ai (TUI)     -> $INSTALL_DIR/"
echo "    3. Copy piki-desktop  (Desktop)  -> $INSTALL_DIR/"
echo "    4. Remove the quarantine attribute from both binaries"
echo "    5. Install themes and config     -> $CONFIG_DIR/"
echo ""

read -rp "  Continue? [Y/n] " answer
if [[ "${answer:-Y}" =~ ^[Nn] ]]; then
    echo "  Aborted."
    exit 0
fi

echo ""

mkdir -p "$INSTALL_DIR"

if [ -f "$SCRIPT_DIR/piki-multi-ai" ]; then
    cp "$SCRIPT_DIR/piki-multi-ai" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/piki-multi-ai"
    xattr -dr com.apple.quarantine "$INSTALL_DIR/piki-multi-ai" 2>/dev/null || true
    echo "  Installed piki-multi-ai -> $INSTALL_DIR/"
else
    echo "  WARNING: piki-multi-ai not found, skipping"
fi

if [ -f "$SCRIPT_DIR/piki-desktop" ]; then
    cp "$SCRIPT_DIR/piki-desktop" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/piki-desktop"
    xattr -dr com.apple.quarantine "$INSTALL_DIR/piki-desktop" 2>/dev/null || true
    echo "  Installed piki-desktop  -> $INSTALL_DIR/"
else
    echo "  WARNING: piki-desktop not found, skipping"
fi

# Install themes
mkdir -p "$THEMES_DIR"

THEME_SRC="$SCRIPT_DIR/themes"

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
    echo "  WARNING: themes/ directory not found, skipping theme install"
fi

# Create default config if it doesn't exist, from the documented example.
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    CONFIG_EXAMPLE="$SCRIPT_DIR/config.example.toml"
    if [ -f "$CONFIG_EXAMPLE" ]; then
        cp "$CONFIG_EXAMPLE" "$CONFIG_DIR/config.toml"
    else
        echo 'theme = "default"' > "$CONFIG_DIR/config.toml"
    fi
    echo "  Created $CONFIG_DIR/config.toml"
fi

# Check PATH
echo ""
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "  NOTE: $INSTALL_DIR is not in your \$PATH."
    echo "  Add it to your shell profile:"
    echo ""
    if [ -f "$HOME/.zshrc" ]; then
        echo "    echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc"
    elif [ -f "$HOME/.bashrc" ]; then
        echo "    echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
    else
        echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
    echo ""
    echo "  Then restart your shell."
fi

echo "  Done. Available themes: $(ls "$THEMES_DIR"/*.toml 2>/dev/null | xargs -I{} basename {} .toml | tr '\n' ' ')"
echo "  Set theme in $CONFIG_DIR/config.toml (e.g. theme = \"nord\")"
echo ""
