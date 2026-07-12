use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppMode};

pub(super) fn handle_inline_edit_input(app: &mut App, key: KeyEvent) -> Option<super::Action> {
    // `[keybindings.editor]` bindings first — they must pre-empt the `Char`
    // fallback below, which types everything else into the buffer.
    if app.config.matches_editor(key, "exit") {
        app.editor = None;
        app.editing_file = None;
        app.mode = AppMode::Normal;
        return None;
    }
    if app.config.matches_editor(key, "save") {
        if let (Some(editor), Some(path)) = (&app.editor, &app.editing_file) {
            let content = editor.contents();
            match std::fs::write(path, &content) {
                Ok(()) => {
                    app.status_message = Some(format!("Saved: {}", path.display()));
                    if let Some(ws) = app.current_workspace_mut() {
                        ws.dirty = true;
                    }
                }
                Err(e) => {
                    app.status_message = Some(format!("Save error: {}", e));
                }
            }
        }
        return None;
    }

    match key.code {
        KeyCode::Up => {
            if let Some(ref mut editor) = app.editor {
                editor.move_up();
            }
        }
        KeyCode::Down => {
            if let Some(ref mut editor) = app.editor {
                editor.move_down();
            }
        }
        KeyCode::Left => {
            if let Some(ref mut editor) = app.editor {
                editor.move_left();
            }
        }
        KeyCode::Right => {
            if let Some(ref mut editor) = app.editor {
                editor.move_right();
            }
        }
        KeyCode::Enter => {
            if let Some(ref mut editor) = app.editor {
                editor.enter();
            }
        }
        KeyCode::Backspace => {
            if let Some(ref mut editor) = app.editor {
                editor.backspace();
            }
        }
        KeyCode::Char(c) => {
            if let Some(ref mut editor) = app.editor {
                editor.insert_char(c);
            }
        }
        KeyCode::Tab => {
            if let Some(ref mut editor) = app.editor {
                // Insert 4 spaces
                for _ in 0..4 {
                    editor.insert_char(' ');
                }
            }
        }
        _ => {}
    }
    // Keep cursor visible after any edit
    if let Some(ref mut editor) = app.editor {
        editor.adjust_scroll(app.pty_rows.saturating_sub(4) as usize);
    }
    None
}
