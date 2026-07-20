use std::sync::Arc;

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{App, AppMode, ToastLevel};
use crate::dialog_state::DialogState;
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
    _terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::LoadPrReview => {
            // Reload the active workspace's already-open review: re-checks
            // the PR's remote head and only re-fetches if it moved (or the
            // local checkout is dirty) — a no-op `Reuse` otherwise.
            let reload_target = app.workspaces.get(app.active_workspace).and_then(|ws| {
                let cr = ws.code_review.as_ref()?;
                Some((cr.repo_nwo.clone(), cr.pr_info.number))
            });
            let Some((repo_nwo, number)) = reload_target else {
                return Ok(());
            };
            let checkout_mgr = manager.review_checkout_manager();
            match checkout_mgr.ensure_pr_checkout(&repo_nwo, number).await {
                Ok(checkout) => match piki_core::github::get_pr_files_by_number(&checkout.path, number).await {
                    Ok(files) => {
                        let existing_comments =
                            piki_core::github::get_pr_review_comments(&checkout.path, number)
                                .await
                                .unwrap_or_default();
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(cr) = ws.code_review.as_mut()
                        {
                            cr.pr_info = checkout.pr;
                            cr.files = files;
                            cr.existing_comments = existing_comments;
                            cr.file_diffs.clear();
                            cr.diff_scroll = 0;
                            cr.cursor_line = 0;
                        }
                        let msg = if checkout.reused { "PR up to date" } else { "PR refreshed" };
                        app.set_toast(msg, ToastLevel::Success);
                    }
                    Err(e) => {
                        app.set_toast(format!("Failed to load PR files: {}", e), ToastLevel::Error);
                    }
                },
                Err(e) => {
                    app.set_toast(format!("gh error: {}", e), ToastLevel::Error);
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
                let base_ref = cr.base_spec.clone();
                Some((cr.review_dir.clone(), file_path, base_ref))
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
                    let reply_drafts: Vec<(u64, String)> =
                        cr.reply_drafts.iter().map(|(id, body)| (*id, body.clone())).collect();
                    cr.show_submit = false;
                    Some((cr.review_dir.clone(), verdict, body, comments, pr_number, reply_drafts))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((review_dir, verdict, body, comments, pr_number, reply_drafts)) = submit_data {
                let result = if comments.is_empty() {
                    piki_core::github::submit_review_by_number(&review_dir, pr_number, verdict, &body).await
                } else {
                    piki_core::github::submit_review_with_comments(
                        &review_dir,
                        pr_number,
                        verdict,
                        &body,
                        &comments,
                    )
                    .await
                };
                match result {
                    Ok(_) => {
                        // Replies to existing threads go through a separate
                        // endpoint (`/comments/{id}/replies`) — send them
                        // one by one after the review itself lands, same
                        // order as the Desktop frontend.
                        let mut failed_replies = 0usize;
                        for (comment_id, reply_body) in &reply_drafts {
                            if piki_core::github::submit_comment_reply(
                                &review_dir,
                                pr_number,
                                *comment_id,
                                reply_body,
                            )
                            .await
                            .is_err()
                            {
                                failed_replies += 1;
                            }
                        }
                        // Submitting only closes the *view*, same as `q` —
                        // the workspace, its tab, and the checkout on disk
                        // are left alone. Deleting the review workspace (and
                        // always deleting its checkout) only ever happens
                        // through the explicit delete-workspace flow.
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(cr) = ws.code_review.as_mut()
                        {
                            // Clear the just-sent draft so reopening the
                            // review can't accidentally resubmit the same
                            // comments; the diffs and existing threads
                            // (including what we just posted, once
                            // refreshed) stay for reference.
                            cr.draft.comments.clear();
                            cr.draft.body.clear();
                            cr.draft.body_cursor = 0;
                            cr.reply_drafts.clear();
                        }
                        app.active_pane = crate::app::ActivePane::WorkspaceList;
                        app.mode = AppMode::Normal;
                        app.active_dialog = None;
                        let msg = if failed_replies > 0 {
                            format!(
                                "Review submitted: {} ({failed_replies} repl{} failed to send)",
                                verdict.label(),
                                if failed_replies == 1 { "y" } else { "ies" },
                            )
                        } else {
                            format!("Review submitted: {}", verdict.label())
                        };
                        let level = if failed_replies > 0 {
                            ToastLevel::Error
                        } else {
                            ToastLevel::Success
                        };
                        app.set_toast(msg, level);
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
        Action::LoadPrList => {
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
            }
            if app.gh_available != Some(true) {
                if let Some(DialogState::PrPicker { loading, error, .. }) = &mut app.active_dialog {
                    *loading = false;
                    *error = Some(
                        "gh CLI not found or not authenticated — run `gh auth login`".to_string(),
                    );
                }
                return Ok(());
            }
            let slot = Arc::clone(&app.pending_pr_list);
            tokio::spawn(async move {
                let result = piki_core::github::list_relevant_prs(50)
                    .await
                    .map_err(|e| e.to_string());
                *slot.lock() = Some(result);
            });
        }
        Action::OpenPrReview(item_idx) => {
            let item = match &app.active_dialog {
                Some(DialogState::PrPicker { items, .. }) => items.get(item_idx).cloned(),
                _ => None,
            };
            let Some(item) = item else {
                return Ok(());
            };
            let checkout_mgr = manager.review_checkout_manager();
            let slot = Arc::clone(&app.pending_pr_checkout);
            tokio::spawn(async move {
                let result = async {
                    let checkout = checkout_mgr
                        .ensure_pr_checkout(&item.repo_nwo, item.number)
                        .await?;
                    let files =
                        piki_core::github::get_pr_files_by_number(&checkout.path, item.number)
                            .await?;
                    let existing_comments =
                        piki_core::github::get_pr_review_comments(&checkout.path, item.number)
                            .await
                            .unwrap_or_default();
                    anyhow::Ok(crate::code_review::ReviewSessionData {
                        checkout,
                        files,
                        existing_comments,
                    })
                }
                .await
                .map_err(|e| e.to_string());
                *slot.lock() = Some(result);
            });
        }
        other => unreachable!("non-review action routed to action::review: {other:?}"),
    }
    Ok(())
}
