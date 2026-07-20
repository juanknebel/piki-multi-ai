use crossterm::event::{KeyCode, KeyEvent};

use crate::config::Config;

/// Result of processing a text input key event
pub(crate) enum TextInputResult {
    /// Key was consumed by text editing
    Consumed,
    /// Key was not a text editing key
    NotConsumed,
}

/// Fast cursor-to-byte-index conversion. For ASCII-only strings (common case),
/// cursor == byte_idx so we skip the O(n) char_indices scan entirely.
#[inline]
fn cursor_to_byte(buf: &str, cursor: usize) -> usize {
    if buf.is_ascii() {
        cursor.min(buf.len())
    } else {
        buf.char_indices()
            .nth(cursor)
            .map_or(buf.len(), |(i, _)| i)
    }
}

/// Handle common text editing keys (Char, Backspace, Delete, Left, Right, Home, End).
/// `validator` returns true if the char should be inserted.
pub(crate) fn handle_text_input(
    buf: &mut String,
    cursor: &mut usize,
    key: KeyEvent,
    validator: impl Fn(char) -> bool,
) -> TextInputResult {
    match key.code {
        KeyCode::Char(c) => {
            if validator(c) {
                let byte_idx = cursor_to_byte(buf, *cursor);
                buf.insert(byte_idx, c);
                *cursor += 1;
            }
            TextInputResult::Consumed
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                *cursor -= 1;
                let byte_idx = cursor_to_byte(buf, *cursor);
                if byte_idx < buf.len() {
                    buf.remove(byte_idx);
                }
            }
            TextInputResult::Consumed
        }
        KeyCode::Delete => {
            let byte_idx = cursor_to_byte(buf, *cursor);
            if byte_idx < buf.len() {
                buf.remove(byte_idx);
            }
            TextInputResult::Consumed
        }
        KeyCode::Left => {
            if *cursor > 0 {
                *cursor -= 1;
            }
            TextInputResult::Consumed
        }
        KeyCode::Right => {
            let len = if buf.is_ascii() {
                buf.len()
            } else {
                buf.chars().count()
            };
            if *cursor < len {
                *cursor += 1;
            }
            TextInputResult::Consumed
        }
        KeyCode::Home => {
            *cursor = 0;
            TextInputResult::Consumed
        }
        KeyCode::End => {
            *cursor = if buf.is_ascii() {
                buf.len()
            } else {
                buf.chars().count()
            };
            TextInputResult::Consumed
        }
        _ => TextInputResult::NotConsumed,
    }
}

/// Insert a string at the cursor position in one operation (for bracketed paste).
pub(crate) fn bulk_insert(buf: &mut String, cursor: &mut usize, text: &str) {
    let byte_idx = cursor_to_byte(buf, *cursor);
    buf.insert_str(byte_idx, text);
    *cursor += if text.is_ascii() {
        text.len()
    } else {
        text.chars().count()
    };
}

/// Handle a paste event by inserting text into the active dialog's text field.
pub(crate) fn handle_bulk_insert(app: &mut crate::app::App, text: &str) {
    use crate::app::AppMode;
    use crate::dialog_state::DialogState;

    match app.mode {
        AppMode::EditAgentRole => {
            if let Some(DialogState::EditAgentRole {
                ref mut role,
                ref mut role_cursor,
                ..
            }) = app.active_dialog
            {
                bulk_insert(role, role_cursor, text);
            }
        }
        AppMode::EditAgent => {
            if let Some(DialogState::EditAgent {
                ref mut name,
                ref mut name_cursor,
                active_field,
                ..
            }) = app.active_dialog
                && active_field == crate::dialog_state::EditAgentField::Name
            {
                let filtered: String = text
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                bulk_insert(name, name_cursor, &filtered);
            }
        }
        AppMode::NewWorkspace => {
            // Delegate to the active field in the workspace dialog
            if let Some(DialogState::NewWorkspace {
                dir,
                dir_cursor,
                destination,
                destination_cursor,
                desc,
                desc_cursor,
                prompt,
                prompt_cursor,
                active_field,
                ..
            }) = &mut app.active_dialog
            {
                use crate::app::DialogField;
                match active_field {
                    DialogField::Directory => bulk_insert(dir, dir_cursor, text),
                    DialogField::Destination => bulk_insert(destination, destination_cursor, text),
                    DialogField::Description => bulk_insert(desc, desc_cursor, text),
                    DialogField::Prompt => bulk_insert(prompt, prompt_cursor, text),
                    _ => {}
                }
            }
        }
        AppMode::EditWorkspace => {
            if let Some(DialogState::EditWorkspace {
                kanban,
                kanban_cursor,
                prompt,
                prompt_cursor,
                active_field,
                ..
            }) = &mut app.active_dialog
            {
                use crate::dialog_state::EditWorkspaceField;
                match active_field {
                    EditWorkspaceField::KanbanPath => bulk_insert(kanban, kanban_cursor, text),
                    EditWorkspaceField::Prompt => bulk_insert(prompt, prompt_cursor, text),
                }
            }
        }
        AppMode::CreateWorktree => {
            if let Some(DialogState::CreateWorktree {
                name,
                name_cursor,
                prompt,
                prompt_cursor,
                kanban,
                kanban_cursor,
                active_field,
                ..
            }) = &mut app.active_dialog
            {
                use crate::dialog_state::CreateWorktreeField;
                match active_field {
                    CreateWorktreeField::Name => bulk_insert(name, name_cursor, text),
                    CreateWorktreeField::Prompt => bulk_insert(prompt, prompt_cursor, text),
                    CreateWorktreeField::KanbanPath => bulk_insert(kanban, kanban_cursor, text),
                }
            }
        }
        AppMode::EditProvider => {
            if let Some(DialogState::EditProvider {
                name,
                name_cursor,
                description,
                desc_cursor,
                command,
                command_cursor,
                default_args,
                args_cursor,
                prompt_flag,
                flag_cursor,
                agent_dir,
                agent_dir_cursor,
                active_field,
                ..
            }) = &mut app.active_dialog
            {
                use crate::dialog_state::EditProviderField;
                match active_field {
                    EditProviderField::Name => bulk_insert(name, name_cursor, text),
                    EditProviderField::Description => bulk_insert(description, desc_cursor, text),
                    EditProviderField::Command => bulk_insert(command, command_cursor, text),
                    EditProviderField::DefaultArgs => bulk_insert(default_args, args_cursor, text),
                    EditProviderField::PromptFlag => bulk_insert(prompt_flag, flag_cursor, text),
                    EditProviderField::AgentDir => bulk_insert(agent_dir, agent_dir_cursor, text),
                    EditProviderField::PromptFormat | EditProviderField::Dispatchable => {}
                }
            }
        }
        AppMode::PrPicker => {
            if let Some(DialogState::PrPicker {
                repo_browse: crate::dialog_state::RepoBrowse::Input { text: buf, cursor },
                ..
            }) = &mut app.active_dialog
            {
                bulk_insert(buf, cursor, text);
            }
        }
        AppMode::DispatchAgent => {
            if let Some(DialogState::DispatchAgent {
                ref mut additional_prompt,
                ref mut additional_prompt_cursor,
                ..
            }) = app.active_dialog
            {
                bulk_insert(additional_prompt, additional_prompt_cursor, text);
            }
        }
        _ => {}
    }
}

/// Esc, or the prefix key — the two universal "back out of this dialog" keys.
///
/// The prefix arm used to hardcode `Ctrl+G`, so rebinding `prefix_key` silently
/// lost it; it now resolves through the config like every other binding.
pub(crate) fn is_cancel(key: KeyEvent, cfg: &Config) -> bool {
    key.code == KeyCode::Esc || cfg.is_prefix_key(key)
}
