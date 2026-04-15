# AGENTS.md — Instructions for AI agents

## Mandatory workflow

### Documentation updates

**ALWAYS** update documentation when making changes:

1. **Before every commit (including amends)**, review `README.md` and update it to reflect the changes being committed. This is mandatory — no exceptions. Check keyboard shortcuts, feature descriptions, prerequisites, and architecture sections.
2. Update `AGENTS.md` if the change affects build commands, architecture descriptions, or developer workflow
3. Update inline code comments only where logic is not self-evident
4. Documentation must be updated in the **same task** as the code change — never leave it for later

### Pre-commit quality gate

**Before every commit**, ensure zero compiler and linter warnings:

```bash
cargo clippy --all-targets    # Must produce 0 warnings
```

If changes touch the desktop frontend (`crates/desktop/frontend/`), also run:

```bash
cd crates/desktop/frontend && npx tsc --noEmit    # Must produce 0 errors
```

Do **not** commit code that introduces new warnings or type errors. Fix all warnings before committing.

## What is this?

agent-multi is a Rust TUI application for running multiple Claude Code instances in parallel, each in its own isolated git worktree. Built with ratatui + crossterm for the UI, portable-pty for PTY management, and tokio for async.

## Build & Test Commands

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run all tests
cargo test <test_name>   # Run a single test (e.g. cargo test test_parse_name_status_basic)
cargo clippy             # Lint
cargo fmt --check        # Check formatting
```

**TUI Snapshot tests** (`crates/tui/src/ui/mod.rs`): Use `insta` crate for visual snapshot testing of rendered UI. Snapshots live in `crates/tui/src/ui/snapshots/`. When adding new UI tests, render to a `TestBackend` buffer and call `insta::assert_snapshot!`. After first run, accept new snapshots with `cargo insta review` or rename `.snap.new` files to `.snap`.

Requires Rust >= 1.85 (edition 2024). Runtime deps: `claude` CLI in PATH, git >= 2.20, optionally `delta` for side-by-side diffs, optionally `gh` CLI for code review feature.

## Architecture

**Event loop** (`main.rs`): Single tokio async loop at 50ms tick rate. Key events produce `Action` variants, which are executed asynchronously. File watcher events are polled each tick with 500ms debounce.

**App state** (`app.rs`): Centralized state in `App` struct. Modal UI with `AppMode` (Normal/Diff/NewWorkspace/Help/WorkspaceSwitcher/etc.) and `interacting` flag that toggles between navigation mode (hjkl between panes) and interaction mode (keys forwarded to active pane). Three panes: `WorkspaceList`, `GitStatus`, `MainPanel`. Navigation mode supports symmetric pane movement (up/down from MainPanel reaches left panes). `previous_workspace` tracks the last-visited workspace for quick toggle via backtick. Non-terminal panes accept `Esc` as alternative to `Ctrl+G` for exiting interaction mode. Quick-action shortcuts (`s`/`u`) work from navigation mode when GitStatus is focused; when files are multi-selected, these dispatch `GitStageSelected`/`GitUnstageSelected` bulk actions instead. Tab/Shift+Tab is context-aware: cycles workspaces in sidebar, subtabs in main panel, files in status panel. Multi-select: `selected_files: HashSet<String>` stores selected file paths (cleared on workspace switch, pruned on refresh). In interaction mode on GitStatus: Space toggles selection (advances cursor), `a` toggles select-all/deselect-all. Selected files show `>` marker and subtle background highlight; block title shows count. Bulk stage/unstage runs a single `git add`/`git reset HEAD` with all paths.

**PTY** (`pty/`): `PtySession` wraps portable-pty (sync) with `spawn_blocking` for non-blocking reads. `vt100::Parser` accumulates terminal state, rendered by `tui-term`. `input.rs` converts crossterm key events to PTY byte sequences.

**Data paths** (`paths.rs`): `DataPaths` struct centralizes all directory paths (database, worktrees, logs, config). Constructed once in `main()` from `--data-dir` CLI flag or platform defaults. When `--data-dir` is set, ALL paths (including config) resolve under that directory for full isolation. Threaded to `event_loop::run()`, `App::new()`, `WorkspaceManager`, `create_storage()`, `Config::load_from()`, and `theme::load_from()`.

**Workspace management** (`workspace/`): `WorkspaceManager` handles git worktree CRUD. Carries a `DataPaths` instance for worktree path resolution. Worktrees stored in `<data_dir>/worktrees/<project>/<name>` with branches matching the workspace name. `FileWatcher` uses `notify` crate with mpsc channels. Each workspace has a persistent `order: u32` field that controls deterministic display ordering across restarts; new workspaces get `max_order + 1`. Tab/Shift+Tab cycling follows sidebar visual order, skipping workspaces in collapsed groups. Workspace switching: Tab/Shift+Tab cycle, 1-9 jump (with number badges in sidebar), Space opens fuzzy workspace switcher (`workspace_switcher.rs`), backtick toggles to previous workspace. Workspaces are also searchable via the command palette (Ctrl+P).

**Diff pipeline** (`diff/runner.rs`): Runs `git diff | delta` (with plain `git diff` fallback), converts ANSI output to ratatui `Text` via `ansi-to-tui`. `run_commit_diff_with_delta` in `action.rs` does the same for `git show <sha>` (used by Git Log viewer).

**Git Log viewer** (`L` in navigation mode): Overlay showing `git log --oneline --graph --decorate --all -50`. State stored in `DialogState::GitLog` (`dialog_state.rs`). Input handled by `handle_git_log_input` in `input/dialog.rs`. Rendered by `render_git_log_overlay` in `ui/dialogs.rs`. Enter on a commit triggers `Action::ViewCommitDiff(sha)` which pipes `git show` through delta and opens the diff overlay.

**Conflict resolution** (`X` in navigation mode, or auto-opens after merge/rebase conflict): Overlay showing conflicted files with resolve actions. State stored in `DialogState::ConflictResolution` (`dialog_state.rs`). Types: `ConflictFile` (path + status), `ConflictStrategy` (Ours/Theirs/MarkResolved). Input handled by `handle_conflict_resolution_input` in `input/dialog.rs`. Rendered by `render_conflict_resolution_overlay` in `ui/dialogs.rs`. Actions: `o` = checkout --ours + add, `t` = checkout --theirs + add, `m` = git add (mark resolved), `e` = open in $EDITOR, `A` = abort merge/rebase. When all conflicts resolved, overlay closes with toast prompting commit. The `GitMerge` action handler detects conflicts (via `git status --porcelain=v1`) instead of auto-aborting on failure, opening this overlay automatically.

**GitHub integration** (`github.rs`): Async wrappers around `gh` CLI for PR operations: view PR info/files, get per-file diffs as parsed unified diffs with line numbers (`parse_unified_diff`), submit reviews with inline comments via `gh api`. Used by the Code Review tab.

**UI** (`ui/`): `layout.rs` is the main render function composing all panels. Sub-modules render individual components (terminal, diff, workspaces, files, tabs, statusbar). `scrollbar.rs` provides a shared `render_vertical()` helper used by all scrollable panels (terminal, diff, markdown, file list, workspace list) — only shown when content overflows. `ui/mod.rs` contains insta snapshot tests for dialogs, overlays, and the full layout; snapshots stored in `ui/snapshots/`.

**Syntax highlighting** (`syntax.rs`): `SyntaxHighlighter` wraps `syntect` for ratatui integration. Provides `find_syntax(path)` (by file extension), `find_syntax_by_name(name)` (by language token), `highlighter_for(syntax)`, and `highlight_line(hl, line, base_style) -> Vec<Span>`. Stored on `App` as `app.syntax`. Integrated in three rendering surfaces: code review diffs (`ui/code_review.rs` — per-line highlighting merged with add/delete base styles), inline editor (`ui/editor.rs` — with cursor overlay splitting), and markdown fenced code blocks (`ui/markdown.rs` — language hint extraction from opening fence). Theme configurable via `syntax_theme` in `config.toml` (default: `base16-ocean.dark`).

**Desktop app** (`crates/desktop/`): Tauri v2 desktop GUI (`piki-desktop`). Rust backend in `src/` wraps `piki-core` via Tauri IPC commands; web frontend in `frontend/` uses vanilla TypeScript with xterm.js for terminal rendering. Uses `RawPtySession` (streams raw bytes to xterm.js, no `vt100`) instead of core's `PtySession`. Shares the same SQLite database as the TUI. Build with `cargo build -p piki-desktop` (requires `cd crates/desktop/frontend && npm install` first).

**API client** (`crates/api-client/`): Independent crate (`piki-api-client`) for HTTP API calls. `ApiClient` trait abstracts the transport layer; `HttpClient` implements it via `reqwest`. Includes a Hurl-like syntax parser (`parser.rs`) that converts `METHOD URL\nHeaders\n\nBody` text into `ParsedRequest` structs. `Protocol` enum (`protocol.rs`) prepared for future gRPC support. `OllamaClient` (`ollama.rs`) provides Ollama HTTP API integration: `list_models()` via `GET /api/tags`, `chat_stream()` for streaming `POST /api/chat` with token-by-token delivery through `mpsc` channels, and `chat()` for non-streaming requests. `LlamaCppClient` (`llamacpp.rs`) provides llama.cpp server integration via the OpenAI-compatible API: `list_models()` via `GET /v1/models`, `chat_stream()` for streaming `POST /v1/chat/completions` with SSE token delivery. Does not depend on `piki-core` or `piki-tui`.

**AI Chat** (`Ctrl+Y` in TUI, `Ctrl+Shift+L` in desktop): Global chat panel powered by local LLMs via Ollama or llama.cpp server. Not tied to any workspace — persists across workspace switches. Chat types (`ChatMessage`, `ChatRole`, `ChatConfig`, `ChatServerType`) defined in `crates/core/src/chat.rs`. `ChatServerType` enum (Ollama, LlamaCpp) selects which backend API to use; each has a default URL (`:11434` for Ollama, `:8080` for llama.cpp). TUI: `ChatPanelState` lives as a top-level `App` field (not in `DialogState`) so state survives toggling the overlay; centered floating panel with message history, streaming indicator, model selector (Tab), and text input; settings (Ctrl+O) include server type selector, base URL, and system prompt. Desktop: right-side panel with CSS grid toggle (`.chat-visible` class), streaming tokens delivered via `"chat-token"` Tauri events, model dropdown populated from the selected server's model list endpoint; settings dialog includes server type dropdown. Config persisted to `UiPrefsStorage` under key `"chat_config"`.

**Providers** (`crates/core/src/providers.rs`): User-configurable provider system. `ProviderConfig` defines a provider's binary, default args, prompt format, dispatchable flag, and agent directory. `ProviderManager` loads/saves from `~/.config/piki-multi/providers.toml`; on first startup, creates the file with a default Claude entry. `AIProvider::Custom(name)` variant links to a `ProviderConfig` by name. Both TUI (`App.provider_manager`) and Desktop (`DesktopApp.provider_manager`) load the manager at startup and thread it through tab spawning, agent dispatch, and agent sync operations.

**Storage** (`crates/core/src/storage/`): Trait-based storage layer using SQLite as the sole backend. Traits: `WorkspaceStorage`, `ApiHistoryStorage`, `UiPrefsStorage`. Single database at `~/.local/share/piki-multi/piki.db` with WAL mode. `AppStorage` struct holds boxed trait objects; created by `storage::create_storage()` factory. `SqliteStorage` uses `parking_lot::Mutex<Connection>` for thread safety. Schema includes FTS5 virtual table for API history search, a unique index on `(source_repo, method, url, request_text)` for deduplication (upsert keeps latest response), and an AFTER UPDATE trigger to keep FTS in sync. API history queries are scoped by `source_repo`. Legacy JSON migration: `piki-multi-ai migrate`.

**API Explorer tab** (`ui/api.rs`, `input/interaction.rs:handle_api_interaction`): Non-PTY tab (like Kanban) where users write HTTP requests in Hurl syntax and view responses. Editor starts empty. Uses `EditorState` for text editing, `ApiTabState` for tab state, and spawns async requests via `tokio::spawn` with a `pending_response` `Arc<Mutex>` slot polled in the event loop. All error paths (parse failures, client init errors, network/request errors) and successful responses are logged via `tracing` so they appear in the in-app log viewer (`Ctrl+L`). `Ctrl+F` opens a search bar in the response panel (`ApiSearchState`); typing performs case-insensitive substring matching across all response bodies with Enter/Shift+Enter to navigate matches and auto-scroll. `Ctrl+H` opens the API history overlay (SQLite backend only) showing past requests with FTS search; Enter loads a history entry into the editor, `d` deletes, `/` searches. Mouse scroll adjusts `editor.scroll_offset` or `response_scroll` depending on which pane the cursor is over (uses `api_response_inner_area` for hit-testing). Mouse click+drag selects text (blue overlay, same as terminal), and releasing copies to clipboard. `Ctrl+C` (or `Ctrl+Shift+C`) copies the entire response body; `Ctrl+C` is preferred since terminals often intercept `Ctrl+Shift+C`. The response inner area is tracked in `app.api_response_inner_area` for mouse hit-testing. Footer bar shows API-specific hints (send, scroll, search, copy, back) when interacting, with search-mode hints when the search overlay is open; the footer cache key includes `api_footer_state` (u8) to track these transitions.

## Task Tracking

Use `flow-cli` to manage the project kanban board. Always set the env var:

```bash
export FLOW_BOARD_PATH=/home/zero/git/agent-multi/.board
```

Common commands:

```bash
flow-cli list                              # See all cards by column
flow-cli show <card_id>                    # Card details
flow-cli create <column> "title" --body "description" --priority medium  # Create card
flow-cli move <card_id> <column>           # Move card between columns
flow-cli edit <card_id> --title "..." --body "..." --priority high       # Update card
flow-cli delete <card_id>                  # Delete card
flow-cli columns                           # List column ids and card counts
```

Output format can be changed with `-f`: `plain` (default), `json`, `xml`, `csv`, `table`, `markdown`.

Priority values: `low`, `medium` (default), `high`, `bug`, `wishlist`.

Columns: `todo`, `in_progress`, `in_review`, `done`.

### Task lifecycle (mandatory)

**ALWAYS** keep the board in sync with your work:

1. **Before implementing**: Search the board (`flow-cli list`) for an existing card matching the task. If none exists, create one with `flow-cli create todo "Title" --body "description" --priority <level>`.
2. **Starting work**: Move the card to `in_progress` before writing any code.
3. **Implementation done**: Move the card to `in_review` once the code is complete and tests pass.
4. **After commit**: Move the card to `done` only after the git commit is created.

Check the board at the start of each session to understand current state.
