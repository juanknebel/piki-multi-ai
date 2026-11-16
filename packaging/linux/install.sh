#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="$HOME/.local/bin"
ICON_DIR="$HOME/.local/share/icons/piki"
DESKTOP_DIR="$HOME/.local/share/applications"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/piki-multi"
THEMES_DIR="$CONFIG_DIR/themes"
# The desktop app scans this directory at startup (commands/theme.rs).
DESKTOP_THEMES_DIR="$CONFIG_DIR/desktop-themes"
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
echo "    6. Install TUI themes            -> $THEMES_DIR/"
echo "    7. Install Desktop (GUI) themes  -> $DESKTOP_THEMES_DIR/"
echo "    8. Create config.toml (if absent) -> $CONFIG_DIR/"
echo ""

# `|| true`: `read` returns non-zero at EOF, and under `set -e` that killed the
# whole install silently when stdin wasn't a terminal (a piped run did nothing
# and exited 1). No terminal means the defaults apply: continue, keep themes.
answer=""
read -rp "  Continue? [Y/n] " answer || true
if [[ "${answer:-Y}" =~ ^[Nn] ]]; then
    echo "  Aborted."
    exit 0
fi

# Ask about themes BEFORE doing any work, so the whole run is decided up front
# and nothing stops half-way to ask. Only asked when there is something to
# lose; with no TTY (piped install) the answer is "keep", the safe default.
OVERWRITE_THEMES=no
existing_themes=0
for f in "$THEMES_DIR"/*.toml "$DESKTOP_THEMES_DIR"/*.json; do
    [ -f "$f" ] && existing_themes=$((existing_themes + 1))
done
if [ "$existing_themes" -gt 0 ]; then
    echo ""
    echo "  $existing_themes theme file(s) are already installed."
    echo "  Overwriting replaces them with this release's copies — any edits you"
    echo "  made to a theme OF THE SAME NAME are lost. Your own themes (names"
    echo "  this release does not ship) are never touched."
    if [ -t 0 ]; then
        theme_answer=""
        read -rp "  Overwrite all themes? [y/N] " theme_answer || true
        [[ "${theme_answer:-N}" =~ ^[Yy] ]] && OVERWRITE_THEMES=yes
    else
        echo "  (no terminal to ask on — keeping the installed themes)"
    fi
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

# Install themes. The tarball's themes/ holds BOTH kinds and they go to
# different places: the TUI reads `<config>/themes/*.toml`
# (piki-tui theme.rs) and the desktop reads `<config>/desktop-themes/*.json`
# (commands/theme.rs `list_custom_themes`). Installing only the .toml side —
# which is what this script used to do — shipped every GUI theme in the
# tarball and then dropped it on the floor.
install_themes() {
    local pattern="$1" dest_dir="$2" label="$3"
    local installed=0 kept=0 replaced=0

    mkdir -p "$dest_dir"
    shopt -s nullglob
    for theme_file in $pattern; do
        local name dest
        name="$(basename "$theme_file")"
        dest="$dest_dir/$name"
        if [ -f "$dest" ]; then
            if [ "$OVERWRITE_THEMES" = yes ]; then
                cp "$theme_file" "$dest"
                replaced=$((replaced + 1))
            else
                kept=$((kept + 1))
            fi
        else
            cp "$theme_file" "$dest"
            installed=$((installed + 1))
        fi
    done
    shopt -u nullglob

    echo "  $label -> $dest_dir"
    echo "    $installed new, $replaced overwritten, $kept kept as-is"
}

THEME_SRC="$SCRIPT_DIR/themes"

if [ -d "$THEME_SRC" ]; then
    install_themes "$THEME_SRC/*.toml" "$THEMES_DIR" "TUI themes    "
    install_themes "$THEME_SRC/*.desktop.json" "$DESKTOP_THEMES_DIR" "Desktop themes"
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

echo "  Done."
echo "  TUI themes:     $(ls "$THEMES_DIR"/*.toml 2>/dev/null | xargs -I{} basename {} .toml | tr '\n' ' ')"
echo "  Desktop themes: $(ls "$DESKTOP_THEMES_DIR"/*.json 2>/dev/null | xargs -I{} basename {} .desktop.json | tr '\n' ' ')"
echo "  Set the TUI theme in $CONFIG_DIR/config.toml (e.g. theme = \"nord\"); pick the"
echo "  desktop one in Settings > Appearance > Open theme editor."
echo ""
