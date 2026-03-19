# AGENTS.md — Instructions for AI agents

## Mandatory workflow

### Documentation updates

**ALWAYS** update documentation when making changes:

1. Update `README.md` if the change affects user-facing behavior, CLI usage, architecture, or project structure
2. Update `AGENTS.md` if the change affects build commands, architecture descriptions, or developer workflow
3. Update inline code comments only where logic is not self-evident
4. Documentation must be updated in the **same task** as the code change — never leave it for later

### Pre-commit quality gate

**Before every commit**, ensure zero compiler and linter warnings:

```bash
cargo clippy --all-targets    # Must produce 0 warnings
```

Do **not** commit code that introduces new warnings. Fix all warnings before committing.

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

Requires Rust >= 1.85 (edition 2024). Runtime deps: `claude` CLI in PATH, git >= 2.20, optionally `delta` for side-by-side diffs, optionally `gh` CLI for code review feature.

## Architecture

**Event loop** (`main.rs`): Single tokio async loop at 50ms tick rate. Key events produce `Action` variants, which are executed asynchronously. File watcher events are polled each tick with 500ms debounce.

**App state** (`app.rs`): Centralized state in `App` struct. Modal UI with `AppMode` (Normal/Diff/NewWorkspace/Help) and `interacting` flag that toggles between navigation mode (hjkl between panes) and interaction mode (keys forwarded to active pane). Three panes: `WorkspaceList`, `GitStatus`, `MainPanel`.

**PTY** (`pty/`): `PtySession` wraps portable-pty (sync) with `spawn_blocking` for non-blocking reads. `vt100::Parser` accumulates terminal state, rendered by `tui-term`. `input.rs` converts crossterm key events to PTY byte sequences.

**Workspace management** (`workspace/`): `WorkspaceManager` handles git worktree CRUD. Worktrees stored in `.agent-multi/worktrees/<name>` with branches `agent-multi/<name>`. `FileWatcher` uses `notify` crate with mpsc channels. Each workspace has a persistent `order: u32` field that controls deterministic display ordering across restarts; new workspaces get `max_order + 1`. Tab/Shift+Tab cycling follows sidebar visual order, skipping workspaces in collapsed groups.

**Diff pipeline** (`diff/runner.rs`): Runs `git diff | delta` (with plain `git diff` fallback), converts ANSI output to ratatui `Text` via `ansi-to-tui`.

**GitHub integration** (`github.rs`): Async wrappers around `gh` CLI for PR operations: view PR info/files, get per-file diffs as parsed unified diffs with line numbers (`parse_unified_diff`), submit reviews with inline comments via `gh api`. Used by the Code Review tab.

**UI** (`ui/`): `layout.rs` is the main render function composing all panels. Sub-modules render individual components (terminal, diff, workspaces, files, tabs, statusbar).

**API client** (`crates/api-client/`): Independent crate (`piki-api-client`) for HTTP API calls. `ApiClient` trait abstracts the transport layer; `HttpClient` implements it via `reqwest`. Includes a Hurl-like syntax parser (`parser.rs`) that converts `METHOD URL\nHeaders\n\nBody` text into `ParsedRequest` structs. `Protocol` enum (`protocol.rs`) prepared for future gRPC support. Does not depend on `piki-core` or `piki-tui`.

**Storage** (`crates/core/src/storage/`): Trait-based storage layer using SQLite as the sole backend. Traits: `WorkspaceStorage`, `ApiHistoryStorage`, `UiPrefsStorage`. Single database at `~/.local/share/piki-multi/piki.db` with WAL mode. `AppStorage` struct holds boxed trait objects; created by `storage::create_storage()` factory. `SqliteStorage` uses `parking_lot::Mutex<Connection>` for thread safety. Schema includes FTS5 virtual table for API history search, a unique index on `(source_repo, method, url, request_text)` for deduplication (upsert keeps latest response), and an AFTER UPDATE trigger to keep FTS in sync. API history queries are scoped by `source_repo`. Legacy JSON migration: `piki-multi-ai migrate`.

**API Explorer tab** (`ui/api.rs`, `input/interaction.rs:handle_api_interaction`): Non-PTY tab (like Kanban) where users write HTTP requests in Hurl syntax and view responses. Editor starts empty. Uses `EditorState` for text editing, `ApiTabState` for tab state, and spawns async requests via `tokio::spawn` with a `pending_response` `Arc<Mutex>` slot polled in the event loop. All error paths (parse failures, client init errors, network/request errors) and successful responses are logged via `tracing` so they appear in the in-app log viewer (`Ctrl+L`). `Ctrl+F` opens a search bar in the response panel (`ApiSearchState`); typing performs case-insensitive substring matching across all response bodies with Enter/Shift+Enter to navigate matches and auto-scroll. `Ctrl+H` opens the API history overlay (SQLite backend only) showing past requests with FTS search; Enter loads a history entry into the editor, `d` deletes, `/` searches. Mouse scroll adjusts `editor.scroll_offset` or `response_scroll` depending on which pane the cursor is over (uses `api_response_inner_area` for hit-testing). Mouse click+drag selects text (blue overlay, same as terminal), and releasing copies to clipboard. `Ctrl+C` (or `Ctrl+Shift+C`) copies the entire response body; `Ctrl+C` is preferred since terminals often intercept `Ctrl+Shift+C`. The response inner area is tracked in `app.api_response_inner_area` for mouse hit-testing. Footer bar shows API-specific hints (send, scroll, search, copy, back) when interacting, with search-mode hints when the search overlay is open; the footer cache key includes `api_footer_state` (u8) to track these transitions.

## Task Tracking

Use `flow` CLI with the project board to track tasks. Always set the env var:

```bash
export FLOW_BOARD_PATH=/home/zero/git/agent-multi/.board
```

Common commands:

```bash
flow list                              # See all cards by column
flow show <card_id>                    # Card details
flow create <column> "title" --body "description" --priority medium  # Create card
flow move <card_id> <column>           # Move card between columns
flow edit <card_id> --title "..." --body "..." --priority high       # Update card
flow delete <card_id>                  # Delete card
```

Priority values: `low`, `medium` (default), `high`, `bug`, `wishlist`.

Columns: `todo`, `in_progress`, `in_review`, `done`.

### Task lifecycle (mandatory)

**ALWAYS** keep the board in sync with your work:

1. **Before implementing**: Search the board (`flow list`) for an existing card matching the task. If none exists, create one with `flow create todo "Title" --body "description" --priority <level>`.
2. **Starting work**: Move the card to `in_progress` before writing any code.
3. **Implementation done**: Move the card to `in_review` once the code is complete and tests pass.
4. **After commit**: Move the card to `done` only after the git commit is created.

Check the board at the start of each session to understand current state.
