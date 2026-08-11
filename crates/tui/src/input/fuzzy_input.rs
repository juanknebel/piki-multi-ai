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
            app.set_toast("Not a markdown file", crate::app::ToastLevel::Error);
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
            app.set_toast("Not a markdown file", crate::app::ToastLevel::Error);
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

/// The hit currently selected in the project-search overlay, resolved to a
/// (full path, 1-based line) pair.
fn selected_hit_full(app: &App) -> Option<(std::path::PathBuf, u32)> {
    let state = app.project_search.as_ref()?;
    let (rel, line) = state.selected_hit()?;
    Some((state.root.join(rel), line))
}

/// Project-wide content search overlay. Shares the `[keybindings.fuzzy]`
/// navigation vocabulary with the other fuzzy overlays; any other typed key
/// edits the query, which re-runs the debounced ripgrep search.
pub(super) fn handle_project_search_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    use crossterm::event::KeyCode;

    let cfg = &app.config;

    // Open in $EDITOR at the hit's line without closing the overlay.
    if cfg.matches_fuzzy(key, "editor") {
        return selected_hit_full(app).map(|(path, line)| Action::OpenEditorAt(path, line));
    }

    // Open the inline editor (closes the overlay; it has no line jump).
    if cfg.matches_fuzzy(key, "inline_edit") {
        if let Some((path, _line)) = selected_hit_full(app) {
            app.project_search = None;
            app.open_inline_editor(path);
        }
        return None;
    }

    if cfg.matches_fuzzy(key, "exit") {
        app.project_search = None;
        app.mode = AppMode::Normal;
        return None;
    }

    if cfg.matches_fuzzy(key, "open") {
        if let Some((path, line)) = selected_hit_full(app) {
            app.project_search = None;
            app.mode = AppMode::Normal;
            return Some(Action::OpenEditorAt(path, line));
        }
        return None;
    }

    // Navigation before the Char fallback, same rule as the fuzzy overlays.
    let hit_count = {
        let state = app.project_search.as_ref()?;
        state.shared.lock().hits.len()
    };
    if cfg.matches_fuzzy(key, "up") {
        let state = app.project_search.as_mut()?;
        if state.selected > 0 {
            state.selected -= 1;
        }
        app.needs_redraw = true;
        return None;
    }
    if cfg.matches_fuzzy(key, "down") {
        let state = app.project_search.as_mut()?;
        if hit_count > 0 && state.selected + 1 < hit_count {
            state.selected += 1;
        }
        app.needs_redraw = true;
        return None;
    }

    match key.code {
        KeyCode::Backspace => {
            let state = app.project_search.as_mut()?;
            state.query.pop();
            state.query_changed();
            app.needs_redraw = true;
        }
        KeyCode::Char(c) => {
            let state = app.project_search.as_mut()?;
            state.query.push(c);
            state.query_changed();
            app.needs_redraw = true;
        }
        _ => {}
    }
    None
}
