use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::{App, AppMode};
use crate::workspace_switcher::SwitcherRow;

use super::list_nav::move_selection;

pub(super) fn handle_workspace_switcher_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let is_cancel = super::text_field_common::is_cancel(key, app.config.platform);
    let state = app.workspace_switcher.as_mut()?;

    if is_cancel {
        app.workspace_switcher = None;
        app.mode = AppMode::Normal;
        return None;
    }

    let total = state.rows.len();
    match key.code {
        KeyCode::Enter => {
            if let Some(row) = state.selected_row() {
                app.workspace_switcher = None;
                app.mode = AppMode::Normal;
                match row {
                    SwitcherRow::Workspace { ws_idx } => app.switch_workspace(ws_idx),
                    SwitcherRow::Tab { ws_idx, tab_idx } => {
                        super::interaction::jump_to_agent(app, (ws_idx, tab_idx));
                    }
                }
            }
            return None;
        }
        // Navigation: arrows plus Ctrl+P/Ctrl+N (readline prev/next). We can't
        // use bare j/k here because this overlay also has a search field — those
        // letters must be typeable into the query. Down/up move by one visible
        // row (tabs and workspaces intermixed). No wrap so the ends feel like a list.
        KeyCode::Down => move_selection(&mut state.selected, total, 1, false),
        KeyCode::Up => move_selection(&mut state.selected, total, -1, false),
        KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
            move_selection(&mut state.selected, total, 1, false)
        }
        KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
            move_selection(&mut state.selected, total, -1, false)
        }
        // Search: printable chars extend the query; backspace trims it. Both
        // refilter the tree and re-clamp the selection.
        KeyCode::Backspace => {
            state.query.pop();
            state.refilter();
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.query.push(c);
            state.refilter();
        }
        _ => {}
    }

    None
}
