# Piki — Linux Package

This package contains:

- **piki-multi-ai** — TUI for running multiple Claude Code instances in parallel
- **piki-desktop** — Desktop GUI application (Tauri)
- **install.sh** — Automated installer
- **icon.png** — Application icon
- **themes/** — Built-in themes
- **config.example.toml** — Default config

## Quick Install

```bash
tar xzf piki-linux-amd64.tar.gz
cd piki-linux-amd64
./install.sh
```

The installer copies binaries to `~/.local/bin/`, installs the app icon,
creates desktop launchers (`.desktop` files) so both apps appear in your
application menu, and installs the built-in themes and default config to
`~/.config/piki-multi/`.

## Manual Install

1. Copy the binaries to a directory in your `$PATH`:

   ```bash
   mkdir -p ~/.local/bin
   cp piki-multi-ai piki-desktop ~/.local/bin/
   ```

2. Optionally copy the icon and create `.desktop` launchers — see
   `install.sh` for the exact format.

3. Optionally install themes and the default config:

   ```bash
   mkdir -p ~/.config/piki-multi/themes
   cp themes/*.toml ~/.config/piki-multi/themes/
   cp config.example.toml ~/.config/piki-multi/config.toml
   ```

## Requirements

- Git >= 2.20
- `claude` CLI in PATH
- Optional: `delta` for side-by-side diffs
- Optional: `gh` CLI for code review features
