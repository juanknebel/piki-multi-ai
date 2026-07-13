use crossterm::event::{KeyCode, KeyEvent};

use crate::config::Config;

/// Result of processing a fuzzy input key event.
pub(crate) enum FuzzyAction {
    /// User pressed Esc -- caller should dismiss the overlay
    Dismiss,
    /// User pressed Enter -- caller should execute the selection
    Select,
    /// Key was handled (query/selection updated) -- no further action
    Handled,
    /// Key was not recognized -- caller may handle it (e.g., Ctrl+O in fuzzy)
    NotHandled,
}

/// Handle common fuzzy overlay input: exit, open, up, down, Backspace, Char.
///
/// The navigation keys come from the `[keybindings.fuzzy]` table, which governs
/// both fuzzy overlays (the file search and the command palette). They are
/// checked *before* the `Char` fallback that feeds the query, so binding one of
/// them to a bare letter would make that letter untypable — the defaults are all
/// special keys for that reason.
///
/// - `cfg`: resolves the `[keybindings.fuzzy]` bindings
/// - `query`: the search query string (mutated on Backspace/Char)
/// - `selected`: currently selected index (mutated on up/down)
/// - `matched_count`: number of currently matched items
/// - `pattern`: the nucleo pattern object for reparsing
/// - `reset_selection`: whether to reset `selected` to 0 on query change
pub(crate) fn handle_fuzzy_input(
    cfg: &Config,
    query: &mut String,
    selected: &mut usize,
    matched_count: usize,
    pattern: &mut nucleo::pattern::MultiPattern,
    key: KeyEvent,
    reset_selection: bool,
) -> FuzzyAction {
    if cfg.matches_fuzzy(key, "exit") {
        return FuzzyAction::Dismiss;
    }
    if cfg.matches_fuzzy(key, "open") {
        return FuzzyAction::Select;
    }
    if cfg.matches_fuzzy(key, "up") {
        if *selected > 0 {
            *selected -= 1;
        }
        return FuzzyAction::Handled;
    }
    if cfg.matches_fuzzy(key, "down") {
        if matched_count > 0 && *selected + 1 < matched_count {
            *selected += 1;
        }
        return FuzzyAction::Handled;
    }

    match key.code {
        KeyCode::Backspace => {
            query.pop();
            if reset_selection {
                *selected = 0;
            }
            let q = query.clone();
            pattern.reparse(0, &q, nucleo::pattern::CaseMatching::Smart, false);
            FuzzyAction::Handled
        }
        KeyCode::Char(c) => {
            query.push(c);
            if reset_selection {
                *selected = 0;
            }
            let q = query.clone();
            pattern.reparse(0, &q, nucleo::pattern::CaseMatching::Smart, true);
            FuzzyAction::Handled
        }
        _ => FuzzyAction::NotHandled,
    }
}
