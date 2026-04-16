use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::State;

use piki_core::github::{
    self, InlineComment, PrFile, PrInfo, ReviewVerdict,
};

use crate::state::DesktopApp;
use super::diff::SideBySideDiff;

#[derive(Serialize, Clone)]
pub struct PrDetail {
    pub info: PrInfo,
    pub files: Vec<PrFile>,
}

#[tauri::command]
pub async fn get_pr_info(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<Option<PrDetail>, String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let info = github::get_pr_for_branch(&ws_path)
        .await
        .map_err(|e| e.to_string())?;

    match info {
        Some(pr) => {
            let files = github::get_pr_files(&ws_path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Some(PrDetail { info: pr, files }))
        }
        None => Ok(None),
    }
}

#[derive(Serialize, Clone)]
pub struct PrFileDiff {
    pub path: String,
    pub lines: Vec<PrDiffLine>,
}

#[derive(Serialize, Clone)]
pub struct PrDiffLine {
    pub line_type: String,
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[tauri::command]
pub async fn get_pr_file_diff(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file: String,
    base_ref: String,
) -> Result<PrFileDiff, String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let parsed = github::get_pr_file_diff_raw(&ws_path, &file, &base_ref)
        .await
        .map_err(|e| e.to_string())?;

    let lines = parsed
        .lines
        .into_iter()
        .map(|l| PrDiffLine {
            line_type: format!("{:?}", l.line_type),
            content: l.content,
            old_line: l.old_line,
            new_line: l.new_line,
        })
        .collect();

    Ok(PrFileDiff { path: file, lines })
}

#[derive(Deserialize)]
pub struct SubmitReviewPayload {
    pub verdict: String,
    pub body: String,
    pub comments: Vec<InlineCommentPayload>,
}

#[derive(Deserialize)]
pub struct InlineCommentPayload {
    pub path: String,
    pub line: u32,
    pub side: String,
    pub body: String,
}

#[tauri::command]
pub async fn submit_pr_review(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    pr_number: u64,
    verdict: String,
    body: String,
    comments: Vec<InlineCommentPayload>,
) -> Result<String, String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let review_verdict = match verdict.as_str() {
        "approve" => ReviewVerdict::Approve,
        "request_changes" => ReviewVerdict::RequestChanges,
        _ => ReviewVerdict::Comment,
    };

    if comments.is_empty() {
        github::submit_review(&ws_path, review_verdict, &body)
            .await
            .map_err(|e| e.to_string())
    } else {
        let inline_comments: Vec<InlineComment> = comments
            .into_iter()
            .map(|c| InlineComment {
                path: c.path,
                line: c.line,
                side: c.side,
                body: c.body,
            })
            .collect();
        github::submit_review_with_comments(
            &ws_path,
            pr_number,
            review_verdict,
            &body,
            &inline_comments,
        )
        .await
        .map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn get_pr_file_side_by_side_diff(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file: String,
    base_ref: String,
) -> Result<SideBySideDiff, String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let diff_spec = format!("{base_ref}...HEAD");
    let output = piki_core::shell_env::command("git")
        .args(["diff", "--no-color", "-U3", &diff_spec, "--", &file])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git diff failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    Ok(super::diff::parse_side_by_side(
        &stdout,
        &base_ref,
        "HEAD",
        &file,
    ))
}

fn get_ws_path(
    state: &State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<std::path::PathBuf, String> {
    let app = state.lock();
    if workspace_idx >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }
    Ok(app.workspaces[workspace_idx].info.path.clone())
}
