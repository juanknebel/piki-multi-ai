# Piki — macOS Package

This package contains:

- **piki-multi-ai** — TUI for running multiple Claude Code instances in parallel
- **piki-desktop** — Desktop GUI application (Tauri), run from the terminal
- **install.sh** — Automated installer
- **themes/** — Built-in themes
- **config.example.toml** — Default config

## Quick Install

```bash
tar xzf piki-macos-arm64.tar.gz
cd piki-macos-arm64
./install.sh
```

The installer copies both binaries to `~/.local/bin/`, removes the
quarantine attribute so Gatekeeper doesn't block them, and installs the
built-in themes and default config to `~/.config/piki-multi/`.

## Manual Install

```bash
mkdir -p ~/.local/bin
cp piki-multi-ai piki-desktop ~/.local/bin/
xattr -dr com.apple.quarantine ~/.local/bin/piki-multi-ai ~/.local/bin/piki-desktop

mkdir -p ~/.config/piki-multi/themes
cp themes/*.toml ~/.config/piki-multi/themes/
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
