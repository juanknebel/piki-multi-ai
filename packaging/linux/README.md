# Piki — Linux Package

This package contains:

- **piki-multi-ai** — TUI for running multiple Claude Code instances in parallel
- **piki-desktop** — Desktop GUI application (Tauri)
- **install.sh** — Automated installer
- **icon.png** — Application icon
- **themes/** — Built-in themes: `*.toml` for the TUI, `*.desktop.json` for the GUI
- **config.example.toml** — Default config

## Quick Install

```bash
tar xzf piki-linux-amd64.tar.gz
cd piki-linux-amd64
./install.sh
```

The installer copies binaries to `~/.local/bin/`, installs the app icon,
creates desktop launchers (`.desktop` files) so both apps appear in your
application menu, and installs the built-in themes and default config under
`~/.config/piki-multi/` — TUI themes in `themes/`, GUI themes in
`desktop-themes/` (the two apps read different directories).

If themes are already installed it asks, before doing anything, whether to
overwrite them: answer `y` to take this release's copies (edits you made to a
theme of the same name are lost) or `N` to keep what you have. Themes of your
own — names this release doesn't ship — are never touched either way. With no
terminal to ask on (a piped run) it keeps the installed ones.

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
   mkdir -p ~/.config/piki-multi/themes ~/.config/piki-multi/desktop-themes
   cp themes/*.toml ~/.config/piki-multi/themes/                    # TUI
   cp themes/*.desktop.json ~/.config/piki-multi/desktop-themes/    # GUI
   cp config.example.toml ~/.config/piki-multi/config.toml
   ```

## Requirements

- Git >= 2.20
- `claude` CLI in PATH
- Optional: `delta` for side-by-side diffs
- Optional: `gh` CLI for code review features
