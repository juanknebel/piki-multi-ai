use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Result of processing a text input key event
pub(crate) enum TextInputResult {
    /// Key was consumed by text editing
    Consumed,
    /// Key was not a text editing key
    NotConsumed,
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
                let byte_idx = buf
                    .char_indices()
                    .nth(*cursor)
                    .map_or(buf.len(), |(i, _)| i);
                buf.insert(byte_idx, c);
                *cursor += 1;
            }
            TextInputResult::Consumed
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                *cursor -= 1;
                let byte_idx = buf
                    .char_indices()
                    .nth(*cursor)
                    .map_or(buf.len(), |(i, _)| i);
                buf.remove(byte_idx);
            }
            TextInputResult::Consumed
        }
        KeyCode::Delete => {
            if let Some((byte_idx, _)) = buf.char_indices().nth(*cursor) {
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
            let len = buf.chars().count();
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
            *cursor = buf.chars().count();
            TextInputResult::Consumed
        }
        _ => TextInputResult::NotConsumed,
    }
}

pub(crate) fn is_cancel(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL))
}
