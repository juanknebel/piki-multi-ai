use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{App, AppMode, ToastLevel};
use crate::code_review::CodeReviewState;
use crate::dialog_state::DialogState;
use crate::helpers::spawn_tab;
use piki_core::workspace::WorkspaceManager;
use piki_core::AIProvider;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::SpawnTab(provider) => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                if provider == AIProvider::Kanban && ws.kanban_app.is_none() {
                    // Initialize Kanban app if it doesn't exist yet for this workspace
                    let kanban_path_opt = ws.kanban_path.clone();

                    let mut kanban_provider = if app.config.kanban.provider == "jira" {
                        Box::new(flow_core::provider_jira::JiraProvider::from_env())
                            as Box<dyn flow_core::provider::Provider>
                    } else {
                        let default_path = kanban_path_opt
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| {
                                app.config
                                    .kanban
                                    .path
                                    .clone()
                                    .map(std::path::PathBuf::from)
                                    .unwrap_or_else(|| {
                                        piki_core::xdg::home_dir()
                                            .join(".config/flow/boards/default")
                                    })
                            });

                        let expanded_path = if let Some(rest) = default_path
                            .to_str()
                            .and_then(|path_str| path_str.strip_prefix("~/"))
                        {
                            if let Ok(home) = std::env::var("HOME") {
                                std::path::PathBuf::from(home).join(rest)
                            } else {
                                default_path
                            }
                        } else {
                            default_path
                        };

                        // Initialize if board.txt doesn't exist
                        let board_txt = expanded_path.join("board.txt");
                        if !board_txt.exists() {
                            if let Err(e) = std::fs::create_dir_all(&expanded_path) {
                                app.status_message =
                                    Some(format!("Failed to create kanban dir: {}", e));
                            } else {
                                let board_content = "col todo \"TO DO\"\ncol in_progress \"IN PROGRESS\"\ncol in_review \"IN REVIEW\"\ncol done \"DONE\"\n";
                                if let Err(e) = std::fs::write(&board_txt, board_content) {
                                    app.status_message =
                                        Some(format!("Failed to write board.txt: {}", e));
                                } else {
                                    for col in &["todo", "in_progress", "in_review", "done"] {
                                        let col_dir = expanded_path.join("cols").join(col);
                                        let _ = std::fs::create_dir_all(&col_dir);
                                        let _ = std::fs::write(col_dir.join("order.txt"), "");
                                    }
                                }
                            }
                        }

                        Box::new(flow_core::provider_local::LocalProvider::new(expanded_path))
                            as Box<dyn flow_core::provider::Provider>
                    };

                    let board = kanban_provider
                        .load_board()
                        .unwrap_or_else(|_e| flow_core::Board { columns: vec![] });
                    let mut kanban = flow_tui::App::new(board);
                    if kanban.board.columns.is_empty() {
                        kanban.banner =
                            Some("Load failed or empty board. Check board.txt.".to_string());
                    }
                    ws.kanban_app = Some(kanban);
                    ws.kanban_provider = Some(kanban_provider);
                }

                // Singleton guard: Kanban, Api and Git tabs must not be duplicated
                if matches!(
                    provider,
                    AIProvider::Kanban | AIProvider::Api | AIProvider::Git
                ) && let Some(idx) = ws.tabs.iter().position(|t| t.provider == provider)
                {
                    ws.active_tab = idx;
                    return Ok(());
                }

                let idx = spawn_tab(ws, &provider, app.pty_rows, app.pty_cols, None, Some(&app.provider_manager), &app.paths, app.pty_output.clone()).await;
                ws.active_tab = idx;
                app.status_message = Some(format!("Opened {} tab", provider.label()));
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

            // Code Review: check gh availability (lazy, cached) then load PR data
            if provider == AIProvider::CodeReview {
                // Lazy gh CLI check — run once, cache forever
                if app.gh_available.is_none() {
                    let gh_ok = tokio::process::Command::new("gh")
                        .arg("--version")
                        .output()
                        .await
                        .is_ok_and(|o| o.status.success());
                    let auth_ok = if gh_ok {
                        tokio::process::Command::new("gh")
                            .args(["auth", "status"])
                            .output()
                            .await
                            .is_ok_and(|o| o.status.success())
                    } else {
                        false
                    };
                    app.gh_available = Some(gh_ok && auth_ok);
                    if !gh_ok {
                        app.set_toast(
                            "gh CLI not found — install from https://cli.github.com/",
                            ToastLevel::Error,
                        );
                    } else if !auth_ok {
                        app.set_toast(
                            "gh not authenticated — run `gh auth login`",
                            ToastLevel::Error,
                        );
                    }
                }
                if app.gh_available != Some(true) {
                    // Remove the tab we just created
                    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                        && ws
                            .current_tab()
                            .is_some_and(|t| t.provider == AIProvider::CodeReview)
                    {
                        ws.close_tab(ws.active_tab);
                    }
                    return Ok(());
                }
                let worktree_path = app
                    .workspaces
                    .get(app.active_workspace)
                    .map(|ws| ws.info.path.clone());
                if let Some(worktree_path) = worktree_path {
                    match piki_core::github::get_pr_for_branch(&worktree_path).await {
                        Ok(Some(pr_info)) => {
                            match piki_core::github::get_pr_files(&worktree_path).await {
                                Ok(files) => {
                                    if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                                        ws.code_review = Some(CodeReviewState::new(pr_info, files));
                                    }
                                    app.set_toast("PR loaded", ToastLevel::Success);
                                }
                                Err(e) => {
                                    app.set_toast(
                                        format!("Failed to load PR files: {}", e),
                                        ToastLevel::Error,
                                    );
                                }
                            }
                        }
                        Ok(None) => {
                            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                                && ws
                                    .current_tab()
                                    .is_some_and(|t| t.provider == AIProvider::CodeReview)
                            {
                                ws.close_tab(ws.active_tab);
                            }
                            app.set_toast("No open PR for this branch", ToastLevel::Error);
                        }
                        Err(e) => {
                            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                                && ws
                                    .current_tab()
                                    .is_some_and(|t| t.provider == AIProvider::CodeReview)
                            {
                                ws.close_tab(ws.active_tab);
                            }
                            app.set_toast(format!("gh error: {}", e), ToastLevel::Error);
                        }
                    }
                }
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
                    app.status_message = Some(format!("Opened {}", label));
                }
            }
            Err(e) => {
                app.status_message = Some(format!("Failed to read file: {}", e));
            }
        },
        Action::OpenMdr(path) => {
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
                    app.status_message = Some(format!("mdr: {}", path.display()));
                }
                Ok(s) => {
                    app.status_message = Some(format!("mdr exited with: {}", s));
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
