use std::sync::Arc;

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{self, App, ToastLevel};
use piki_core::WorkspaceType;
use piki_core::workspace::{FileWatcher, WorkspaceManager};

pub(super) async fn handle(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
    _terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::CreateWorkspace(name, description, prompt, kanban_path, dir, ws_type, group) => {
            let result = match ws_type {
                WorkspaceType::Simple => {
                    manager
                        .create_simple(&name, &description, &prompt, kanban_path, &dir)
                        .await
                }
                WorkspaceType::Worktree => {
                    manager
                        .create(&name, &description, &prompt, kanban_path, &dir)
                        .await
                }
                WorkspaceType::Project => {
                    manager
                        .create_project(&name, &description, &prompt, kanban_path, &dir)
                        .await
                }
            };
            match result {
                Ok(mut info) => {
                    info.group = group;
                    info.order = app
                        .workspaces
                        .iter()
                        .map(|w| w.info.order)
                        .max()
                        .map(|m| m + 1)
                        .unwrap_or(0);
                    app.workspaces.push(app::Workspace::from_info(info));
                    let new_idx = app.workspaces.len() - 1;
                    app.switch_workspace(new_idx);

                    // Start file watcher
                    let ws = &mut app.workspaces[new_idx];
                    match FileWatcher::new(ws.path.clone(), ws.name.clone()) {
                        Ok(watcher) => {
                            ws.watcher = Some(watcher);
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Watcher error: {}", e));
                        }
                    }

                    // Persist config async
                    {
                        let source = app.workspaces[new_idx].source_repo.clone();
                        let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                        let storage = Arc::clone(&app.storage);
                        tokio::spawn(async move {
                            let _ = storage.workspaces.save_workspaces(&source, &infos);
                        });
                    }
                }
                Err(e) => {
                    app.status_message = Some(format!("Error: {}", e));
                }
            }
        }
        Action::CreateGithubWorkspace(
            name,
            description,
            prompt,
            kanban_path,
            github_url,
            destination_dir,
            group,
        ) => {
            let result = manager
                .create_from_github(
                    &name,
                    &description,
                    &prompt,
                    kanban_path,
                    &github_url,
                    &destination_dir,
                )
                .await;
            match result {
                Ok(mut info) => {
                    info.group = group;
                    info.order = app
                        .workspaces
                        .iter()
                        .map(|w| w.info.order)
                        .max()
                        .map(|m| m + 1)
                        .unwrap_or(0);
                    app.workspaces.push(app::Workspace::from_info(info));
                    let new_idx = app.workspaces.len() - 1;
                    app.switch_workspace(new_idx);

                    let ws = &mut app.workspaces[new_idx];
                    match FileWatcher::new(ws.path.clone(), ws.name.clone()) {
                        Ok(watcher) => {
                            ws.watcher = Some(watcher);
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Watcher error: {}", e));
                        }
                    }

                    {
                        let source = app.workspaces[new_idx].source_repo.clone();
                        let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                        let storage = Arc::clone(&app.storage);
                        tokio::spawn(async move {
                            let _ = storage.workspaces.save_workspaces(&source, &infos);
                        });
                    }
                }
                Err(e) => {
                    app.status_message = Some(format!("Error: {}", e));
                }
            }
        }
        Action::EditWorkspace(idx, kanban_path, prompt, group) => {
            if let Some(ws) = app.workspaces.get_mut(idx) {
                if ws.kanban_path != kanban_path {
                    ws.kanban_app = None;
                    ws.kanban_provider = None;
                }
                ws.kanban_path = kanban_path;
                ws.prompt = prompt;
                ws.info.group = group;
                {
                    let source = ws.source_repo.clone();
                    let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                    let storage = Arc::clone(&app.storage);
                    tokio::spawn(async move {
                        let _ = storage.workspaces.save_workspaces(&source, &infos);
                    });
                }
                app.set_toast("Workspace updated", ToastLevel::Success);
            }
        }
        Action::DeleteWorkspace(idx, target_column) => {
            if idx < app.workspaces.len() {
                // If this was a dispatched agent, move card to the chosen column
                let dispatch_info = app.workspaces[idx]
                    .info
                    .dispatch_card_id
                    .clone()
                    .zip(app.workspaces[idx].info.dispatch_source_kanban.clone());
                if let Some((card_id, kanban_path)) = dispatch_info
                    && let Some(target_col) = target_column
                {
                    let source_ws_idx = app.workspaces.iter().position(|w| {
                        w.kanban_path.as_deref() == Some(kanban_path.as_str())
                            && w.kanban_provider.is_some()
                    });
                    if let Some(src_idx) = source_ws_idx {
                        let src_ws = &mut app.workspaces[src_idx];
                        if let Some(ref mut kp) = src_ws.kanban_provider {
                            if let Ok(board) = kp.load_board() {
                                for col in &board.columns {
                                    if let Some(card) = col.cards.iter().find(|c| c.id == card_id) {
                                        let _ = kp.update_card(
                                            &card_id,
                                            &card.title,
                                            &card.description,
                                            card.priority,
                                            "",
                                            &card.project,
                                        );
                                        break;
                                    }
                                }
                            }
                            let _ = kp.move_card(&card_id, &target_col);
                            if let Ok(board) = kp.load_board()
                                && let Some(ref mut ka) = src_ws.kanban_app
                            {
                                ka.board = board;
                                ka.clamp();
                            }
                        }
                    }
                }

                let is_worktree =
                    app.workspaces[idx].info.workspace_type == WorkspaceType::Worktree;

                // Kill all PTY sessions before removing
                for tab in &mut app.workspaces[idx].tabs {
                    if let Some(ref mut pty) = tab.pty_session {
                        let _ = pty.kill();
                    }
                }
                // Drop watcher (stops watching)
                app.workspaces[idx].watcher = None;

                let source_repo = app.workspaces[idx].source_repo.clone();

                let removed = if !is_worktree {
                    // Simple workspaces: just remove from list
                    app.workspaces.remove(idx);
                    true
                } else {
                    let name = app.workspaces[idx].name.clone();
                    match manager.remove(&name, &source_repo).await {
                        Ok(()) => {
                            app.workspaces.remove(idx);
                            true
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                            false
                        }
                    }
                };

                if removed {
                    // Adjust indices
                    if app.workspaces.is_empty() {
                        app.active_workspace = 0;
                        app.selected_workspace = 0;
                    } else {
                        if app.active_workspace >= app.workspaces.len() {
                            app.active_workspace = app.workspaces.len() - 1;
                        }
                        if app.selected_workspace >= app.workspaces.len() {
                            app.selected_workspace = app.workspaces.len() - 1;
                        }
                    }

                    // Persist config
                    {
                        let source = source_repo.clone();
                        let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                        let storage = Arc::clone(&app.storage);
                        tokio::spawn(async move {
                            let _ = storage.workspaces.save_workspaces(&source, &infos);
                        });
                    }
                }
            }
        }
        Action::RemoveFromList(idx) => {
            if idx < app.workspaces.len() {
                // Kill all PTY sessions
                for tab in &mut app.workspaces[idx].tabs {
                    if let Some(ref mut pty) = tab.pty_session {
                        let _ = pty.kill();
                    }
                }
                app.workspaces[idx].watcher = None;

                let source_repo = app.workspaces[idx].source_repo.clone();
                app.workspaces.remove(idx);

                // Adjust indices
                if app.workspaces.is_empty() {
                    app.active_workspace = 0;
                    app.selected_workspace = 0;
                } else {
                    if app.active_workspace >= app.workspaces.len() {
                        app.active_workspace = app.workspaces.len() - 1;
                    }
                    if app.selected_workspace >= app.workspaces.len() {
                        app.selected_workspace = app.workspaces.len() - 1;
                    }
                }

                // Persist config
                {
                    let source = source_repo.clone();
                    let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                    let storage = Arc::clone(&app.storage);
                    tokio::spawn(async move {
                        let _ = storage.workspaces.save_workspaces(&source, &infos);
                    });
                }
            }
        }
        other => unreachable!("non-workspace action routed to action::workspace: {other:?}"),
    }
    Ok(())
}
