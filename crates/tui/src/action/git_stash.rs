use std::sync::Arc;

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{ActivePane, App, AppMode, ToastLevel};
use crate::dialog_state::DialogState;
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::GitStashList => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let entries = parse_stash_list(&worktree).await;
            app.active_dialog = Some(DialogState::GitStash {
                entries,
                selected: 0,
                scroll: 0,
                input_mode: false,
                input_buffer: String::new(),
                input_cursor: 0,
            });
            app.mode = AppMode::GitStash;
        }
        Action::GitStashSave(message) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let output = tokio::process::Command::new("git")
                .args(["stash", "push", "-m", &message])
                .current_dir(&worktree)
                .output()
                .await?;
            if output.status.success() {
                app.set_toast(format!("Stashed: {}", message), ToastLevel::Success);
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.dirty = true;
                    ws.last_refresh = None;
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                app.set_toast(
                    format!("Stash failed: {}", stderr.trim()),
                    ToastLevel::Error,
                );
            }
            // Refresh stash list in the overlay
            let entries = parse_stash_list(&worktree).await;
            if let Some(DialogState::GitStash {
                entries: ref mut e,
                ref mut selected,
                ..
            }) = app.active_dialog
            {
                *e = entries;
                if *selected >= e.len() {
                    *selected = e.len().saturating_sub(1);
                }
            }
        }
        Action::GitStashPop(idx) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let stash_ref = format!("stash@{{{}}}", idx);
            let output = tokio::process::Command::new("git")
                .args(["stash", "pop", &stash_ref])
                .current_dir(&worktree)
                .output()
                .await?;
            if output.status.success() {
                app.set_toast(format!("Popped {}", stash_ref), ToastLevel::Success);
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.dirty = true;
                    ws.last_refresh = None;
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                app.set_toast(format!("Pop failed: {}", stderr.trim()), ToastLevel::Error);
            }
            let entries = parse_stash_list(&worktree).await;
            if let Some(DialogState::GitStash {
                entries: ref mut e,
                ref mut selected,
                ..
            }) = app.active_dialog
            {
                *e = entries;
                if *selected >= e.len() {
                    *selected = e.len().saturating_sub(1);
                }
            }
        }
        Action::GitStashApply(idx) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let stash_ref = format!("stash@{{{}}}", idx);
            let output = tokio::process::Command::new("git")
                .args(["stash", "apply", &stash_ref])
                .current_dir(&worktree)
                .output()
                .await?;
            if output.status.success() {
                app.set_toast(format!("Applied {}", stash_ref), ToastLevel::Success);
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.dirty = true;
                    ws.last_refresh = None;
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                app.set_toast(
                    format!("Apply failed: {}", stderr.trim()),
                    ToastLevel::Error,
                );
            }
        }
        Action::GitStashDrop(idx) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let stash_ref = format!("stash@{{{}}}", idx);
            let output = tokio::process::Command::new("git")
                .args(["stash", "drop", &stash_ref])
                .current_dir(&worktree)
                .output()
                .await?;
            if output.status.success() {
                app.set_toast(format!("Dropped {}", stash_ref), ToastLevel::Success);
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                app.set_toast(format!("Drop failed: {}", stderr.trim()), ToastLevel::Error);
            }
            let entries = parse_stash_list(&worktree).await;
            if let Some(DialogState::GitStash {
                entries: ref mut e,
                ref mut selected,
                ..
            }) = app.active_dialog
            {
                *e = entries;
                if *selected >= e.len() {
                    *selected = e.len().saturating_sub(1);
                }
            }
        }
        Action::GitStashShow(idx) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let stash_ref = format!("stash@{{{}}}", idx);
            let term_size = terminal.size()?;
            let overlay_inner_width = (term_size.width * 90 / 100).saturating_sub(2);
            let width = if overlay_inner_width > 10 {
                overlay_inner_width
            } else {
                120
            };
            // Try delta first, fall back to plain git stash show
            let ansi_bytes = match super::run_git_diff_with_delta(
                &worktree,
                &["stash", "show", "-p", "--color=always", &stash_ref],
                width,
            ).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    let output = tokio::process::Command::new("git")
                        .args(["stash", "show", "-p", "--color=always", &stash_ref])
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
                    app.diff_file_path = Some(format!("stash: {}", stash_ref));
                    app.diff_scroll = 0;
                    app.mode = AppMode::Diff;
                    app.active_pane = ActivePane::MainPanel;
                }
                Err(e) => {
                    app.status_message = Some(format!("Failed to parse stash diff: {}", e));
                }
            }
        }
        other => unreachable!("non-stash action routed to action::git_stash: {other:?}"),
    }
    Ok(())
}

/// Parse `git stash list` output into (ref, message) pairs.
async fn parse_stash_list(worktree: &std::path::Path) -> Vec<(String, String)> {
    let output = tokio::process::Command::new("git")
        .args(["stash", "list"])
        .current_dir(worktree)
        .output()
        .await
        .ok();
    match output {
        Some(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .filter_map(|line| {
                    // Format: "stash@{0}: On main: my message" or "stash@{0}: WIP on main: ..."
                    let colon_pos = line.find(':')?;
                    let stash_ref = line[..colon_pos].trim().to_string();
                    let message = line[colon_pos + 1..].trim().to_string();
                    Some((stash_ref, message))
                })
                .collect()
        }
        _ => Vec::new(),
    }
}
