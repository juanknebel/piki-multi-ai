use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
        AppMode::NewWorkspace | AppMode::EditWorkspace => {
            // Delegate to the active field in the workspace dialog
            if let Some(ref mut dialog) = app.active_dialog {
                match dialog {
                    DialogState::NewWorkspace {
                        name,
                        name_cursor,
                        dir,
                        dir_cursor,
                        desc,
                        desc_cursor,
                        prompt,
                        prompt_cursor,
                        active_field,
                        ..
                    } => {
                        use crate::app::DialogField;
                        match active_field {
                            DialogField::Name => bulk_insert(name, name_cursor, text),
                            DialogField::Directory => bulk_insert(dir, dir_cursor, text),
                            DialogField::Description => bulk_insert(desc, desc_cursor, text),
                            DialogField::Prompt => bulk_insert(prompt, prompt_cursor, text),
                            _ => {}
                        }
                    }
                    DialogState::CommitMessage { buffer } => {
                        buffer.push_str(text);
                    }
                    _ => {}
                }
            }
        }
        AppMode::CommitMessage => {
            if let Some(DialogState::CommitMessage { ref mut buffer }) = app.active_dialog {
                buffer.push_str(text);
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

pub(crate) fn is_cancel(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL))
}
