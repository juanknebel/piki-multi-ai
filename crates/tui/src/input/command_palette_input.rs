use crossterm::event::KeyEvent;

use crate::action::Action;
use crate::app::{App, AppMode};
use crate::config::parse_key_event;

use super::fuzzy_common::{FuzzyAction, handle_fuzzy_input};

pub(super) fn handle_command_palette_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let state = app.command_palette.as_mut()?;
    let matched_count = state.nucleo.snapshot().matched_item_count() as usize;
    let action = handle_fuzzy_input(
        &mut state.query,
        &mut state.selected,
        matched_count,
        &mut state.nucleo.pattern,
        key,
        true, // command palette resets selection on query change
    );

    match action {
        FuzzyAction::Dismiss => {
            app.command_palette = None;
            app.mode = AppMode::Normal;
        }
        FuzzyAction::Select => {
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
        FuzzyAction::Handled => {
            app.needs_redraw = true;
        }
        FuzzyAction::NotHandled => {}
    }
    None
}

/// Execute a palette command by looking up its keybinding and dispatching through navigation mode.
fn execute_palette_command(app: &mut App, id: &str, switch_idx: Option<usize>) -> Option<Action> {
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
