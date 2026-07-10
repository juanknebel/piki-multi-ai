use crossterm::event::KeyEvent;

use crate::action::Action;
use crate::app::{App, AppMode};
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

/// Execute a palette command by dispatching its app action directly.
fn execute_palette_command(app: &mut App, id: &str, switch_idx: Option<usize>) -> Option<Action> {
    // Handle dynamic workspace switch commands
    if let Some(idx) = switch_idx {
        app.switch_workspace_and_focus(idx);
        return None;
    }

    // Commands with hardcoded keys (not in config)
    match id {
        "manage_agents" => {
            if let Some(ref storage) = app.storage.agent_profiles
                && let Some(ws) = app.current_workspace()
            {
                let repo = ws.source_repo.clone();
                if let Ok(agents) = storage.load_agents(&repo) {
                    app.agent_profiles = agents;
                }
            }
            app.active_dialog = Some(crate::dialog_state::DialogState::ManageAgents { selected: 0 });
            app.mode = crate::app::AppMode::ManageAgents;
            return None;
        }
        "manage_providers" => {
            app.active_dialog = Some(crate::dialog_state::DialogState::ManageProviders { selected: 0 });
            app.mode = crate::app::AppMode::ManageProviders;
            return None;
        }
        // Actions that live outside the app keybinding table
        "delete_workspace" => return super::app_actions::open_delete_workspace(app),
        "copy" => {
            crate::helpers::copy_visible_terminal(app);
            return None;
        }
        _ => {}
    }

    if super::APP_ACTIONS.contains(&id) {
        return super::dispatch_app_action(app, id);
    }
    app.status_message = Some(format!("Unknown command: {}", id));
    None
}
