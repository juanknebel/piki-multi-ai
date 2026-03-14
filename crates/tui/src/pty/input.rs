use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Convert a crossterm KeyEvent into the raw bytes to send to the PTY.
/// Returns None for key combinations we don't handle.
pub fn key_to_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    // Shift+Enter or Ctrl+Enter — send newline (LF) so CLIs like Claude/Gemini insert a line break
    // Shift+Enter requires the Kitty keyboard protocol; Ctrl+Enter works as fallback
    if (key.modifiers.contains(KeyModifiers::SHIFT)
        || key.modifiers.contains(KeyModifiers::CONTROL))
        && key.code == KeyCode::Enter
    {
        return Some(vec![10]); // LF
    }

    // Ctrl + letter
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && let KeyCode::Char(c) = key.code
    {
        // Ctrl+A = 1, Ctrl+B = 2, ..., Ctrl+Z = 26
        let byte = (c.to_ascii_lowercase() as u8)
            .wrapping_sub(b'a')
            .wrapping_add(1);
        if (1..=26).contains(&byte) {
            return Some(vec![byte]);
        }
    }

    match key.code {
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            Some(s.as_bytes().to_vec())
        }
        KeyCode::Enter => Some(vec![13]),      // CR
        KeyCode::Backspace => Some(vec![127]), // DEL
        KeyCode::Tab => Some(vec![9]),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()), // Shift+Tab
        KeyCode::Esc => Some(vec![27]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(n) => f_key_bytes(n),
        _ => None,
    }
}

fn f_key_bytes(n: u8) -> Option<Vec<u8>> {
    let seq = match n {
        1 => b"\x1bOP".as_slice(),
        2 => b"\x1bOQ",
        3 => b"\x1bOR",
        4 => b"\x1bOS",
        5 => b"\x1b[15~",
        6 => b"\x1b[17~",
        7 => b"\x1b[18~",
        8 => b"\x1b[19~",
        9 => b"\x1b[20~",
        10 => b"\x1b[21~",
        11 => b"\x1b[23~",
        12 => b"\x1b[24~",
        _ => return None,
    };
    Some(seq.to_vec())
}
