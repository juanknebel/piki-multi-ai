#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="$HOME/.local/bin"
ICON_DIR="$HOME/.local/share/icons/piki"
DESKTOP_DIR="$HOME/.local/share/applications"
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
echo "    4. Install icon                  -> $ICON_DIR/"
echo "    5. Create desktop launchers      -> $DESKTOP_DIR/"
echo ""

read -rp "  Continue? [Y/n] " answer
if [[ "${answer:-Y}" =~ ^[Nn] ]]; then
    echo "  Aborted."
    exit 0
fi

echo ""

# Create directories
mkdir -p "$INSTALL_DIR" "$ICON_DIR" "$DESKTOP_DIR"

# Copy binaries
if [ -f "$SCRIPT_DIR/piki-multi-ai" ]; then
    cp "$SCRIPT_DIR/piki-multi-ai" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/piki-multi-ai"
    echo "  Installed piki-multi-ai -> $INSTALL_DIR/"
else
    echo "  WARNING: piki-multi-ai not found, skipping"
fi

if [ -f "$SCRIPT_DIR/piki-desktop" ]; then
    cp "$SCRIPT_DIR/piki-desktop" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/piki-desktop"
    echo "  Installed piki-desktop  -> $INSTALL_DIR/"
else
    echo "  WARNING: piki-desktop not found, skipping"
fi

# Copy icon
if [ -f "$SCRIPT_DIR/icon.png" ]; then
    cp "$SCRIPT_DIR/icon.png" "$ICON_DIR/"
    echo "  Installed icon          -> $ICON_DIR/"
fi

# Create .desktop for Desktop app
if [ -f "$INSTALL_DIR/piki-desktop" ]; then
    cat > "$DESKTOP_DIR/piki-desktop.desktop" << EOF
[Desktop Entry]
Name=Piki Desktop
Exec=$INSTALL_DIR/piki-desktop
Icon=$ICON_DIR/icon.png
Type=Application
Categories=Development;
Comment=Multi-agent workspace manager
EOF
    echo "  Created launcher        -> $DESKTOP_DIR/piki-desktop.desktop"
fi

# Create .desktop for TUI
if [ -f "$INSTALL_DIR/piki-multi-ai" ]; then
    cat > "$DESKTOP_DIR/piki-tui.desktop" << EOF
[Desktop Entry]
Name=Piki TUI
Exec=$INSTALL_DIR/piki-multi-ai
Icon=$ICON_DIR/icon.png
Type=Application
Terminal=true
Categories=Development;
Comment=Multi-agent workspace manager (terminal)
EOF
    echo "  Created launcher        -> $DESKTOP_DIR/piki-tui.desktop"
fi

# Update desktop database
if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi

# Check PATH
echo ""
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "  NOTE: $INSTALL_DIR is not in your \$PATH."
    echo "  Add it to your shell profile:"
    echo ""
    if [ -f "$HOME/.bashrc" ]; then
        echo "    echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
    elif [ -f "$HOME/.zshrc" ]; then
        echo "    echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc"
    else
        echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
    echo ""
    echo "  Then restart your shell."
fi

echo "  Done!"
echo ""
