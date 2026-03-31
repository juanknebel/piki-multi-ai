use crossterm::event::{KeyCode, KeyEvent};

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

/// Handle common fuzzy overlay input: Esc, Enter, Up, Down, Backspace, Char.
///
/// - `query`: the search query string (mutated on Backspace/Char)
/// - `selected`: currently selected index (mutated on Up/Down)
/// - `matched_count`: number of currently matched items
/// - `pattern`: the nucleo pattern object for reparsing
/// - `reset_selection`: whether to reset `selected` to 0 on query change
pub(crate) fn handle_fuzzy_input(
    query: &mut String,
    selected: &mut usize,
    matched_count: usize,
    pattern: &mut nucleo::pattern::MultiPattern,
    key: KeyEvent,
    reset_selection: bool,
) -> FuzzyAction {
    match key.code {
        KeyCode::Esc => FuzzyAction::Dismiss,
        KeyCode::Enter => FuzzyAction::Select,
        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
            }
            FuzzyAction::Handled
        }
        KeyCode::Down => {
            if matched_count > 0 && *selected + 1 < matched_count {
                *selected += 1;
            }
            FuzzyAction::Handled
        }
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
