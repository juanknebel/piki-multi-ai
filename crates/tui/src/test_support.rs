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
