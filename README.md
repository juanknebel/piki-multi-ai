# agent-multi

[![Nightly](https://github.com/juanknebel/piki-multi-ai/actions/workflows/nightly.yml/badge.svg)](https://github.com/juanknebel/piki-multi-ai/actions/workflows/nightly.yml)

A terminal UI for orchestrating multiple [Claude Code](https://docs.anthropic.com/en/docs/claude-code) instances in parallel — each running in its own isolated git worktree, pointing to an existing directory, or managing a multi-service project root. A [Tauri desktop app](#desktop-application) shares the same core and database.

Built with Rust and [ratatui](https://ratatui.rs/).

| Sidebar focused | Terminal focused |
|:---:|:---:|
| ![Sidebar focused](screenshots/01-general-navigate.png) | ![Terminal focused](screenshots/02-general-interact.png) |

<details>
<summary><b>More screenshots</b> — dashboard, new-tab menu, agents, API explorer, code review</summary>

### Workspace Dashboard

![Dashboard](screenshots/03-dashboard.png)

### New Tab (categorized menu)

| Main menu | AI Agents | Tools |
|:---:|:---:|:---:|
| ![New Tab](screenshots/04-new-tab.png) | ![AI Agents](screenshots/05-new-tab-agents.png) | ![Tools](screenshots/06-new-tab-tools.png) |

### AI Agent Tab

![OpenCode](screenshots/07-tab-opencode.png)

### API Explorer

| Request & Response | Response search |
|:---:|:---:|
| ![API Explorer](screenshots/09-tab-api-explorer.png) | ![API Search](screenshots/10-tab-api-explorer-search.png) |

### Code Review

| Inline comment | General comment |
|:---:|:---:|
| ![Inline comment](screenshots/11-code-review-inline-comment.png) | ![General comment](screenshots/12-code-review-general-comment.png) |

</details>

## Features

### Workspaces & orchestration

- **Parallel workspaces** — run multiple AI coding sessions simultaneously: isolated git worktrees, existing directories (Simple), or multi-service roots (Project)
- **Worktree families** — workspaces from the same repo nest automatically under a collapsible parent row, derived from the git worktree structure
- **Dynamic tabs** — workspaces start empty; open Shell, AI-agent, or Tool tabs on demand from a categorized menu; singletons focus instead of duplicating; any tab can be renamed
- **Workspace dashboard & switcher** — bird's-eye overview of every workspace with status and git info; fuzzy tree switcher and Alt-Tab-style previous-workspace toggle
- **Command palette** — VS Code-style fuzzy palette over every command, with recently-used ranking and live keybinding hints
- **SQLite persistence** — workspaces, UI preferences, and API history restore automatically on startup; the last focused workspace is remembered

### Sessions & terminal

- **Persistent sessions** — every terminal tab runs inside a lightweight background daemon ("tmux without the UI", designed after [shpool](https://github.com/shell-pool/shpool)): tabs survive quitting or crashing the app and re-attach on the next launch with screen and scrollback restored; shared between TUI and desktop; zero setup, no external dependency; managed in-app (TUI sessions overlay `Ctrl+G Ctrl+S`, desktop Sessions dialog `Alt+Shift+S`) or the `sessions` CLI; closing a running tab in the desktop offers *Keep running* to hand it back to the daemon, the status bar shows `sessions N` (click to manage), and orphan sessions can be adopted as tabs from either frontend
- **Live terminal rendering** — full ANSI terminal emulation via `vt100` + `tui-term`, with real-time output from every agent
- **tmux-style prefix keybindings** — keys always go to the focused pane; app actions live behind a one-shot `Ctrl+G` prefix; fully rebindable
- **Terminal search & scrollback** — search output, scroll mode, mouse-wheel scrollback that also captures inline-TUI transcripts
- **Clipboard & mouse** — drag-to-select with auto-copy, paste, click-to-focus, drag-to-resize, contextual scrolling everywhere (Wayland, X11, macOS, Windows)
- **Shell integration** — zsh/bash/fish tabs report cwd and per-command exit codes (OSC 133/7), feeding tab badges and notifications; user dotfiles are preserved

### AI agents

- **Multi-provider tabs** — Claude Code, Gemini, OpenCode, Kilo, Codex out of the box; add your own binaries via `providers.toml`
- **Structured agent lifecycle** — Claude Code and Antigravity tabs report precise running / needs-permission / idle / done states through hook-driven in-band events (no PTY-silence guessing), with graceful fallback to an idle heuristic
- **Agents pane** — one pane listing every running agent across all workspaces with live status and elapsed run time (`3m 12s`); jump straight to any of them. In the desktop, an agent that needs you is amber in the tab bar, status bar, Agents panel, workspace list and activity bar at once, and `Alt+A` lands on it from any workspace (permission requests first, then unseen news — press again to walk through them)
- **Agent profiles & dispatch** — define named agents per project, sync them to provider-native subagent files, and dispatch them from a kanban card into an auto-created worktree with a composed prompt
- **OS notifications** — agent-finished / needs-attention / command-finished toasts (or OSC 9 for tmux/ssh), with optional chimes and smart suppression when you're already looking at the tab; one `[notifications]` config serves the TUI and the desktop
- **AI Chat** — global chat panel backed by local LLMs (Ollama or llama.cpp), with an agentic tool-use mode that can inspect the active workspace

### Built-in tools

- **Git via lazygit** — all git handling delegated to an embedded [lazygit](https://github.com/jesseduffield/lazygit) tab per workspace
- **Code Review** — pick any GitHub PR relevant to you, get an ephemeral checkout, review side-by-side diffs with inline comment threads, and submit via `gh`
- **Kanban board** — integrated task board powered by [flow](https://github.com/juanknebel/flow), with agent dispatch from cards
- **API Explorer** — HTTP client tab with Hurl-like syntax, pretty-printed responses, and searchable per-project history
- **File tools** — fuzzy file search ([nucleo](https://github.com/helix-editor/nucleo)), project-wide content search (ripgrep), inline editor with syntax highlighting, `$EDITOR` integration, markdown viewer
- **Observability** — in-app log viewer, structured file logging, live system status header

The full reference — CLI commands, every keybinding, workspace lifecycle, and internals — lives in [docs/technical.md](docs/technical.md).

## Desktop Application

A desktop GUI is available via `piki-desktop`, built with [Tauri v2](https://v2.tauri.app/). It reuses the same `piki-core` logic and SQLite database as the TUI, so workspaces created in either interface are visible in the other.

Highlights on top of the shared feature set:

- **Native-feeling shell** — custom menu bar, activity bar + sidebar views (Explorer, Files, Source Control, Agents), resizable panes, full keyboard accessibility
- **Theming** — 5 built-in presets (Obsidian Dark, Nord, Catppuccin Mocha, Solarized Light, Tokyo Night), a full per-variable theme editor, and drop-in custom theme files; light themes are first-class — every tint, scrim, shadow and the syntax colours of markdown code blocks derive from the active palette
- **UI zoom** — `Ctrl+=` / `Ctrl+-` / `Ctrl+0` scale the whole interface and the terminal font together (persisted; `Ctrl+Shift+…` twins work with a terminal focused); the layout keeps the editor usable in small windows (chat floats over it below 1000px wide)
- **xterm.js terminals** — WebGL rendering, native clipboard, terminal search
- **Editors** — CodeMirror 6 code tabs with LSP support (diagnostics, completion, hover, go-to-definition), Milkdown WYSIWYG markdown tabs, file viewer with quick-edit; `Ctrl+F` finds a file (opens instantly, index honours `.gitignore`) and `Enter` lands straight in an editor tab
- **Native git** — staging, commits, amend, pull / push with honest in-flight buttons (`↓N` / `↑N`, one op at a time), per-file discard (confirmed; deletes an untracked file), a fuzzy branch switcher from the status bar, merge/rebase, stash, side-by-side diffs with conflict resolution, git log, live ahead/behind tracking
- **Split panes** — recursive horizontal/vertical splits, each pane with its own tab bar; layout persists per workspace
- **Editor-grade tabs & sidebar** — middle-click closes a tab (a running process still gets its confirm), `+` and a `⋯` all-tabs list never scroll away, right-click menus on tabs (rename, split, move to another workspace with the process alive, close / keep running) and on workspace rows (open, agents, info, edit, merge, delete); the `Ctrl+Space` switcher ranks by most-recently-used and matches fuzzily (`wsauth` finds `ws-auth`) with an agent / dirty-git glyph per row; an empty workspace shows its name and branch with Shell / provider / Open file buttons; deleting a workspace says what it removes for its type, counts uncommitted changes and lists the agents it will terminate
- **Extras** — file explorer with git decorations, web preview tab for local dev servers, kanban board, API explorer with history, AI chat panel, system info dashboard

Desktop keyboard shortcuts are listed in [docs/technical.md](docs/technical.md#desktop-keyboard-shortcuts); all of them are editable at runtime (`Alt+S`). The window reopens with the size, position and maximized state it was closed with.

## Installation

### Prerequisites

Required:

- [Rust](https://rustup.rs/) >= 1.85 (edition 2024)
- [git](https://git-scm.com/) >= 2.20 (worktree support)
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) (`claude` in PATH)

Optional, feature-gated:

- [lazygit](https://github.com/jesseduffield/lazygit) — powers the Git tab
- [gh](https://cli.github.com/) — code review (`gh auth login` to authenticate)
- [ripgrep](https://github.com/BurntSushi/ripgrep) — project search (falls back to `grep -rn`)
- [jq](https://jqlang.github.io/jq/) — structured agent integration and API Explorer JSON filtering
- [Ollama](https://ollama.ai/) or [llama.cpp](https://github.com/ggerganov/llama.cpp) — AI Chat panel
- [Node.js](https://nodejs.org/) >= 18 — building the desktop app
- Tauri system libraries (desktop app on Linux): `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`

### TUI

```bash
git clone https://github.com/juanknebel/piki-multi-ai.git
cd piki-multi-ai
./scripts/install.sh              # builds release + installs to ~/.local/bin
./scripts/install.sh -d /usr/local/bin  # custom directory
```

Or build manually with `cargo build --release` — the binary lands at `target/release/piki-multi-ai`.

### Desktop

```bash
./scripts/install-desktop.sh      # builds release binary + desktop entry + icons
```

The install script places the binary in `~/.local/bin/`, installs icons to `~/.local/share/icons/hicolor/`, and creates a `.desktop` entry so Piki Desktop appears in Linux application menus. To build manually:

```bash
cd crates/desktop/frontend && npm install && cd -
cd crates/desktop && cargo tauri build
# Binary at: target/release/piki-desktop
```

## Configuration & Theming

All UI aspects, including keybindings and themes, are customizable via `~/.config/piki-multi/config.toml`. Generate a fully-commented starting point with:

```bash
mkdir -p ~/.config/piki-multi
piki-multi-ai generate-config > ~/.config/piki-multi/config.toml
```

### Keybindings

The `prefix_key` setting (default `"ctrl-g"`) defines the tmux-style prefix. The `[keybindings.app]` table holds all global actions; each value is either a single binding string or an array of alternatives. Strings starting with `prefix-` fire after the prefix key (e.g. `"prefix-c"` = `Ctrl+G c`); anything else is a **direct chord** that fires without the prefix (e.g. `"alt-n"`) — handy for promoting frequent actions, at the cost of shadowing that chord in the embedded terminal. Bindings that collide with each other or with the prefix key are ignored with a warning in the logs.

The remaining tables are per-surface local keys (`scroll`, `agents`, `markdown`, `workspace_list`, `fuzzy`, `editor`, `new_workspace`, `new_tab`, `dashboard`, `sessions`, `logs`, `help`, `about`, `workspace_info`).

```toml
theme = "nord"

[keybindings]
prefix_key = "ctrl-a"          # use Ctrl+A as the prefix, screen-style

[keybindings.app]
quit = "prefix-Q"              # Ctrl+A Q to quit
next_tab = ["prefix-n", "alt-n"]  # also bind Alt+N directly, no prefix

[keybindings.fuzzy]
editor = "ctrl-o"  # Change open in editor from default ctrl-e
```

Keys support `ctrl-`, `alt-`, and `shift-` modifiers (e.g., `ctrl-shift-c`), plus the `prefix-` marker in the `app` table. Special key names: `enter`, `tab`, `backspace`, `esc`, arrows, `pageup`/`pagedown`, `home`/`end`, `insert`, `delete`, and `f1`-`f12`. The default bindings are listed in [docs/technical.md](docs/technical.md#tui-keybindings), and the in-app help (`Ctrl+G ?`) always reflects your current configuration.

### Themes (TUI)

Theme files live at `~/.config/piki-multi/themes/<name>.toml`, selected with `theme = "<name>"` in `config.toml`. You only need to specify the colors you want to override — everything else falls back to defaults:

```toml
[border]
active = "#88c0d0"

[file_list]
modified = "#ebcb8b"
added = "#a3be8c"
deleted = "#bf616a"

[status]
needs_you = "#ebcb8b"
```

See `themes/piki-dark.toml` in the repo for all available color keys (including the `[status]` agent-state and `[diff]` code-review groups). Colors can be named (`"Red"`, `"DarkGray"`), `"Reset"` (terminal default), or hex (`"#rrggbb"`).

Included themes (copied to the config dir by `install.sh`, never overwriting):

| Theme | Description |
|-------|-------------|
| `default` | The built-in "Cabina" dark palette (violet ink, iris accent) |
| `piki-dark` | Same as `default`, under an explicit name |
| `piki-ansi` | Cabina degraded to the 16 ANSI colors, for terminals without truecolor |
| `nord` | Arctic, muted dark palette |
| `tokyonight` | Dark blue-tinted palette |
| `synthwave` | Neon retro-futuristic |
| `breeze` | KDE Plasma's signature dark palette with blue accent |
| `solarized-light` | Warm light background |
| `catppuccin-latte` | Pastel light palette |

### Themes (desktop)

The desktop app ships 5 built-in presets and also scans `~/.config/piki-multi/desktop-themes/` for `*.json` files at startup. Any valid file appears in the preset dropdown next to the built-ins, no recompilation needed:

```json
{
  "id": "my-theme",
  "name": "My Theme",
  "isDark": true,
  "colors": {
    "bg-primary": "#232629",
    "accent-primary": "#3daee9",
    "text-primary": "#eff0f1"
  }
}
```

- `id` must not collide with a built-in (`obsidian-dark`, `nord-dark`, `catppuccin-mocha`, `solarized-light`, `tokyo-night`); colliding files are ignored.
- `colors` is partial-friendly: any key you omit falls back to `obsidian-dark` (when `isDark: true`) or `solarized-light` (when `isDark: false`); invalid hex values are dropped silently.
- See `frontend/src/theme.ts` for the full list of color keys.

### Custom providers (`providers.toml`)

Add custom AI providers via `~/.config/piki-multi/providers.toml` (created with a default Claude entry on first startup). Each provider specifies the binary, arguments, and how prompts are passed:

```toml
[[providers]]
name = "My Custom AI"
description = "A custom AI tool"
command = "/usr/local/bin/my-ai"
default_args = ["--json"]
dispatchable = true
agent_dir = ".my-ai/agents"

[providers.prompt_format]
type = "Flag"          # "Positional" (bare arg), "Flag" (via flag), or "None"
value = "--task"
```

| Field | Description |
|-------|------------|
| `name` | Display name (shown in tab bar and menus) |
| `description` | Human-readable description |
| `command` | Binary path or name (resolved via `$PATH`) |
| `default_args` | Arguments always passed before prompt args |
| `prompt_format` | How prompts are passed: `Positional`, `Flag`, or `None` |
| `dispatchable` | Whether this provider appears in agent dispatch menus |
| `agent_dir` | Repo subdirectory for agent config files (e.g. `.claude/agents`) |

Custom providers appear alongside built-in providers in the New Tab menu and in agent dispatch dialogs, in both the TUI and the desktop app. Providers can also be managed in-app (`Ctrl+G v` in the TUI, Tools → Providers in the desktop).

### Notifications (`[notifications]`)

How background agent events reach you — the TUI and the desktop app both read this table:

```toml
[notifications]
delivery = "system"   # "system" (OS desktop toast, default) | "terminal" | "off"
sound = true          # built-in chimes; off by default
# Optional custom sounds (any format your system player decodes):
# sound_path = "~/sounds/ding.wav"            # used for all events
# sound_done_path = "~/sounds/done.wav"       # agent finished
# sound_attention_path = "~/sounds/hey.wav"   # agent needs you
```

- `delivery = "terminal"` emits an **OSC 9** escape so your terminal emulator (kitty, ghostty, …) shows its own notification — useful inside tmux or over ssh where a desktop toast can't reach you; the sequence is tmux-passthrough-wrapped automatically.
- **Sound is independent of `delivery`** — chimes play even with `delivery = "off"`. Ascending chime = task done; descending-then-up = agent needs input. Chimes fire only for agent events, never for plain shell commands.
- Nothing fires for the tab you're currently looking at while the window has focus.
- Playback uses `pw-play`/`paplay`/`aplay` on Linux, `afplay` on macOS — no audio stack in piki itself. Set `PIKI_DISABLE_SOUND=1` to hard-mute regardless of config.

### Persistent sessions (`[sessions]`)

Persistent sessions are on by default and need no setup. To run every tab in-process instead (the pre-daemon behavior):

```toml
[sessions]
enabled = false
```

The daemon can be managed in-app (TUI sessions overlay `Ctrl+G Ctrl+S`, desktop Sessions dialog `Alt+Shift+S`) or from the CLI (`piki-multi-ai sessions list|kill|stop`); see [docs/technical.md](docs/technical.md#persistent-sessions) for the full behavior and [docs/persistent-sessions.md](docs/persistent-sessions.md) for the design.

## Documentation

- [docs/technical.md](docs/technical.md) — the complete reference: CLI, layout, every keybinding, workspace lifecycle, agent integrations, architecture, and development workflow
- [docs/persistent-sessions.md](docs/persistent-sessions.md) — persistent-session daemon design and wire protocol
- [docs/performance.md](docs/performance.md) — event-loop performance model and invariants

## License

GPL-2.0 — See [LICENSE](LICENSE) for details.
