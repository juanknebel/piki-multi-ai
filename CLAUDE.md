# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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

Requires Rust >= 1.85 (edition 2024). Runtime deps: `claude` CLI in PATH, git >= 2.20, optionally `delta` for side-by-side diffs.

## Architecture

**Event loop** (`main.rs`): Single tokio async loop at 50ms tick rate. Key events produce `Action` variants, which are executed asynchronously. File watcher events are polled each tick with 500ms debounce.

**App state** (`app.rs`): Centralized state in `App` struct. Modal UI with `AppMode` (Normal/Diff/NewWorkspace/Help) and `interacting` flag that toggles between navigation mode (hjkl between panes) and interaction mode (keys forwarded to active pane). Three panes: `WorkspaceList`, `FileList`, `MainPanel`.

**PTY** (`pty/`): `PtySession` wraps portable-pty (sync) with `spawn_blocking` for non-blocking reads. `vt100::Parser` accumulates terminal state, rendered by `tui-term`. `input.rs` converts crossterm key events to PTY byte sequences.

**Workspace management** (`workspace/`): `WorkspaceManager` handles git worktree CRUD. Worktrees stored in `.agent-multi/worktrees/<name>` with branches `agent-multi/<name>`. `FileWatcher` uses `notify` crate with mpsc channels.

**Diff pipeline** (`diff/runner.rs`): Runs `git diff | delta` (with plain `git diff` fallback), converts ANSI output to ratatui `Text` via `ansi-to-tui`.

**UI** (`ui/`): `layout.rs` is the main render function composing all panels. Sub-modules render individual components (terminal, diff, workspaces, files, tabs, statusbar).
