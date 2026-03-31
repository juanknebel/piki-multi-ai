use crossterm::event::KeyEvent;

use crate::action::Action;
use crate::app::{App, AppMode};

use super::fuzzy_common::{FuzzyAction, handle_fuzzy_input};

pub(super) fn handle_workspace_switcher_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let state = app.workspace_switcher.as_mut()?;

    let matched_count = state.nucleo.snapshot().matched_item_count() as usize;
    let action = handle_fuzzy_input(
        &mut state.query,
        &mut state.selected,
        matched_count,
        &mut state.nucleo.pattern,
        key,
        true, // workspace switcher resets selection on query change
    );

    match action {
        FuzzyAction::Dismiss => {
            app.workspace_switcher = None;
            app.mode = AppMode::Normal;
        }
        FuzzyAction::Select => {
            if let Some(ref state) = app.workspace_switcher
                && let Some(idx) = state.selected_index()
            {
                app.workspace_switcher = None;
                app.mode = AppMode::Normal;
                app.switch_workspace(idx);
            }
        }
        FuzzyAction::Handled | FuzzyAction::NotHandled => {}
    }

    // Tick the nucleo matcher so results update
    if let Some(ref mut s) = app.workspace_switcher {
        s.nucleo.tick(10);
    }

    None
}
