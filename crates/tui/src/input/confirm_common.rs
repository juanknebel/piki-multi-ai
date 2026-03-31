use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppMode};

pub(crate) enum ConfirmResult {
    Yes,
    No,
    Cancel,
    NotHandled,
}

pub(crate) fn handle_yn_input(key: KeyEvent) -> ConfirmResult {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => ConfirmResult::Yes,
        KeyCode::Char('n') | KeyCode::Char('N') => ConfirmResult::No,
        KeyCode::Esc => ConfirmResult::Cancel,
        _ => ConfirmResult::NotHandled,
    }
}

pub(crate) fn dismiss_dialog(app: &mut App) {
    app.active_dialog = None;
    app.mode = AppMode::Normal;
}
