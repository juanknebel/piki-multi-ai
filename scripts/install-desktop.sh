#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="piki-desktop"
APP_NAME="Piki Desktop"
APP_ID="com.piki.desktop"
DEFAULT_DEST="$HOME/.local/bin"
APPLICATIONS_DIR="$HOME/.local/share/applications"
ICONS_DIR="$HOME/.local/share/icons/hicolor"

usage() {
    echo "Usage: $0 [-d DEST_DIR] [-h]"
    echo ""
    echo "Build Piki Desktop (Tauri) in release mode, install the binary,"
    echo "and create a .desktop entry for Linux application menus."
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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DESKTOP_DIR="$PROJECT_ROOT/crates/desktop"
FRONTEND_DIR="$DESKTOP_DIR/frontend"
ICONS_SRC="$DESKTOP_DIR/icons"

# Step 1: Install frontend dependencies
echo "Installing frontend dependencies..."
cd "$FRONTEND_DIR"
npm install --silent
cd "$DESKTOP_DIR"

# Step 2: Build release binary via cargo tauri build
echo "Building $BINARY_NAME in release mode..."
cargo tauri build --no-bundle

# Step 3: Install binary
mkdir -p "$DEST_DIR"
cp "$PROJECT_ROOT/target/release/$BINARY_NAME" "$DEST_DIR/$BINARY_NAME"
chmod +x "$DEST_DIR/$BINARY_NAME"
echo "Installed binary to $DEST_DIR/$BINARY_NAME"

# Check if destination is in PATH
case ":$PATH:" in
    *":$DEST_DIR:"*) ;;
    *) echo "Warning: $DEST_DIR is not in your PATH" ;;
esac

# Step 4: Install icons
for size in 32 128 256; do
    icon_dir="$ICONS_DIR/${size}x${size}/apps"
    mkdir -p "$icon_dir"
    if [ -f "$ICONS_SRC/${size}x${size}.png" ]; then
        cp "$ICONS_SRC/${size}x${size}.png" "$icon_dir/$APP_ID.png"
    fi
done

# Install scalable SVG icon
mkdir -p "$ICONS_DIR/scalable/apps"
if [ -f "$ICONS_SRC/icon.svg" ]; then
    cp "$ICONS_SRC/icon.svg" "$ICONS_DIR/scalable/apps/$APP_ID.svg"
fi

echo "Installed icons to $ICONS_DIR"

# Step 5: Create .desktop file
mkdir -p "$APPLICATIONS_DIR"

cat > "$APPLICATIONS_DIR/$APP_ID.desktop" << EOF
[Desktop Entry]
Name=$APP_NAME
Comment=Multi AI workspace manager - run multiple coding agents in parallel
Exec=$DEST_DIR/$BINARY_NAME
Icon=$APP_ID
Type=Application
Categories=Development;IDE;
Keywords=ai;terminal;workspace;git;agent;coding;
StartupNotify=true
StartupWMClass=$BINARY_NAME
Terminal=false
EOF

chmod +x "$APPLICATIONS_DIR/$APP_ID.desktop"
echo "Created desktop entry at $APPLICATIONS_DIR/$APP_ID.desktop"

# Step 6: Update icon cache (if available)
if command -v gtk-update-icon-cache &>/dev/null; then
    gtk-update-icon-cache -f -t "$ICONS_DIR" 2>/dev/null || true
fi

# Update desktop database (if available)
if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "$APPLICATIONS_DIR" 2>/dev/null || true
fi

echo ""
echo "Done! $APP_NAME is now installed."
echo "  Binary:  $DEST_DIR/$BINARY_NAME"
echo "  Desktop: $APPLICATIONS_DIR/$APP_ID.desktop"
echo "  Run:     $BINARY_NAME"
