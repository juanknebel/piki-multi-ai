# Piki — macOS Package

This package contains:

- **piki-multi-ai** — TUI for running multiple Claude Code instances in parallel
- **piki-desktop** — Desktop GUI application (Tauri), run from the terminal
- **install.sh** — Automated installer
- **themes/** — Built-in themes: `*.toml` for the TUI, `*.desktop.json` for the GUI
- **config.example.toml** — Default config

## Quick Install

```bash
tar xzf piki-macos-arm64.tar.gz
cd piki-macos-arm64
./install.sh
```

The installer copies both binaries to `~/.local/bin/`, removes the
quarantine attribute so Gatekeeper doesn't block them, and installs the
built-in themes and default config under `~/.config/piki-multi/` — TUI themes
in `themes/`, GUI themes in `desktop-themes/` (the two apps read different
directories).

If themes are already installed it asks, before doing anything, whether to
overwrite them: answer `y` to take this release's copies (edits you made to a
theme of the same name are lost) or `N` to keep what you have. Themes of your
own — names this release doesn't ship — are never touched either way. With no
terminal to ask on (a piped run) it keeps the installed ones.

## Manual Install

```bash
mkdir -p ~/.local/bin
cp piki-multi-ai piki-desktop ~/.local/bin/
xattr -dr com.apple.quarantine ~/.local/bin/piki-multi-ai ~/.local/bin/piki-desktop

mkdir -p ~/.config/piki-multi/themes ~/.config/piki-multi/desktop-themes
cp themes/*.toml ~/.config/piki-multi/themes/                    # TUI
cp themes/*.desktop.json ~/.config/piki-multi/desktop-themes/    # GUI
cp config.example.toml ~/.config/piki-multi/config.toml
```

## Prefer a native app bundle?

Use the `.dmg` from the same release instead: mount it, drag **Piki
Desktop.app** to `/Applications`, then remove its quarantine attribute:

```bash
xattr -cr /Applications/Piki\ Desktop.app
```

## Requirements

- Git >= 2.20
- `claude` CLI in PATH
- Optional: `delta` for side-by-side diffs
- Optional: `gh` CLI for code review features
