use std::path::Path;
use std::process::Stdio;

use crate::domain::FileStatus;

/// Run a diff for the given file, piped through `delta --side-by-side`.
/// Returns raw ANSI bytes suitable for conversion with ansi-to-tui.
/// Falls back to plain git diff if delta is not installed.
/// For untracked files, uses `git diff --no-index /dev/null <file>`.
pub async fn run_diff(
    worktree_path: &Path,
    file_path: &str,
    width: u16,
    file_status: &FileStatus,
) -> anyhow::Result<Vec<u8>> {
    // Try with delta first
    match run_diff_with_delta(worktree_path, file_path, width, file_status).await {
        Ok(bytes) => Ok(bytes),
        Err(_) => {
            tracing::warn!(
                file = file_path,
                "delta not available, falling back to plain git diff"
            );
            // Fallback: plain git diff with color
            let output = if *file_status == FileStatus::Untracked {
                tokio::process::Command::new("git")
                    .args([
                        "diff",
                        "--color=always",
                        "--no-index",
                        "/dev/null",
                        file_path,
                    ])
                    .current_dir(worktree_path)
                    .output()
                    .await?
            } else {
                tokio::process::Command::new("git")
                    .args(["diff", "--color=always", "HEAD", "--", file_path])
                    .current_dir(worktree_path)
                    .output()
                    .await?
            };
            Ok(output.stdout)
        }
    }
}

async fn run_diff_with_delta(
    worktree_path: &Path,
    file_path: &str,
    width: u16,
    file_status: &FileStatus,
) -> anyhow::Result<Vec<u8>> {
    let mut cmd = tokio::process::Command::new("git");
    if *file_status == FileStatus::Untracked {
        cmd.args([
            "diff",
            "--color=always",
            "--no-index",
            "/dev/null",
            file_path,
        ]);
    } else {
        cmd.args(["diff", "--color=always", "HEAD", "--", file_path]);
    }
    let git_diff = cmd
        .current_dir(worktree_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let git_stdout = git_diff
        .stdout
        .ok_or_else(|| anyhow::anyhow!("failed to capture git diff stdout"))?;

    // Convert tokio ChildStdout to std Stdio for piping
    let git_stdout_std: Stdio = git_stdout.try_into()?;

    let delta_output = tokio::process::Command::new("delta")
        .args([
            "--side-by-side",
            &format!("--width={}", width),
            "--paging=never",
            "--true-color=always",
            "--line-fill-method=ansi",
        ])
        .stdin(git_stdout_std)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;

    if !delta_output.status.success() {
        anyhow::bail!("delta exited with non-zero status");
    }

    Ok(delta_output.stdout)
}
