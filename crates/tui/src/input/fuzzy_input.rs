use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::{App, AppMode};

use super::fuzzy_common::{FuzzyAction, handle_fuzzy_input};

pub(super) fn handle_fuzzy_search_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Handle fuzzy-specific keybindings first (before common keys)
    match key.code {
        // Ctrl+O: open markdown file in a new tab
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.selected_path())
                .map(String::from);

            if let (Some(rel_path), Some(ws)) = (selected_path, app.current_workspace()) {
                if rel_path.ends_with(".md") || rel_path.ends_with(".markdown") {
                    let full_path = ws.path.join(&rel_path);
                    app.fuzzy = None;
                    app.mode = AppMode::Normal;
                    return Some(Action::OpenMarkdown(full_path));
                } else {
                    app.status_message = Some("Not a markdown file".to_string());
                }
            }
            return None;
        }
        // Alt+M: open markdown file in external mdr viewer
        KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::ALT) => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.selected_path())
                .map(String::from);

            if let (Some(rel_path), Some(ws)) = (selected_path, app.current_workspace()) {
                if rel_path.ends_with(".md") || rel_path.ends_with(".markdown") {
                    let full_path = ws.path.join(&rel_path);
                    return Some(Action::OpenMdr(full_path));
                } else {
                    app.status_message = Some("Not a markdown file".to_string());
                }
            }
            return None;
        }
        // Ctrl+E: open in $EDITOR
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.selected_path())
                .map(String::from);

            if let (Some(rel_path), Some(ws)) = (selected_path, app.current_workspace()) {
                let full_path = ws.path.join(&rel_path);
                return Some(Action::OpenEditor(full_path));
            }
            return None;
        }
        // Ctrl+V: open inline editor
        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.selected_path())
                .map(String::from);

            if let Some(rel_path) = selected_path
                && let Some(ws) = app.current_workspace()
            {
                let full_path = ws.path.join(&rel_path);
                app.fuzzy = None;
                app.open_inline_editor(full_path);
            }
            return None;
        }
        _ => {}
    }

    // Delegate common keys to the shared handler
    let state = app.fuzzy.as_mut()?;
    let matched_count = state.nucleo.snapshot().matched_item_count() as usize;
    let action = handle_fuzzy_input(
        &mut state.query,
        &mut state.selected,
        matched_count,
        &mut state.nucleo.pattern,
        key,
        false, // fuzzy search does NOT reset selection on query change
    );

    match action {
        FuzzyAction::Dismiss => {
            app.fuzzy = None;
            app.mode = AppMode::Normal;
        }
        FuzzyAction::Select => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.selected_path())
                .map(String::from);

            if let Some(path) = selected_path {
                // Check if file is in changed_files list; if so, open its diff
                if let Some(ws) = app.current_workspace() {
                    if let Some(idx) = ws.changed_files.iter().position(|f| f.path == path) {
                        app.fuzzy = None;
                        app.mode = AppMode::Normal;
                        app.selected_file = idx;
                        return Some(Action::OpenDiff(idx));
                    } else {
                        app.status_message = Some(format!("{} has no changes to diff", path));
                    }
                }
            }
        }
        FuzzyAction::Handled => {
            app.needs_redraw = true;
        }
        FuzzyAction::NotHandled => {}
    }
    None
}
