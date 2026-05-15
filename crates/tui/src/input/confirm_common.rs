use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{ActivePane, App, AppMode};

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

/// Dismiss + force the active pane (used by handlers that want to drop focus
/// back to a specific pane after closing, e.g. delete-workspace returning to
/// the workspace list).
pub(crate) fn dismiss_dialog_to_pane(app: &mut App, pane: ActivePane) {
    dismiss_dialog(app);
    app.active_pane = pane;
}

/// Pattern-match `app.active_dialog` against a `DialogState::$variant { ... }`
/// and run `$body` with each named field bound as `&mut T`. Returns the body's
/// `Option<Action>` on a match, otherwise `None`. Replaces the
/// `let Some(DialogState::X { ref mut … }) = app.active_dialog else { return None; };`
/// boilerplate at the top of every dialog handler.
macro_rules! with_dialog_mut {
    ($app:expr, $variant:ident { $($pat:tt)* } => $body:block) => {{
        if let Some($crate::dialog_state::DialogState::$variant { $($pat)* }) =
            &mut $app.active_dialog
        {
            $body
        } else {
            None
        }
    }};
}

pub(crate) use with_dialog_mut;
