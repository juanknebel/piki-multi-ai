use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{App, AppMode};

pub(super) fn handle_workspace_switcher_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let state = app.workspace_switcher.as_mut()?;

    match key.code {
        KeyCode::Esc => {
            app.workspace_switcher = None;
            app.mode = AppMode::Normal;
        }
        KeyCode::Enter => {
            if let Some(idx) = state.selected_index() {
                app.workspace_switcher = None;
                app.mode = AppMode::Normal;
                app.switch_workspace(idx);
            }
        }
        KeyCode::Up => {
            if state.selected > 0 {
                state.selected -= 1;
            }
        }
        KeyCode::Down => {
            let snap = state.nucleo.snapshot();
            let count = snap.matched_item_count() as usize;
            if state.selected + 1 < count {
                state.selected += 1;
            }
        }
        KeyCode::Backspace => {
            state.query.pop();
            state.selected = 0;
            let query = state.query.clone();
            state
                .nucleo
                .pattern
                .reparse(0, &query, nucleo::pattern::CaseMatching::Smart, false);
        }
        KeyCode::Char(c) => {
            state.query.push(c);
            state.selected = 0;
            let query = state.query.clone();
            state
                .nucleo
                .pattern
                .reparse(0, &query, nucleo::pattern::CaseMatching::Smart, true);
        }
        _ => {}
    }

    // Tick the nucleo matcher so results update
    if let Some(ref mut s) = app.workspace_switcher {
        s.nucleo.tick(10);
    }

    None
}
