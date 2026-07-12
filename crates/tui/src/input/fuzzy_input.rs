use crossterm::event::KeyEvent;

use crate::action::Action;
use crate::app::{App, AppMode};

use super::fuzzy_common::{FuzzyAction, handle_fuzzy_input};

/// The path currently selected in the fuzzy overlay, resolved against the
/// active workspace.
fn selected_full_path(app: &App) -> Option<std::path::PathBuf> {
    let rel = app.fuzzy.as_ref()?.selected_path()?.to_string();
    Some(app.current_workspace()?.path.join(rel))
}

fn is_markdown(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md" | "markdown")
    )
}

pub(super) fn handle_fuzzy_search_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Fuzzy-specific bindings first — they must pre-empt the shared handler's
    // `Char` fallback, which would otherwise type them into the query.
    let cfg = &app.config;

    // Open a markdown file in a new tab.
    if cfg.matches_fuzzy(key, "markdown") {
        let path = selected_full_path(app)?;
        if !is_markdown(&path) {
            app.status_message = Some("Not a markdown file".to_string());
            return None;
        }
        app.fuzzy = None;
        app.mode = AppMode::Normal;
        return Some(Action::OpenMarkdown(path));
    }

    // Open a markdown file in the external `mdr` viewer.
    if cfg.matches_fuzzy(key, "mdr") {
        let path = selected_full_path(app)?;
        if !is_markdown(&path) {
            app.status_message = Some("Not a markdown file".to_string());
            return None;
        }
        return Some(Action::OpenMdr(path));
    }

    // Open in $EDITOR.
    if cfg.matches_fuzzy(key, "editor") {
        return selected_full_path(app).map(Action::OpenEditor);
    }

    // Open the inline editor.
    if cfg.matches_fuzzy(key, "inline_edit") {
        if let Some(path) = selected_full_path(app) {
            app.fuzzy = None;
            app.open_inline_editor(path);
        }
        return None;
    }

    // Delegate common keys to the shared handler. `config` and `fuzzy` are
    // disjoint fields, so the shared borrow of one coexists with the mutable
    // borrow of the other.
    let cfg = &app.config;
    let state = app.fuzzy.as_mut()?;
    let matched_count = state.nucleo.snapshot().matched_item_count() as usize;
    let action = handle_fuzzy_input(
        cfg,
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
            // Enter opens the file in $EDITOR (diffs live in the lazygit tab now)
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.selected_path())
                .map(String::from);

            if let Some(path) = selected_path
                && let Some(ws) = app.current_workspace()
            {
                let full_path = ws.path.join(&path);
                app.fuzzy = None;
                app.mode = AppMode::Normal;
                return Some(Action::OpenEditor(full_path));
            }
        }
        FuzzyAction::Handled => {
            app.needs_redraw = true;
        }
        FuzzyAction::NotHandled => {}
    }
    None
}
