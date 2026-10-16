use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{App, AppMode};
use crate::dialog_state::DialogState;
use crate::helpers::spawn_tab;
use piki_core::AIProvider;
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::SpawnTab(provider) => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                // Singleton guard: Api and Git tabs must not be duplicated
                if matches!(provider, AIProvider::Api | AIProvider::Git)
                    && let Some(idx) = ws.tabs.iter().position(|t| t.provider == provider)
                {
                    ws.active_tab = idx;
                    app.active_pane = crate::app::ActivePane::MainPanel;
                    return Ok(());
                }

                let (idx, spawn_error) = spawn_tab(
                    ws,
                    &provider,
                    app.pty_rows,
                    app.pty_cols,
                    None,
                    Some(&app.provider_manager),
                    &app.paths,
                    app.session_daemon.clone(),
                    app.pty_output.clone(),
                )
                .await;
                ws.active_tab = idx;
                app.active_pane = crate::app::ActivePane::MainPanel;
                match spawn_error {
                    Some(err) => app.set_toast(err, crate::app::ToastLevel::Error),
                    None => app.set_toast(
                        format!("Opened {} tab", provider.label()),
                        crate::app::ToastLevel::Success,
                    ),
                }
            }

            // The tab is up either way; warn that its status will be guessed
            // rather than read, so "alive" in the Agents pane isn't a mystery.
            if let Some((agent, missing)) =
                crate::helpers::missing_bridge_prereqs(&provider, &app.provider_manager)
            {
                tracing::warn!(
                    %agent, ?missing,
                    "agent hook bridge disabled — falling back to the idle heuristic"
                );
                app.active_dialog = Some(DialogState::MissingPrereqs { agent, missing });
                app.mode = AppMode::MissingPrereqs;
            }
        }
        Action::RestartTab(target) => {
            // The key handler already checked that this tab's process exited;
            // re-check here because the action runs a turn later.
            let Some(ws) = app.workspaces.get_mut(app.active_workspace) else {
                return Ok(());
            };
            let Some(tab) = ws.tabs.get(target) else {
                return Ok(());
            };
            if tab.pty_session.as_ref().is_none_or(|p| p.peek_alive()) {
                return Ok(());
            }
            let provider = tab.provider.clone();
            let title = tab.custom_title.clone();
            let closable = tab.closable;
            let dead_session_id = tab.session_id.clone();

            // Drop the dead tab by hand: `close_tab` refuses a non-closable
            // one (the initial shell), and that is the tab most worth getting
            // back. Its process is already gone, so there is nothing to kill.
            ws.tabs.remove(target);
            let (spawned, spawn_error) = spawn_tab(
                ws,
                &provider,
                app.pty_rows,
                app.pty_cols,
                None,
                Some(&app.provider_manager),
                &app.paths,
                app.session_daemon.clone(),
                app.pty_output.clone(),
            )
            .await;
            // `spawn_tab` appends; put the tab back where it was so the tab
            // bar doesn't reshuffle under the user, and carry over what the
            // tab was (custom title, closability).
            let mut tab = ws.tabs.remove(spawned);
            tab.custom_title = title.clone();
            tab.closable = closable;
            let new_session_id = tab.session_id.clone();
            ws.tabs.insert(target, tab);
            ws.active_tab = target;
            app.active_pane = crate::app::ActivePane::MainPanel;

            // The old session exited; drop it so it can't linger as an orphan
            // in the sessions overlay, and re-apply the title to the new one
            // so the rename survives the next restart too.
            if let Some(sid) = dead_session_id {
                crate::helpers::remove_session(app, &sid);
            }
            if let (Some(sid), Some(_)) = (new_session_id.as_deref(), title.as_deref()) {
                crate::helpers::rename_session(app, sid, title.clone());
            }

            match spawn_error {
                Some(err) => app.set_toast(err, crate::app::ToastLevel::Error),
                None => app.set_toast(
                    format!("Restarted {}", provider.label()),
                    crate::app::ToastLevel::Success,
                ),
            }
        }
        Action::ShowScratchTerminal => {
            if app.scratch.pty_session.is_none() {
                crate::helpers::spawn_scratch_terminal(app, app.pty_rows, app.pty_cols).await;
            }
            if app.scratch.pty_session.is_some() {
                app.scratch.visible = true;
                app.scratch.prefix_pending = false;
                app.mode = AppMode::ScratchTerminal;
            }
        }
        Action::OpenMarkdown(path) => match std::fs::read_to_string(&path) {
            Ok(content) => {
                let label = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| "markdown".to_string());
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.add_markdown_tab(label.clone(), content, Some(&app.syntax));
                    app.active_pane = crate::app::ActivePane::MainPanel;
                    app.set_toast(format!("Opened {}", label), crate::app::ToastLevel::Success);
                }
            }
            Err(e) => {
                app.set_toast(
                    format!("Failed to read file: {}", e),
                    crate::app::ToastLevel::Error,
                );
            }
        },
        Action::OpenMdr(path) => {
            let _watchdog = crate::watchdog::suspend();
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::PopKeyboardEnhancementFlags,
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableBracketedPaste,
            )?;
            ratatui::restore();
            let status = std::process::Command::new("mdr").arg(&path).status();
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
                    app.set_toast(
                        format!("mdr: {}", path.display()),
                        crate::app::ToastLevel::Info,
                    );
                }
                Ok(s) => {
                    app.set_toast(
                        format!("mdr exited with: {}", s),
                        crate::app::ToastLevel::Info,
                    );
                }
                Err(_) => {
                    app.status_message =
                        Some("mdr not found. Install: cargo install markdown-reader".to_string());
                }
            }
            if app.mode == AppMode::FuzzySearch {
                app.fuzzy = None;
                app.mode = AppMode::Normal;
            }
        }
        other => unreachable!("non-tab action routed to action::tabs: {other:?}"),
    }
    Ok(())
}
