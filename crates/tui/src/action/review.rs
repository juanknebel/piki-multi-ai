use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{App, AppMode, ToastLevel};
use crate::code_review::CodeReviewState;
use piki_core::workspace::WorkspaceManager;
use piki_core::AIProvider;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    _terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::LoadPrReview => {
            let worktree_path = app
                .workspaces
                .get(app.active_workspace)
                .map(|ws| ws.path.clone());
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
        Action::LoadPrFileDiff(file_idx) => {
            // Extract what we need before the async call
            let diff_data = app.workspaces.get_mut(app.active_workspace).and_then(|ws| {
                let cr = ws.code_review.as_mut()?;
                let file = cr.files.get(file_idx)?;
                let file_path = file.path.clone();
                if cr.file_diffs.contains_key(&file_path) {
                    return None; // Already cached
                }
                cr.loading = true;
                let base_ref = cr.pr_info.base_ref_name.clone();
                Some((ws.path.clone(), file_path, base_ref))
            });
            if let Some((worktree_path, file_path, base_ref)) = diff_data {
                match piki_core::github::get_pr_file_diff_raw(&worktree_path, &file_path, &base_ref)
                    .await
                {
                    Ok(parsed) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(ref mut cr) = ws.code_review
                        {
                            cr.file_diffs.insert(file_path, parsed);
                            cr.diff_scroll = 0;
                            cr.cursor_line = 0;
                            cr.loading = false;
                        }
                    }
                    Err(e) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(ref mut cr) = ws.code_review
                        {
                            cr.loading = false;
                        }
                        app.set_toast(format!("Diff error: {}", e), ToastLevel::Error);
                    }
                }
            }
        }
        Action::SubmitPrReview => {
            let submit_data = if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                if let Some(cr) = ws.code_review.as_mut() {
                    let verdict = cr.draft.verdict;
                    let body = cr.draft.body.clone();
                    let comments = cr.draft.comments.clone();
                    let pr_number = cr.pr_info.number;
                    cr.show_submit = false;
                    Some((ws.info.path.clone(), verdict, body, comments, pr_number))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((worktree_path, verdict, body, comments, pr_number)) = submit_data {
                let result = if comments.is_empty() {
                    piki_core::github::submit_review(&worktree_path, verdict, &body).await
                } else {
                    piki_core::github::submit_review_with_comments(
                        &worktree_path,
                        pr_number,
                        verdict,
                        &body,
                        &comments,
                    )
                    .await
                };
                match result {
                    Ok(_) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                            ws.code_review = None;
                            if ws
                                .current_tab()
                                .is_some_and(|t| t.provider == AIProvider::CodeReview)
                            {
                                ws.close_tab(ws.active_tab);
                            }
                        }
                        app.mode = AppMode::Normal;
                        app.interacting = false;
                        app.active_dialog = None;
                        app.set_toast(
                            format!("Review submitted: {}", verdict.label()),
                            ToastLevel::Success,
                        );
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        let user_msg = if msg.contains("Can not request changes on your own")
                            || msg.contains("Can not approve your own")
                        {
                            "Cannot approve/request-changes on your own PR — use Comment"
                                .to_string()
                        } else if msg.contains("Unprocessable Entity") {
                            format!("GitHub rejected: {}", msg)
                        } else {
                            format!("Submit failed: {}", msg)
                        };
                        // Show error inside the submit overlay so user can see it and retry
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(cr) = ws.code_review.as_mut()
                        {
                            cr.show_submit = true;
                            cr.submit_error = Some(user_msg);
                        }
                    }
                }
            }
        }
        other => unreachable!("non-review action routed to action::review: {other:?}"),
    }
    Ok(())
}
