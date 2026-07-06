use std::sync::Arc;

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{ActivePane, App, AppMode};
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::OpenEditor(path) => {
            // Suspend TUI, open $EDITOR, restore TUI
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::PopKeyboardEnhancementFlags,
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableBracketedPaste,
            )?;
            ratatui::restore();
            let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let status = std::process::Command::new(&editor_cmd).arg(&path).status();
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
        Action::OpenDiff(file_idx) => {
            if let Some(ws) = app.workspaces.get(app.active_workspace)
                && let Some(file) = ws.changed_files.get(file_idx)
            {
                let file_path = file.path.clone();
                // Compute diff width from actual terminal size (matches diff overlay: 90% width minus borders)
                let term_size = terminal.size()?;
                let overlay_inner_width = (term_size.width * 90 / 100).saturating_sub(2);
                let width = if overlay_inner_width > 10 {
                    overlay_inner_width
                } else {
                    120
                };
                let cache_key = format!("{}@{}", file_path, width);
                // Check cache first to avoid re-running git diff | delta
                if let Some(cached) = app.diff_cache.get(&cache_key) {
                    app.diff_content = Some(Arc::clone(cached));
                    app.diff_file_path = Some(file_path);
                    app.diff_scroll = 0;
                    app.mode = AppMode::Diff;
                    app.active_pane = ActivePane::MainPanel;
                } else {
                    let worktree_path = ws.path.clone();
                    let file_status = file.status.clone();
                    match piki_core::diff::runner::run_diff(
                        &worktree_path,
                        &file_path,
                        width,
                        &file_status,
                    )
                    .await
                    {
                        Ok(ansi_bytes) => {
                            use ansi_to_tui::IntoText;
                            match ansi_bytes.into_text() {
                                Ok(text) => {
                                    let text = Arc::new(text);
                                    app.insert_diff_cache(cache_key, Arc::clone(&text));
                                    app.diff_content = Some(text);
                                    app.diff_file_path = Some(file_path);
                                    app.diff_scroll = 0;
                                    app.mode = AppMode::Diff;
                                    app.active_pane = ActivePane::MainPanel;
                                }
                                Err(e) => {
                                    app.status_message =
                                        Some(format!("Failed to parse diff: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Diff error: {}", e));
                        }
                    }
                }
            }
        }
        other => unreachable!("non-file action routed to action::files: {other:?}"),
    }
    Ok(())
}
