use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{App, AppMode};
use crate::config::parse_key_event;

pub(super) fn handle_command_palette_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.command_palette = None;
            app.mode = AppMode::Normal;
        }
        KeyCode::Up => {
            if let Some(ref mut state) = app.command_palette
                && state.selected > 0
            {
                state.selected -= 1;
            }
        }
        KeyCode::Down => {
            if let Some(ref mut state) = app.command_palette {
                let count = state.nucleo.snapshot().matched_item_count() as usize;
                if count > 0 && state.selected + 1 < count {
                    state.selected += 1;
                }
            }
        }
        KeyCode::Enter => {
            let selected = app
                .command_palette
                .as_ref()
                .and_then(|s| s.selected_command())
                .map(|cmd| (cmd.id, cmd.switch_workspace_idx));

            if let Some((id, switch_idx)) = selected {
                // Close palette first
                app.command_palette = None;
                app.mode = AppMode::Normal;
                // Execute the command
                return execute_palette_command(app, id, switch_idx);
            }
        }
        KeyCode::Backspace => {
            if let Some(ref mut state) = app.command_palette {
                state.query.pop();
                state.selected = 0;
                let query = state.query.clone();
                state.nucleo.pattern.reparse(
                    0,
                    &query,
                    nucleo::pattern::CaseMatching::Smart,
                    false,
                );
            }
            app.needs_redraw = true;
        }
        KeyCode::Char(c) => {
            if let Some(ref mut state) = app.command_palette {
                state.query.push(c);
                state.selected = 0;
                let query = state.query.clone();
                state
                    .nucleo
                    .pattern
                    .reparse(0, &query, nucleo::pattern::CaseMatching::Smart, true);
            }
            app.needs_redraw = true;
        }
        _ => {}
    }
    None
}

/// Execute a palette command by looking up its keybinding and dispatching through navigation mode.
fn execute_palette_command(
    app: &mut App,
    id: &str,
    switch_idx: Option<usize>,
) -> Option<Action> {
    // Handle dynamic workspace switch commands
    if let Some(idx) = switch_idx {
        app.switch_workspace(idx);
        return None;
    }

    let binding = app.config.get_binding("navigation", id);
    if binding == "???" {
        app.status_message = Some(format!("Unknown command: {}", id));
        return None;
    }
    if let Some(key_event) = parse_key_event(&binding) {
        return super::handle_navigation_mode(app, key_event);
    }
    None
}
