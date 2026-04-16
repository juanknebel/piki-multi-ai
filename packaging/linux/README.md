# Piki — Linux Package

This package contains:

- **piki-multi-ai** — TUI for running multiple Claude Code instances in parallel
- **piki-desktop** — Desktop GUI application (Tauri)
- **install.sh** — Automated installer
- **icon.png** — Application icon

## Quick Install

```bash
tar xzf piki-linux-amd64.tar.gz
cd piki-linux-amd64
./install.sh
```

The installer copies binaries to `~/.local/bin/`, installs the app icon,
and creates desktop launchers (`.desktop` files) so both apps appear in
your application menu.

## Manual Install

1. Copy the binaries to a directory in your `$PATH`:

   ```bash
   mkdir -p ~/.local/bin
   cp piki-multi-ai piki-desktop ~/.local/bin/
   ```

2. Optionally copy the icon and create `.desktop` launchers — see
   `install.sh` for the exact format.

## Requirements

- Git >= 2.20
- `claude` CLI in PATH
- Optional: `delta` for side-by-side diffs
- Optional: `gh` CLI for code review features
