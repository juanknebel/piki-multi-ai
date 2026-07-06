use std::sync::Arc;

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{ActivePane, App, AppMode, ToastLevel};
use crate::dialog_state::{ConflictFile, ConflictStrategy, DialogState};
use piki_core::workspace::WorkspaceManager;
use piki_core::MergeStrategy;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::GitMerge(strategy) => {
            // Extract data we need and drop the mutable borrow on workspaces
            let merge_data = app.workspaces.get(app.active_workspace).map(|ws| {
                (
                    ws.source_repo.clone(),
                    ws.branch.clone(),
                    ws.path.clone(),
                )
            });
            if let Some((source_repo, branch, ws_path)) = merge_data {
                // Check workspace has no uncommitted changes
                let status_output = tokio::process::Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(&ws_path)
                    .output()
                    .await?;
                let status_str = String::from_utf8_lossy(&status_output.stdout);
                if !status_str.trim().is_empty() {
                    app.set_toast(
                        "Merge aborted: workspace has uncommitted changes",
                        ToastLevel::Error,
                    );
                    return Ok(());
                }

                // Detect main branch
                let main_branch = WorkspaceManager::detect_main_branch(&source_repo).await;

                match strategy {
                    MergeStrategy::Merge => {
                        // Stash source repo if dirty
                        let src_status = tokio::process::Command::new("git")
                            .args(["status", "--porcelain"])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        let src_dirty = !String::from_utf8_lossy(&src_status.stdout)
                            .trim()
                            .is_empty();
                        if src_dirty {
                            tokio::process::Command::new("git")
                                .args(["stash", "push", "-m", "piki-multi-merge-temp"])
                                .current_dir(&source_repo)
                                .output()
                                .await?;
                        }

                        // Save current branch to restore later
                        let prev_branch = tokio::process::Command::new("git")
                            .args(["rev-parse", "--abbrev-ref", "HEAD"])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        let prev = String::from_utf8_lossy(&prev_branch.stdout)
                            .trim()
                            .to_string();

                        // Checkout main
                        let checkout = tokio::process::Command::new("git")
                            .args(["checkout", &main_branch])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        if !checkout.status.success() {
                            let stderr = String::from_utf8_lossy(&checkout.stderr);
                            app.set_toast(
                                format!("Checkout {} failed: {}", main_branch, stderr.trim()),
                                ToastLevel::Error,
                            );
                            if src_dirty {
                                let _ = tokio::process::Command::new("git")
                                    .args(["stash", "pop"])
                                    .current_dir(&source_repo)
                                    .output()
                                    .await;
                            }
                            return Ok(());
                        }

                        // Merge
                        let merge = tokio::process::Command::new("git")
                            .args(["merge", &branch])
                            .current_dir(&source_repo)
                            .output()
                            .await?;

                        let mut has_conflicts = false;
                        if merge.status.success() {
                            let stdout = String::from_utf8_lossy(&merge.stdout);
                            let first = stdout.lines().next().unwrap_or("Merged");
                            app.set_toast(
                                format!("✓ Merged '{}' into {}: {}", branch, main_branch, first),
                                ToastLevel::Success,
                            );
                        } else {
                            // Check for conflict markers in git status
                            let conflict_check = tokio::process::Command::new("git")
                                .args(["status", "--porcelain=v1"])
                                .current_dir(&source_repo)
                                .output()
                                .await?;
                            let conflict_stdout =
                                String::from_utf8_lossy(&conflict_check.stdout);
                            let conflict_files: Vec<ConflictFile> =
                                piki_core::git::parse_porcelain_status(&conflict_stdout)
                                    .into_iter()
                                    .filter(|f| {
                                        matches!(f.status, piki_core::FileStatus::Conflicted)
                                    })
                                    .map(|f| ConflictFile {
                                        path: f.path.clone(),
                                        status: format!("{:?}", f.status),
                                    })
                                    .collect();
                            if !conflict_files.is_empty() {
                                has_conflicts = true;
                                // Open conflict resolution overlay — stay on main branch
                                // so user can resolve conflicts in the source repo
                                app.active_dialog = Some(DialogState::ConflictResolution {
                                    files: conflict_files,
                                    selected: 0,
                                    repo_path: source_repo.clone(),
                                });
                                app.mode = AppMode::ConflictResolution;
                                app.set_toast(
                                    "Merge conflicts detected — resolve below",
                                    ToastLevel::Error,
                                );
                            } else {
                                // Some other merge error, abort
                                let _ = tokio::process::Command::new("git")
                                    .args(["merge", "--abort"])
                                    .current_dir(&source_repo)
                                    .output()
                                    .await;
                                let stderr = String::from_utf8_lossy(&merge.stderr);
                                app.set_toast(
                                    format!("Merge failed: {}", stderr.trim()),
                                    ToastLevel::Error,
                                );
                            }
                        }

                        // Only restore branch and stash if no conflicts
                        // (conflicts need to be resolved on the current branch)
                        if !has_conflicts {
                            if prev != main_branch {
                                let _ = tokio::process::Command::new("git")
                                    .args(["checkout", &prev])
                                    .current_dir(&source_repo)
                                    .output()
                                    .await;
                            }
                            if src_dirty {
                                let _ = tokio::process::Command::new("git")
                                    .args(["stash", "pop"])
                                    .current_dir(&source_repo)
                                    .output()
                                    .await;
                            }
                        }
                    }
                    MergeStrategy::Rebase => {
                        // Rebase workspace branch onto main
                        let rebase = tokio::process::Command::new("git")
                            .args(["rebase", &main_branch])
                            .current_dir(&ws_path)
                            .output()
                            .await?;

                        if !rebase.status.success() {
                            // Check for conflict markers in git status
                            let conflict_check = tokio::process::Command::new("git")
                                .args(["status", "--porcelain=v1"])
                                .current_dir(&ws_path)
                                .output()
                                .await?;
                            let conflict_stdout =
                                String::from_utf8_lossy(&conflict_check.stdout);
                            let conflict_files: Vec<ConflictFile> =
                                piki_core::git::parse_porcelain_status(&conflict_stdout)
                                    .into_iter()
                                    .filter(|f| {
                                        matches!(f.status, piki_core::FileStatus::Conflicted)
                                    })
                                    .map(|f| ConflictFile {
                                        path: f.path.clone(),
                                        status: format!("{:?}", f.status),
                                    })
                                    .collect();
                            if !conflict_files.is_empty() {
                                app.active_dialog = Some(DialogState::ConflictResolution {
                                    files: conflict_files,
                                    selected: 0,
                                    repo_path: ws_path.clone(),
                                });
                                app.mode = AppMode::ConflictResolution;
                                app.set_toast(
                                    "Rebase conflicts detected — resolve below",
                                    ToastLevel::Error,
                                );
                            } else {
                                let stderr = String::from_utf8_lossy(&rebase.stderr);
                                let _ = tokio::process::Command::new("git")
                                    .args(["rebase", "--abort"])
                                    .current_dir(&ws_path)
                                    .output()
                                    .await;
                                app.set_toast(
                                    format!("Rebase failed: {}", stderr.trim()),
                                    ToastLevel::Error,
                                );
                            }
                            return Ok(());
                        }

                        // Now fast-forward merge in source repo
                        let prev_branch = tokio::process::Command::new("git")
                            .args(["rev-parse", "--abbrev-ref", "HEAD"])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        let prev = String::from_utf8_lossy(&prev_branch.stdout)
                            .trim()
                            .to_string();

                        let _ = tokio::process::Command::new("git")
                            .args(["checkout", &main_branch])
                            .current_dir(&source_repo)
                            .output()
                            .await;

                        let ff = tokio::process::Command::new("git")
                            .args(["merge", "--ff-only", &branch])
                            .current_dir(&source_repo)
                            .output()
                            .await?;

                        if ff.status.success() {
                            app.set_toast(
                                format!("✓ Rebased and merged '{}' into {}", branch, main_branch),
                                ToastLevel::Success,
                            );
                        } else {
                            let stderr = String::from_utf8_lossy(&ff.stderr);
                            app.set_toast(
                                format!("Fast-forward failed: {}", stderr.trim()),
                                ToastLevel::Error,
                            );
                        }

                        // Restore previous branch
                        if prev != main_branch {
                            let _ = tokio::process::Command::new("git")
                                .args(["checkout", &prev])
                                .current_dir(&source_repo)
                                .output()
                                .await;
                        }
                    }
                }

                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.dirty = true;
                    let _ = ws.refresh_changed_files().await;
                }
            }
        }
        Action::ViewConflictDiff(file) => {
            // Use repo_path from the conflict dialog (conflicts may be in source_repo, not worktree)
            let worktree = match &app.active_dialog {
                Some(DialogState::ConflictResolution { repo_path, .. }) => repo_path.clone(),
                _ => match app.current_workspace() {
                    Some(ws) => ws.path.clone(),
                    None => return Ok(()),
                },
            };
            let term_size = terminal.size()?;
            let overlay_inner_width = (term_size.width * 90 / 100).saturating_sub(2);
            let width = if overlay_inner_width > 10 {
                overlay_inner_width
            } else {
                120
            };
            // Try delta first, fall back to plain git diff
            let ansi_bytes =
                match super::run_git_diff_with_delta(
                    &worktree,
                    &["diff", "--color=always", "--", &file],
                    width,
                ).await {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        let output = tokio::process::Command::new("git")
                            .args(["diff", "--color=always", "--", &file])
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
                    app.diff_file_path = Some(format!("conflict: {}", file));
                    app.diff_scroll = 0;
                    app.mode = AppMode::Diff;
                    app.active_pane = ActivePane::MainPanel;
                }
                Err(e) => {
                    app.status_message = Some(format!("Failed to parse diff: {}", e));
                }
            }
        }
        Action::ResolveConflict { file, strategy } => {
            // Use repo_path from the conflict dialog (conflicts may be in source_repo)
            let worktree = match &app.active_dialog {
                Some(DialogState::ConflictResolution { repo_path, .. }) => repo_path.clone(),
                _ => match app.current_workspace() {
                    Some(ws) => ws.path.clone(),
                    None => return Ok(()),
                },
            };
            match strategy {
                ConflictStrategy::Ours => {
                    tokio::process::Command::new("git")
                        .args(["checkout", "--ours", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                    tokio::process::Command::new("git")
                        .args(["add", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                }
                ConflictStrategy::Theirs => {
                    tokio::process::Command::new("git")
                        .args(["checkout", "--theirs", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                    tokio::process::Command::new("git")
                        .args(["add", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                }
                ConflictStrategy::MarkResolved => {
                    tokio::process::Command::new("git")
                        .args(["add", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                }
            }
            app.set_toast(format!("Resolved: {}", file), ToastLevel::Success);
            // Remove from conflict list
            if let Some(DialogState::ConflictResolution {
                ref mut files,
                ref mut selected,
                ..
            }) = app.active_dialog
            {
                files.retain(|f| f.path != file);
                if files.is_empty() {
                    // Auto-commit to complete the merge
                    let merge_commit = tokio::process::Command::new("git")
                        .args(["commit", "--no-edit"])
                        .current_dir(&worktree)
                        .output()
                        .await;
                    let msg = match merge_commit {
                        Ok(out) if out.status.success() => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let first = stdout.lines().next().unwrap_or("Merge committed");
                            format!("✓ All conflicts resolved and merge committed: {}", first)
                        }
                        _ => "All conflicts resolved — commit to complete merge".to_string(),
                    };
                    app.active_dialog = None;
                    app.mode = AppMode::Normal;
                    app.diff_content = None;
                    app.diff_file_path = None;
                    app.set_toast(msg, ToastLevel::Success);
                } else if *selected >= files.len() {
                    *selected = files.len() - 1;
                }
            }
            // Refresh workspace
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                ws.dirty = true;
                ws.last_refresh = None;
            }
        }
        Action::AbortMerge => {
            let worktree = match &app.active_dialog {
                Some(DialogState::ConflictResolution { repo_path, .. }) => repo_path.clone(),
                _ => match app.current_workspace() {
                    Some(ws) => ws.path.clone(),
                    None => return Ok(()),
                },
            };
            // Try merge --abort first, fall back to rebase --abort
            let merge_result = tokio::process::Command::new("git")
                .args(["merge", "--abort"])
                .current_dir(&worktree)
                .output()
                .await?;
            if !merge_result.status.success() {
                tokio::process::Command::new("git")
                    .args(["rebase", "--abort"])
                    .current_dir(&worktree)
                    .output()
                    .await?;
            }
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            app.diff_content = None;
            app.diff_file_path = None;
            app.set_toast("Merge aborted", ToastLevel::Info);
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                ws.dirty = true;
                ws.last_refresh = None;
            }
        }
        Action::DetectConflicts => {
            let (ws_path, source_repo) = match app.current_workspace() {
                Some(ws) => (ws.path.clone(), ws.source_repo.clone()),
                None => return Ok(()),
            };
            // Check both worktree and source_repo for conflicts
            for repo_path in [&ws_path, &source_repo] {
                let output = tokio::process::Command::new("git")
                    .args(["status", "--porcelain=v1"])
                    .current_dir(repo_path)
                    .output()
                    .await?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let conflicts: Vec<ConflictFile> =
                    piki_core::git::parse_porcelain_status(&stdout)
                        .into_iter()
                        .filter(|f| matches!(f.status, piki_core::FileStatus::Conflicted))
                        .map(|f| ConflictFile {
                            path: f.path.clone(),
                            status: format!("{:?}", f.status),
                        })
                        .collect();
                if !conflicts.is_empty() {
                    app.open_conflict_resolution_with(conflicts, repo_path.clone());
                    return Ok(());
                }
            }
            app.set_toast("No conflicts detected", ToastLevel::Info);
        }
        other => unreachable!("non-merge action routed to action::git_merge: {other:?}"),
    }
    Ok(())
}

