# agent-multi

A terminal UI for orchestrating multiple [Claude Code](https://docs.anthropic.com/en/docs/claude-code) instances in parallel, each running in its own isolated git worktree.

Built with Rust and [ratatui](https://ratatui.rs/). Inspired by [superset.sh](https://github.com/supermaven-inc/superset.sh).

## Features

- **Parallel workspaces** — Run multiple AI coding sessions simultaneously, each in an isolated git worktree
- **Multi-AI provider** — Sub-tabs for Claude Code, Gemini CLI, and Codex per workspace; cycle with `g`
- **Live terminal rendering** — See AI assistant output in real-time with full ANSI color support via `tui-term`
- **Interactive input** — Type directly into any AI session (Enter on the terminal pane to interact)
- **Git branch-style naming** — Workspace names support `/`, `.`, `-`, `_` (e.g. `feature/login`, `bugfix/issue-42`)
- **Rich workspace list** — Each workspace shows name, description, worktree path, status, and file count
- **File watching** — Automatically detects file changes in each worktree using `notify`
- **Full git status** — STATUS panel shows all file states: modified, staged, untracked, conflicted, renamed, and more via `git status --porcelain=v1`
- **Side-by-side diffs** — View diffs as a floating overlay rendered by [delta](https://github.com/dandavison/delta) with ANSI colors preserved (terminal stays visible behind)
- **Tab navigation** — Switch between workspaces with Tab, Shift+Tab, or number keys 1-9
- **Vim-style navigation** — j/k for movement, Enter to activate, Esc to go back

## Prerequisites

- [Rust](https://rustup.rs/) >= 1.85 (edition 2024)
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) (`claude` in PATH)
- [git](https://git-scm.com/) >= 2.20 (worktree support)
- [delta](https://github.com/dandavison/delta) (optional, for side-by-side diffs — falls back to plain git diff)

## Installation

```bash
git clone https://github.com/your-user/agent-multi.git
cd agent-multi
cargo build --release
```

The binary will be at `target/release/piki-multi-ai`.

Or use the install script:

```bash
./install.sh              # installs to ~/.local/bin
./install.sh -d /usr/local/bin  # custom directory
```

## Usage

```bash
piki-multi-ai
```

On startup, all previously created workspaces are restored automatically regardless of the current directory. To create new workspaces, press `n` and provide a name (supports git branch characters like `/`), a git repository path, and an optional description.

### Persistence

Workspace configurations are saved automatically and restored on startup. You can run `piki-multi-ai` from any directory — it will discover and restore all previously created workspaces.

```
~/.local/share/piki-multi/
  worktrees/<project-name>/<workspace-name>/   # git worktrees
  workspaces/<project-name>.json               # workspace config per project
```

Branch names match the workspace name exactly (e.g. workspace `feature/login` creates branch `feature/login`). Stale entries (worktrees deleted manually) are cleaned up automatically on load.

### Layout

```
+------------------+-------------------------------------------------------+
| WORKSPACES       |  [ ws-1 ]  [ ws-2 ]  [ ws-3 ]   (tabs)               |
|                  |  [ Claude Code ] [ Gemini ] [ Codex ]  (sub-tabs)     |
|  ▶ ws-1 (active) |-------------------------------------------------------|
|    ● busy | 3    |                                                       |
|    Fix login bug |  AI assistant live terminal output                    |
|    ~/.local/...  |  (diff opens as floating overlay)                     |
|                  |                                                       |
|    ws-2          |                                                       |
|------------------+                                                       |
| STATUS           |                                                       |
|                  |-------------------------------------------------------|
|  M src/auth.rs   |  branch: feature/ws-1 | 3 files | Claude Code: busy  |
|  A src/new.rs    +-------------------------------------------------------+
|  ? untracked.txt |
+------------------+--------------------------------------------------------+
  [n] new  [d] delete  [Tab] switch  [g] switch AI  [?] help  [q] quit
```

### File status indicators

The STATUS panel uses `git status --porcelain=v1` and shows:

| Indicator | Meaning | Color |
|-----------|---------|-------|
| `M` | Modified (unstaged) | Yellow |
| `A` | Added (staged new file) | Green |
| `D` | Deleted | Red |
| `R` | Renamed | Cyan |
| `?` | Untracked | Dark gray |
| `C` | Conflicted (merge conflict) | Magenta |
| `S` | Staged (index only) | Green |
| `SM` | Staged + modified in working tree | Yellow |

### Keybindings

The UI uses a **vim-style modal model**: navigate between panes, then press Enter to interact.

**Navigation mode** (yellow border):

| Key | Action |
|-----|--------|
| `h` / `j` / `k` / `l` | Move between panes |
| `Enter` | Interact with selected pane |
| `n` | Create new workspace |
| `d` | Delete selected workspace |
| `Tab` / `Shift+Tab` | Next / previous workspace |
| `1`-`9` | Jump to workspace N |
| `g` | Cycle AI provider (Claude → Gemini → Codex) |
| `?` | Help overlay |
| `q` | Quit |

**Interaction mode** (green border):

| Key | Action |
|-----|--------|
| `Esc` | Back to navigation mode |
| *Terminal pane* | All keys forwarded to Claude Code |
| *Workspace list* | `j`/`k` select, `Enter` activate |
| *File list* | `j`/`k` select, `Enter` open diff |

**In diff view:**

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll up/down |
| `Ctrl+d` / `Ctrl+u` | Page down/up |
| `g` / `G` | Top / bottom |
| `n` / `p` | Next / previous file |
| `Esc` | Close diff |

## Architecture

```
src/
  main.rs              # Tokio main loop, event handling, action dispatch
  app.rs               # App state, Workspace model, git status parsing
  pty/
    session.rs         # PTY management (portable-pty + vt100 parser)
    input.rs           # Crossterm key events -> PTY bytes
  ui/
    layout.rs          # Full TUI layout (all panels, overlays)
    terminal.rs        # Live PTY rendering (tui-term)
    diff.rs            # Diff rendering (ansi-to-tui)
  workspace/
    manager.rs         # Git worktree CRUD
    config.rs          # Workspace config persistence (JSON)
    watcher.rs         # File system watcher (notify)
  diff/
    runner.rs          # git diff | delta pipeline (with untracked file support)
```

### Sequence diagram

```mermaid
sequenceDiagram
    actor User
    participant Main as main.rs<br/>(event loop)
    participant App as App State
    participant WM as WorkspaceManager
    participant PTY as PtySession
    participant Watcher as FileWatcher
    participant Diff as DiffRunner
    participant UI as UI (ratatui)

    Note over Main: Startup
    Main->>WM: new()
    WM-->>Main: manager
    Main->>App: new()
    Main->>Main: ws_config::load_all()
    Main-->>App: restore persisted workspaces

    loop Event loop (50ms tick)
        Main->>UI: terminal.draw(render(app))
        UI-->>User: TUI frame

        alt User presses 'n' (new workspace)
            User->>Main: KeyEvent('n')
            Main->>App: mode = NewWorkspace
            User->>Main: KeyEvent(Enter) with name
            Main->>WM: create(name)
            WM->>WM: git worktree add
            WM-->>App: Workspace { path, branch }
            Main->>PTY: spawn(path, rows, cols)
            PTY->>PTY: portable-pty fork + exec claude
            PTY-->>App: PtySession + vt100 parser
            Main->>Watcher: new(path)
            Watcher->>Watcher: notify::watch(path)
            Watcher-->>App: FileWatcher (mpsc channel)

        else User types in terminal (Ctrl+\ focused)
            User->>Main: KeyEvent(char)
            Main->>PTY: write(key_to_bytes(key))
            PTY->>PTY: claude process receives input
            Note over PTY: spawn_blocking reads PTY output
            PTY->>App: vt100 parser accumulates state
            Main->>UI: tui-term renders PseudoTerminal

        else File change detected
            Watcher-->>App: watcher.try_recv() → dirty=true
            Note over Main: debounce (500ms)
            Main->>App: ws.refresh_changed_files()
            App->>App: git status --porcelain=v1

        else User presses Enter on file (open diff)
            User->>Main: KeyEvent(Enter)
            Main->>Diff: run_diff(path, file, width, status)
            Diff->>Diff: git diff | delta (or fallback)
            Diff-->>Main: ANSI bytes
            Main->>Main: ansi_to_tui → Text
            Main->>App: diff_content, mode=Diff
            Main->>UI: render diff view with scroll

        else User presses 'd' (delete workspace)
            User->>Main: KeyEvent('d')
            Main->>PTY: kill()
            Main->>App: watcher = None
            Main->>WM: remove(name)
            WM->>WM: git worktree remove + branch -D

        else User presses 'q' (quit)
            User->>Main: KeyEvent('q')
            Main->>Main: shutdown()
            loop Each workspace
                Main->>PTY: kill()
                Main->>App: pty=None, watcher=None
            end
            Main->>UI: restore terminal
        end
    end
```

### Key design decisions

- **portable-pty** (sync) wrapped with `tokio::task::spawn_blocking` for non-blocking PTY reads
- **vt100** parser accumulates terminal state; **tui-term** renders it as a ratatui widget
- **ansi-to-tui** converts delta's ANSI output to `ratatui::text::Text` for the diff view
- Each workspace gets its own PTY session and file watcher, running independently
- Worktrees are stored in `~/.local/share/piki-multi/worktrees/<project>/<name>` with branch names matching the workspace name exactly
- Event-driven architecture: key handlers return `Option<Action>`, main loop executes actions asynchronously
- STATUS panel uses `git status --porcelain=v1` for full coverage of untracked, staged, conflicted, and renamed files
- Diff runner uses `git diff --no-index /dev/null <file>` for untracked files

## License

GPL-2.0 — See [LICENSE](LICENSE) for details.
