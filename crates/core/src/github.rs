use std::path::Path;

use serde::{Deserialize, Serialize};

/// PR metadata from `gh pr view --json ...`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrInfo {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub state: String,
    #[serde(default)]
    pub review_decision: Option<String>,
    pub url: String,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub additions: u64,
    pub deletions: u64,
}

/// A file changed in the PR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrFile {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct PrFilesResponse {
    files: Vec<PrFile>,
}

/// Review verdict for `gh pr review`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Approve,
    RequestChanges,
    Comment,
}

impl ReviewVerdict {
    pub fn label(&self) -> &'static str {
        match self {
            ReviewVerdict::Approve => "Approve",
            ReviewVerdict::RequestChanges => "Request Changes",
            ReviewVerdict::Comment => "Comment",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            ReviewVerdict::Approve => ReviewVerdict::RequestChanges,
            ReviewVerdict::RequestChanges => ReviewVerdict::Comment,
            ReviewVerdict::Comment => ReviewVerdict::Approve,
        }
    }

    fn flag(&self) -> &'static str {
        match self {
            ReviewVerdict::Approve => "--approve",
            ReviewVerdict::RequestChanges => "--request-changes",
            ReviewVerdict::Comment => "--comment",
        }
    }

    pub fn api_event(&self) -> &'static str {
        match self {
            ReviewVerdict::Approve => "APPROVE",
            ReviewVerdict::RequestChanges => "REQUEST_CHANGES",
            ReviewVerdict::Comment => "COMMENT",
        }
    }
}

// ── Diff parsing types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLineType {
    FileHeader,
    HunkHeader,
    Context,
    Addition,
    Deletion,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ParsedDiff {
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InlineComment {
    pub path: String,
    pub line: u32,
    pub side: String,
    pub body: String,
}

/// Parse a unified diff (output of `git diff --no-color`) into structured lines.
pub fn parse_unified_diff(raw: &str) -> ParsedDiff {
    let mut lines = Vec::new();
    let mut old_counter: u32 = 0;
    let mut new_counter: u32 = 0;

    for raw_line in raw.lines() {
        if raw_line.starts_with("diff --git")
            || raw_line.starts_with("index ")
            || raw_line.starts_with("--- ")
            || raw_line.starts_with("+++ ")
        {
            lines.push(DiffLine {
                line_type: DiffLineType::FileHeader,
                content: raw_line.to_string(),
                old_line: None,
                new_line: None,
            });
        } else if raw_line.starts_with("@@") {
            // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
            if let Some(rest) = raw_line.strip_prefix("@@ ") {
                let parts: Vec<&str> = rest.splitn(3, ' ').collect();
                if parts.len() >= 2 {
                    // Parse -N,M -> old_start
                    if let Some(old_spec) = parts[0].strip_prefix('-') {
                        let old_start_str = old_spec.split(',').next().unwrap_or("1");
                        old_counter = old_start_str.parse().unwrap_or(1);
                    }
                    // Parse +N,M -> new_start
                    if let Some(new_spec) = parts[1].strip_prefix('+') {
                        // new_spec might end with "@@" or "@@..." — strip the @@ suffix
                        let new_spec_clean = new_spec.split("@@").next().unwrap_or(new_spec);
                        let new_start_str = new_spec_clean.split(',').next().unwrap_or("1");
                        new_counter = new_start_str.parse().unwrap_or(1);
                    }
                }
            }
            lines.push(DiffLine {
                line_type: DiffLineType::HunkHeader,
                content: raw_line.to_string(),
                old_line: None,
                new_line: None,
            });
        } else if let Some(rest) = raw_line.strip_prefix('+') {
            lines.push(DiffLine {
                line_type: DiffLineType::Addition,
                content: rest.to_string(),
                old_line: None,
                new_line: Some(new_counter),
            });
            new_counter += 1;
        } else if let Some(rest) = raw_line.strip_prefix('-') {
            lines.push(DiffLine {
                line_type: DiffLineType::Deletion,
                content: rest.to_string(),
                old_line: Some(old_counter),
                new_line: None,
            });
            old_counter += 1;
        } else if raw_line.starts_with('\\') {
            // "\ No newline at end of file" — informational
            lines.push(DiffLine {
                line_type: DiffLineType::Context,
                content: raw_line.to_string(),
                old_line: None,
                new_line: None,
            });
        } else {
            // Context line (starts with ' ' or is empty)
            let content = raw_line.strip_prefix(' ').unwrap_or(raw_line);
            lines.push(DiffLine {
                line_type: DiffLineType::Context,
                content: content.to_string(),
                old_line: Some(old_counter),
                new_line: Some(new_counter),
            });
            old_counter += 1;
            new_counter += 1;
        }
    }

    ParsedDiff { lines }
}

// ── Async commands ──────────────────────────────────────────────────────────

/// Fetch PR info for the current branch. Returns `None` if no open PR exists.
pub async fn get_pr_for_branch(worktree_path: &Path) -> anyhow::Result<Option<PrInfo>> {
    tracing::info!(path = %worktree_path.display(), "gh: fetching PR info for branch");
    let output = crate::shell_env::command("gh")
        .args([
            "pr",
            "view",
            "--json",
            "number,title,body,state,reviewDecision,url,headRefName,baseRefName,additions,deletions",
        ])
        .current_dir(worktree_path)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no pull requests found") || stderr.contains("no open pull requests") {
            tracing::info!("gh: no open PR found for current branch");
            return Ok(None);
        }
        tracing::error!(stderr = %stderr.trim(), "gh pr view failed");
        anyhow::bail!("gh pr view failed: {}", stderr.trim());
    }

    let info: PrInfo = serde_json::from_slice(&output.stdout)?;
    tracing::info!(pr = info.number, title = %info.title, state = %info.state, "gh: PR info loaded");
    Ok(Some(info))
}

/// Fetch the list of changed files in the PR.
pub async fn get_pr_files(worktree_path: &Path) -> anyhow::Result<Vec<PrFile>> {
    tracing::info!(path = %worktree_path.display(), "gh: fetching PR files");
    let output = crate::shell_env::command("gh")
        .args(["pr", "view", "--json", "files"])
        .current_dir(worktree_path)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!(stderr = %stderr.trim(), "gh pr view --json files failed");
        anyhow::bail!("gh pr view --json files failed: {}", stderr.trim());
    }

    let resp: PrFilesResponse = serde_json::from_slice(&output.stdout)?;
    tracing::info!(count = resp.files.len(), "gh: PR files loaded");
    Ok(resp.files)
}

/// Fetch a parsed diff for a specific file in the PR.
/// Runs `git diff --no-color base...HEAD -- file` and parses the unified output.
pub async fn get_pr_file_diff_raw(
    worktree_path: &Path,
    file: &str,
    base_ref: &str,
) -> anyhow::Result<ParsedDiff> {
    let diff_spec = format!("{base_ref}...HEAD");
    tracing::debug!(file, diff_spec = %diff_spec, "gh: fetching raw diff for file");
    let output = crate::shell_env::command("git")
        .args(["diff", "--no-color", &diff_spec, "--", file])
        .current_dir(worktree_path)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!(file, stderr = %stderr.trim(), "git diff failed");
        anyhow::bail!("git diff failed: {}", stderr.trim());
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let parsed = parse_unified_diff(&raw);
    tracing::debug!(file, lines = parsed.lines.len(), "gh: diff parsed");
    Ok(parsed)
}

/// Get the repository owner/name (e.g. "owner/repo") via `gh`.
pub async fn get_repo_nwo(worktree_path: &Path) -> anyhow::Result<String> {
    tracing::debug!(path = %worktree_path.display(), "gh: fetching repo nwo");
    let output = crate::shell_env::command("gh")
        .args([
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ])
        .current_dir(worktree_path)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!(stderr = %stderr.trim(), "gh repo view failed");
        anyhow::bail!("gh repo view failed: {}", stderr.trim());
    }

    let nwo = String::from_utf8_lossy(&output.stdout).trim().to_string();
    tracing::debug!(nwo = %nwo, "gh: repo nwo resolved");
    Ok(nwo)
}

/// Submit a review with inline comments via `gh api`.
pub async fn submit_review_with_comments(
    worktree_path: &Path,
    pr_number: u64,
    verdict: ReviewVerdict,
    body: &str,
    comments: &[InlineComment],
) -> anyhow::Result<String> {
    tracing::info!(
        pr = pr_number,
        verdict = verdict.api_event(),
        comment_count = comments.len(),
        body_len = body.len(),
        "gh: submitting review with inline comments"
    );
    let nwo = get_repo_nwo(worktree_path).await?;

    // Build JSON payload — only include "side" for deletion lines
    let comments_json: Vec<serde_json::Value> = comments
        .iter()
        .map(|c| {
            let mut obj = serde_json::json!({
                "path": c.path,
                "line": c.line,
                "body": c.body,
            });
            if c.side == "LEFT" {
                obj["side"] = serde_json::json!("LEFT");
            }
            obj
        })
        .collect();

    let mut payload = serde_json::json!({
        "event": verdict.api_event(),
        "comments": comments_json,
    });
    // Only include body if non-empty (GitHub API rejects empty body for some events)
    if !body.is_empty() {
        payload["body"] = serde_json::json!(body);
    }

    let payload_str = serde_json::to_string_pretty(&payload)?;
    let endpoint = format!("repos/{nwo}/pulls/{pr_number}/reviews");

    tracing::info!(endpoint = %endpoint, "gh: POST review");
    tracing::debug!(payload = %payload_str, "gh: review payload");

    // Spawn the process with piped stdin, write synchronously via spawn_blocking
    let worktree_owned = worktree_path.to_path_buf();
    let output = tokio::task::spawn_blocking(move || -> anyhow::Result<std::process::Output> {
        let mut child = crate::shell_env::sync_command("gh")
            .args(["api", &endpoint, "--method", "POST", "--input", "-"])
            .current_dir(&worktree_owned)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(payload_str.as_bytes())?;
            // Drop closes stdin
        }

        Ok(child.wait_with_output()?)
    })
    .await??;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        tracing::error!(
            stderr = %stderr.trim(),
            stdout = %stdout.trim(),
            "gh api review submit failed"
        );
        anyhow::bail!("gh api review submit failed: {}", stderr.trim());
    }

    tracing::info!("gh: review submitted successfully");
    tracing::debug!(response = %stdout.trim(), "gh: review response");
    Ok(stdout.trim().to_string())
}

/// Submit a review on the current PR (no inline comments).
pub async fn submit_review(
    worktree_path: &Path,
    verdict: ReviewVerdict,
    body: &str,
) -> anyhow::Result<String> {
    tracing::info!(
        verdict = verdict.label(),
        body_len = body.len(),
        "gh: submitting review (no inline comments)"
    );
    let mut args = vec!["pr", "review", verdict.flag()];
    if !body.is_empty() {
        args.push("--body");
        args.push(body);
    }

    tracing::debug!(args = ?args, "gh: running gh pr review");
    let output = crate::shell_env::command("gh")
        .args(&args)
        .current_dir(worktree_path)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!(stderr = %stderr.trim(), "gh pr review failed");
        anyhow::bail!("gh pr review failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    tracing::info!("gh: review submitted successfully (no inline comments)");
    Ok(stdout.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unified_diff_basic() {
        let raw = "\
diff --git a/src/main.rs b/src/main.rs
index abc1234..def5678 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,6 +10,7 @@ fn main() {
     let x = 1;
     let y = 2;
+    let z = 3;
     println!(\"hello\");
     println!(\"world\");
 }";
        let parsed = parse_unified_diff(raw);

        // 4 file headers + 1 hunk header + 6 content lines = 11
        assert_eq!(parsed.lines.len(), 11);

        // File headers
        assert_eq!(parsed.lines[0].line_type, DiffLineType::FileHeader);
        assert_eq!(parsed.lines[1].line_type, DiffLineType::FileHeader);
        assert_eq!(parsed.lines[2].line_type, DiffLineType::FileHeader);
        assert_eq!(parsed.lines[3].line_type, DiffLineType::FileHeader);

        // Hunk header
        assert_eq!(parsed.lines[4].line_type, DiffLineType::HunkHeader);

        // Context: "    let x = 1;" — old=10, new=10
        assert_eq!(parsed.lines[5].line_type, DiffLineType::Context);
        assert_eq!(parsed.lines[5].old_line, Some(10));
        assert_eq!(parsed.lines[5].new_line, Some(10));
        assert_eq!(parsed.lines[5].content, "    let x = 1;");

        // Context: "    let y = 2;" — old=11, new=11
        assert_eq!(parsed.lines[6].line_type, DiffLineType::Context);
        assert_eq!(parsed.lines[6].old_line, Some(11));
        assert_eq!(parsed.lines[6].new_line, Some(11));

        // Addition: "    let z = 3;" — new=12
        assert_eq!(parsed.lines[7].line_type, DiffLineType::Addition);
        assert_eq!(parsed.lines[7].old_line, None);
        assert_eq!(parsed.lines[7].new_line, Some(12));
        assert_eq!(parsed.lines[7].content, "    let z = 3;");

        // Context: "    println!(\"hello\");" — old=12, new=13
        assert_eq!(parsed.lines[8].line_type, DiffLineType::Context);
        assert_eq!(parsed.lines[8].old_line, Some(12));
        assert_eq!(parsed.lines[8].new_line, Some(13));

        // Context: "}" — old=14, new=15
        assert_eq!(parsed.lines[10].line_type, DiffLineType::Context);
        assert_eq!(parsed.lines[10].content, "}");
    }

    #[test]
    fn test_parse_unified_diff_deletion() {
        let raw = "\
@@ -1,3 +1,2 @@
 line1
-removed
 line2";
        let parsed = parse_unified_diff(raw);
        assert_eq!(parsed.lines.len(), 4); // hunk + 3 content

        // Context: old=1, new=1
        assert_eq!(parsed.lines[1].line_type, DiffLineType::Context);
        assert_eq!(parsed.lines[1].old_line, Some(1));
        assert_eq!(parsed.lines[1].new_line, Some(1));

        // Deletion: old=2
        assert_eq!(parsed.lines[2].line_type, DiffLineType::Deletion);
        assert_eq!(parsed.lines[2].old_line, Some(2));
        assert_eq!(parsed.lines[2].new_line, None);
        assert_eq!(parsed.lines[2].content, "removed");

        // Context: old=3, new=2
        assert_eq!(parsed.lines[3].line_type, DiffLineType::Context);
        assert_eq!(parsed.lines[3].old_line, Some(3));
        assert_eq!(parsed.lines[3].new_line, Some(2));
    }

    #[test]
    fn test_parse_unified_diff_no_newline_marker() {
        let raw = "\
@@ -1,2 +1,2 @@
-old
+new
\\ No newline at end of file";
        let parsed = parse_unified_diff(raw);
        assert_eq!(parsed.lines.len(), 4);
        assert_eq!(parsed.lines[3].line_type, DiffLineType::Context);
        assert_eq!(parsed.lines[3].old_line, None);
        assert_eq!(parsed.lines[3].new_line, None);
    }

    #[test]
    fn test_review_verdict_api_event() {
        assert_eq!(ReviewVerdict::Approve.api_event(), "APPROVE");
        assert_eq!(ReviewVerdict::RequestChanges.api_event(), "REQUEST_CHANGES");
        assert_eq!(ReviewVerdict::Comment.api_event(), "COMMENT");
    }
}
