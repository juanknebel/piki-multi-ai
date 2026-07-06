use std::sync::Arc;

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{self, ActivePane, App, AppMode, ToastLevel};
use crate::dialog_state::{DialogState, GitLogEntry};
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::GitStage(file_idx) => {
            let ws_idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(ws_idx)
                && let Some(file) = ws.changed_files.get(file_idx)
            {
                let file_path = file.path.clone();
                let worktree = ws.path.clone();
                let status_tx = app.status_tx.clone();
                let undo_tx = app.undo_tx.clone();
                app.status_message = Some(format!("Staging: {}", file_path));
                ws.dirty = true;
                ws.last_refresh = None;
                tokio::spawn(async move {
                    let output = tokio::process::Command::new("git")
                        .args(["add", &file_path])
                        .current_dir(&worktree)
                        .output()
                        .await;
                    match output {
                        Ok(o) if o.status.success() => {
                            let _ = undo_tx.send(app::UndoEntry {
                                action: app::UndoAction::Stage,
                                workspace_idx: ws_idx,
                                file_path: file_path.clone(),
                            });
                            let _ = status_tx.send(format!("Staged: {} [C-z undo]", file_path));
                        }
                        Ok(o) => {
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            let _ = status_tx.send(format!("Stage failed: {}", stderr.trim()));
                        }
                        Err(e) => {
                            let _ = status_tx.send(format!("Stage error: {}", e));
                        }
                    }
                });
            }
        }
        Action::GitUnstage(file_idx) => {
            let ws_idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(ws_idx)
                && let Some(file) = ws.changed_files.get(file_idx)
            {
                let file_path = file.path.clone();
                let worktree = ws.path.clone();
                let status_tx = app.status_tx.clone();
                let undo_tx = app.undo_tx.clone();
                app.status_message = Some(format!("Unstaging: {}", file_path));
                ws.dirty = true;
                ws.last_refresh = None;
                tokio::spawn(async move {
                    let output = tokio::process::Command::new("git")
                        .args(["reset", "HEAD", &file_path])
                        .current_dir(&worktree)
                        .output()
                        .await;
                    match output {
                        Ok(o) if o.status.success() => {
                            let _ = undo_tx.send(app::UndoEntry {
                                action: app::UndoAction::Unstage,
                                workspace_idx: ws_idx,
                                file_path: file_path.clone(),
                            });
                            let _ = status_tx.send(format!("Unstaged: {} [C-z undo]", file_path));
                        }
                        Ok(o) => {
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            let _ = status_tx.send(format!("Unstage failed: {}", stderr.trim()));
                        }
                        Err(e) => {
                            let _ = status_tx.send(format!("Unstage error: {}", e));
                        }
                    }
                });
            }
        }
        Action::GitStageSelected => {
            let ws_idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(ws_idx) {
                let paths: Vec<String> = app.selected_files.iter().cloned().collect();
                if !paths.is_empty() {
                    let worktree = ws.path.clone();
                    let status_tx = app.status_tx.clone();
                    let undo_tx = app.undo_tx.clone();
                    let count = paths.len();
                    app.status_message = Some(format!("Staging {} files...", count));
                    ws.dirty = true;
                    ws.last_refresh = None;
                    app.selected_files.clear();
                    tokio::spawn(async move {
                        let mut args = vec!["add".to_string()];
                        args.extend(paths.iter().cloned());
                        let output = tokio::process::Command::new("git")
                            .args(&args)
                            .current_dir(&worktree)
                            .output()
                            .await;
                        match output {
                            Ok(o) if o.status.success() => {
                                for p in &paths {
                                    let _ = undo_tx.send(app::UndoEntry {
                                        action: app::UndoAction::Stage,
                                        workspace_idx: ws_idx,
                                        file_path: p.clone(),
                                    });
                                }
                                let _ =
                                    status_tx.send(format!("Staged {} files [C-z undo]", count));
                            }
                            Ok(o) => {
                                let stderr = String::from_utf8_lossy(&o.stderr);
                                let _ =
                                    status_tx.send(format!("Stage failed: {}", stderr.trim()));
                            }
                            Err(e) => {
                                let _ = status_tx.send(format!("Stage error: {}", e));
                            }
                        }
                    });
                }
            }
        }
        Action::GitUnstageSelected => {
            let ws_idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(ws_idx) {
                let paths: Vec<String> = app.selected_files.iter().cloned().collect();
                if !paths.is_empty() {
                    let worktree = ws.path.clone();
                    let status_tx = app.status_tx.clone();
                    let undo_tx = app.undo_tx.clone();
                    let count = paths.len();
                    app.status_message = Some(format!("Unstaging {} files...", count));
                    ws.dirty = true;
                    ws.last_refresh = None;
                    app.selected_files.clear();
                    tokio::spawn(async move {
                        let mut args = vec!["reset".to_string(), "HEAD".to_string()];
                        args.extend(paths.iter().cloned());
                        let output = tokio::process::Command::new("git")
                            .args(&args)
                            .current_dir(&worktree)
                            .output()
                            .await;
                        match output {
                            Ok(o) if o.status.success() => {
                                for p in &paths {
                                    let _ = undo_tx.send(app::UndoEntry {
                                        action: app::UndoAction::Unstage,
                                        workspace_idx: ws_idx,
                                        file_path: p.clone(),
                                    });
                                }
                                let _ = status_tx
                                    .send(format!("Unstaged {} files [C-z undo]", count));
                            }
                            Ok(o) => {
                                let stderr = String::from_utf8_lossy(&o.stderr);
                                let _ =
                                    status_tx.send(format!("Unstage failed: {}", stderr.trim()));
                            }
                            Err(e) => {
                                let _ = status_tx.send(format!("Unstage error: {}", e));
                            }
                        }
                    });
                }
            }
        }
        Action::GitCommit(message) => {
            let toast = if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                let worktree = ws.path.clone();
                let output = tokio::process::Command::new("git")
                    .args(["commit", "-m", &message])
                    .current_dir(&worktree)
                    .output()
                    .await?;
                let result = if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let first_line = stdout.lines().next().unwrap_or("Committed").to_string();
                    (format!("✓ {}", first_line), ToastLevel::Success)
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    (
                        format!("Commit failed: {}", stderr.trim()),
                        ToastLevel::Error,
                    )
                };
                ws.dirty = true;
                ws.last_refresh = None;
                Some(result)
            } else {
                None
            };
            if let Some((msg, lvl)) = toast {
                app.set_toast(msg, lvl);
            }
        }
        Action::GitPush => {
            let toast = if let Some(ws) = app.workspaces.get(app.active_workspace) {
                let worktree = ws.path.clone();
                let output = tokio::process::Command::new("git")
                    .args(["push"])
                    .current_dir(&worktree)
                    .output()
                    .await?;
                let result = if output.status.success() {
                    ("✓ Pushed successfully".to_string(), ToastLevel::Success)
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    (format!("Push failed: {}", stderr.trim()), ToastLevel::Error)
                };
                Some(result)
            } else {
                None
            };
            if let Some((msg, lvl)) = toast {
                app.set_toast(msg, lvl);
            }
        }
        Action::Undo => {
            if let Some(entry) = app.undo_stack.pop_back() {
                // Validate workspace still exists
                if let Some(ws) = app.workspaces.get_mut(entry.workspace_idx) {
                    let worktree = ws.path.clone();
                    let file_path = entry.file_path.clone();
                    let status_tx = app.status_tx.clone();
                    ws.dirty = true;
                    ws.last_refresh = None;
                    match entry.action {
                        app::UndoAction::Stage => {
                            // Undo a stage = unstage
                            tokio::spawn(async move {
                                let output = tokio::process::Command::new("git")
                                    .args(["reset", "HEAD", &file_path])
                                    .current_dir(&worktree)
                                    .output()
                                    .await;
                                match output {
                                    Ok(o) if o.status.success() => {
                                        let _ =
                                            status_tx.send(format!("✓ Undo stage: {}", file_path));
                                    }
                                    Ok(o) => {
                                        let stderr = String::from_utf8_lossy(&o.stderr);
                                        let _ = status_tx
                                            .send(format!("Undo failed: {}", stderr.trim()));
                                    }
                                    Err(e) => {
                                        let _ = status_tx.send(format!("Undo error: {}", e));
                                    }
                                }
                            });
                        }
                        app::UndoAction::Unstage => {
                            // Undo an unstage = re-stage
                            tokio::spawn(async move {
                                let output = tokio::process::Command::new("git")
                                    .args(["add", &file_path])
                                    .current_dir(&worktree)
                                    .output()
                                    .await;
                                match output {
                                    Ok(o) if o.status.success() => {
                                        let _ = status_tx
                                            .send(format!("✓ Undo unstage: {}", file_path));
                                    }
                                    Ok(o) => {
                                        let stderr = String::from_utf8_lossy(&o.stderr);
                                        let _ = status_tx
                                            .send(format!("Undo failed: {}", stderr.trim()));
                                    }
                                    Err(e) => {
                                        let _ = status_tx.send(format!("Undo error: {}", e));
                                    }
                                }
                            });
                        }
                    }
                } else {
                    app.set_toast(
                        "Undo failed: workspace no longer exists".to_string(),
                        ToastLevel::Error,
                    );
                }
            } else {
                app.set_toast("Nothing to undo".to_string(), ToastLevel::Info);
            }
        }
        Action::LoadGitLog => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let output = tokio::process::Command::new("git")
                .args(["log", "--oneline", "--graph", "--decorate", "--all", "-50"])
                .current_dir(&worktree)
                .output()
                .await?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<GitLogEntry> = stdout
                .lines()
                .map(|line| {
                    // Extract SHA: skip graph chars (*, |, /, \, space), then look for hex
                    let trimmed = line.trim_start_matches(|c: char| "*|/\\ ".contains(c));
                    let sha = trimmed
                        .split_whitespace()
                        .next()
                        .filter(|s| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit()))
                        .map(String::from);
                    GitLogEntry {
                        raw_line: line.to_string(),
                        sha,
                    }
                })
                .collect();
            app.active_dialog = Some(DialogState::GitLog {
                lines,
                selected: 0,
                scroll: 0,
            });
            app.mode = AppMode::GitLog;
        }
        Action::ViewCommitDiff(sha) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            // Compute diff width from terminal size (matches diff overlay: 90% width minus borders)
            let term_size = terminal.size()?;
            let overlay_inner_width = (term_size.width * 90 / 100).saturating_sub(2);
            let width = if overlay_inner_width > 10 {
                overlay_inner_width
            } else {
                120
            };
            // Try delta first, fall back to plain git show
            let ansi_bytes = match super::run_git_diff_with_delta(&worktree, &["show", "--color=always", &sha], width).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    // Fallback: plain git show with color
                    let output = tokio::process::Command::new("git")
                        .args(["show", "--color=always", "--stat", "-p", &sha])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                    output.stdout
                }
            };
            use ansi_to_tui::IntoText;
            match ansi_bytes.into_text() {
                Ok(text) => {
                    let text = Arc::new(text);
                    app.diff_content = Some(text);
                    app.diff_file_path = Some(format!("commit {}", sha));
                    app.diff_scroll = 0;
                    app.mode = AppMode::Diff;
                    app.active_pane = ActivePane::MainPanel;
                }
                Err(e) => {
                    app.status_message = Some(format!("Failed to parse commit diff: {}", e));
                }
            }
        }
        other => unreachable!("non-git action routed to action::git: {other:?}"),
    }
    Ok(())
}

