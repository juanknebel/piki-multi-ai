//! Test helpers shared between rendering snapshot tests (`ui/mod.rs`) and
//! input handler unit tests (`input/dialog_tests.rs`). Compiled only under
//! `#[cfg(test)]`. Items are `allow(dead_code)` because not every test module
//! uses every helper.

#![allow(dead_code)]

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;

pub(crate) fn test_storage() -> std::sync::Arc<piki_core::storage::AppStorage> {
    std::sync::Arc::new(piki_core::storage::AppStorage {
        workspaces: Box::new(piki_core::storage::json::JsonStorage),
        api_history: None,
        ui_prefs: None,
        agent_profiles: None,
    })
}

pub(crate) fn test_app() -> App {
    App::new(test_storage(), &piki_core::paths::DataPaths::default_paths())
}

/// Build an `App` whose `DataPaths` resolve under an isolated temp directory.
/// Use this for tests that exercise handler paths writing to `providers.toml`,
/// the SQLite DB, or any other on-disk state — otherwise they pollute the
/// real user config dir (`~/.config/piki-multi`) and break other tests'
/// snapshots. Returns the `App` together with the `TempDir` guard; keep the
/// guard alive for the duration of the test or paths will be deleted.
pub(crate) fn test_app_isolated() -> (App, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("create temp dir for test");
    let paths = piki_core::paths::DataPaths::new(tmp.path().to_path_buf());
    let app = App::new(test_storage(), &paths);
    (app, tmp)
}

pub(crate) fn test_terminal(w: u16, h: u16) -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(w, h)).unwrap()
}

/// Convert buffer to snapshot-friendly string (trimmed trailing whitespace per line).
pub(crate) fn buffer_to_snapshot(buf: &ratatui::buffer::Buffer) -> String {
    let area = buf.area();
    let mut lines = Vec::new();
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            let cell = &buf[(x, y)];
            line.push_str(cell.symbol());
        }
        lines.push(line.trim_end().to_string());
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Build a `KeyEvent` with no modifiers — convenient shorthand for tests.
pub(crate) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Build a `KeyEvent` with custom modifiers.
pub(crate) fn key_with_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

// ── Workspaces and tabs ───────────────────────────────────────────────
//
// `Workspace` is perfectly unit-testable: `Workspace::from_info` is a plain
// synchronous constructor and every PTY/git field defaults to empty (no
// session, no watcher, no git calls). Attach a bare `vt100::Parser` when a
// test needs a terminal to look at. These helpers used to be private to
// `app.rs`'s test module, which made everything workspace-shaped look
// untestable from the other test modules.

/// A `WorkspaceInfo` with sane defaults. `source_repo` is derived from `order`
/// so separate calls produce standalone workspaces; pass the same
/// `source_repo` to two of them to build a worktree "family".
pub(crate) fn test_ws_info(name: &str, order: u32) -> piki_core::WorkspaceInfo {
    piki_core::WorkspaceInfo {
        name: name.to_string(),
        path: std::path::PathBuf::from("/tmp/test"),
        workspace_type: piki_core::WorkspaceType::Simple,
        description: String::new(),
        prompt: String::new(),
        kanban_path: None,
        order,
        source_repo: std::path::PathBuf::from(format!("/tmp/test-{order}")),
        source_repo_display: String::new(),
        dispatch_card_id: None,
        dispatch_source_kanban: None,
        dispatch_agent_name: None,
        origin: piki_core::WorkspaceOrigin::default(),
        is_git_repo: true,
        ephemeral: false,
        pr_repo_nwo: None,
        pr_number: None,
    }
}

/// Push a standalone workspace onto `app` and return its index.
pub(crate) fn add_test_workspace(app: &mut App) -> usize {
    let idx = app.workspaces.len();
    let info = test_ws_info(&format!("test-ws-{idx}"), idx as u32);
    app.workspaces.push(crate::app::Workspace::from_info(info));
    app.workspaces.len() - 1
}

/// Give the active tab of `ws_idx` a live in-memory terminal (a bare vt100
/// parser, no real PTY) so terminal-dependent paths see a searchable pane.
pub(crate) fn add_terminal_tab(app: &mut App, ws_idx: usize) -> usize {
    let ws = &mut app.workspaces[ws_idx];
    let idx = ws.add_tab(piki_core::AIProvider::Shell, true, None);
    ws.tabs[idx].pty_parser = Some(std::sync::Arc::new(parking_lot::Mutex::new(
        vt100::Parser::new(24, 80, 0),
    )));
    ws.active_tab = idx;
    idx
}

/// Add a Custom-provider (agent) tab to `ws_idx` and return its index.
pub(crate) fn add_agent_tab(app: &mut App, ws_idx: usize, name: &str) -> usize {
    app.workspaces[ws_idx].add_tab(piki_core::AIProvider::Custom(name.to_string()), true, None)
}
