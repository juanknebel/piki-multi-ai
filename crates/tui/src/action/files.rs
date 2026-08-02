use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{App, AppMode};
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::OpenEditor(..) | Action::OpenEditorAt(..) => {
            let (path, line) = match action {
                Action::OpenEditor(path) => (path, None),
                Action::OpenEditorAt(path, line) => (path, Some(line)),
                _ => unreachable!(),
            };
            // Suspend TUI, open $EDITOR, restore TUI
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::PopKeyboardEnhancementFlags,
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableBracketedPaste,
            )?;
            ratatui::restore();
            let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let mut cmd = std::process::Command::new(&editor_cmd);
            // `+N` positions the cursor — the vi/vim/nano/emacs/micro
            // convention; editors that don't understand it open the file
            // anyway and treat it as an extra argument at worst.
            if let Some(line) = line {
                cmd.arg(format!("+{line}"));
            }
            let status = cmd.arg(&path).status();
            *terminal = ratatui::init();
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::EnableMouseCapture,
                crossterm::event::EnableBracketedPaste,
                crossterm::event::PushKeyboardEnhancementFlags(
                    crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                )
            )?;
            match status {
                Ok(s) if s.success() => {
                    if let Some(ws) = app.current_workspace_mut() {
                        ws.dirty = true;
                    }
                    app.status_message = Some(format!("Edited: {}", path.display()));
                }
                Ok(s) => {
                    app.status_message = Some(format!("Editor exited with: {}", s));
                }
                Err(e) => {
                    app.status_message = Some(format!("Failed to run {}: {}", editor_cmd, e));
                }
            }
            // Close fuzzy search if it was open
            if app.mode == AppMode::FuzzySearch {
                app.fuzzy = None;
                app.mode = AppMode::Normal;
            }
        }

        other => unreachable!("non-file action routed to action::files: {other:?}"),
    }
    Ok(())
}
