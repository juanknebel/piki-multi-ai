use std::path::PathBuf;

use anyhow::{Context, bail};
use tokio::process::Command;

use crate::domain::WorkspaceInfo;

// No branch prefix — branch name matches the workspace name exactly.

/// Returns the base directory for worktrees:
/// `$HOME/.local/share/piki-multi/worktrees/<project_dir_name>`
fn worktrees_base(git_root: &PathBuf) -> PathBuf {
    let project_name = git_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".local/share/piki-multi/worktrees")
        .join(project_name)
}

/// Manages git worktree creation and removal.
/// Stateless — each operation receives the source directory.
pub struct WorkspaceManager;

impl WorkspaceManager {
    pub fn new() -> Self {
        Self
    }

    /// Detect the git root for a given directory
    pub async fn git_root(source_dir: &PathBuf) -> anyhow::Result<PathBuf> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(source_dir)
            .output()
            .await
            .context("failed to run git rev-parse")?;

        if !output.status.success() {
            bail!(
                "not a git repository: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(PathBuf::from(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    }

    /// Create a new workspace with its own git worktree and branch.
    /// `source_dir` is any directory inside the target git repository.
    pub async fn create(
        &self,
        name: &str,
        description: &str,
        prompt: &str,
        kanban_path: Option<String>,
        source_dir: &PathBuf,
    ) -> anyhow::Result<WorkspaceInfo> {
        let git_root = Self::git_root(source_dir).await?;
        let worktrees_dir = worktrees_base(&git_root);
        let worktree_path = worktrees_dir.join(name);
        let branch_name = name.to_string();

        if worktree_path.exists() {
            bail!("workspace '{}' already exists", name);
        }

        // Ensure the worktrees directory exists
        tokio::fs::create_dir_all(&worktrees_dir)
            .await
            .context("failed to create worktrees directory")?;

        // Check if branch exists on remote
        let ls_remote = Command::new("git")
            .args(["ls-remote", "--heads", "origin", &branch_name])
            .current_dir(&git_root)
            .output()
            .await
            .context("failed to check remote branches")?;

        let remote_exists = ls_remote.status.success() && !ls_remote.stdout.is_empty();

        let output = if remote_exists {
            // Fetch the remote branch first
            let fetch = Command::new("git")
                .args(["fetch", "origin", &branch_name])
                .current_dir(&git_root)
                .output()
                .await
                .context("failed to fetch remote branch")?;

            if !fetch.status.success() {
                bail!(
                    "git fetch failed: {}",
                    String::from_utf8_lossy(&fetch.stderr).trim()
                );
            }

            // Create worktree from remote branch (auto-creates local tracking branch)
            Command::new("git")
                .args([
                    "worktree",
                    "add",
                    worktree_path.to_str().unwrap(),
                    &branch_name,
                ])
                .current_dir(&git_root)
                .output()
                .await
                .context("failed to create worktree")?
        } else {
            // Create worktree with new branch
            Command::new("git")
                .args([
                    "worktree",
                    "add",
                    worktree_path.to_str().unwrap(),
                    "-b",
                    &branch_name,
                ])
                .current_dir(&git_root)
                .output()
                .await
                .context("failed to create worktree")?
        };

        if !output.status.success() {
            bail!(
                "git worktree add failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(WorkspaceInfo::new(
            name.to_string(),
            description.to_string(),
            prompt.to_string(),
            kanban_path,
            branch_name,
            worktree_path,
            git_root,
        ))
    }

    /// Detect the main branch name (main, master, etc.) for a repository.
    pub async fn detect_main_branch(source_repo: &PathBuf) -> String {
        // Try symbolic-ref of origin/HEAD
        if let Ok(output) = Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
            .current_dir(source_repo)
            .output()
            .await
        {
            if output.status.success() {
                let refname = String::from_utf8_lossy(&output.stdout).trim().to_string();
                // refs/remotes/origin/main -> main
                if let Some(branch) = refname.strip_prefix("refs/remotes/origin/") {
                    return branch.to_string();
                }
            }
        }
        // Fallback: check if "main" branch exists, otherwise "master"
        if let Ok(output) = Command::new("git")
            .args(["rev-parse", "--verify", "refs/heads/main"])
            .current_dir(source_repo)
            .output()
            .await
        {
            if output.status.success() {
                return "main".to_string();
            }
        }
        "master".to_string()
    }

    /// Remove a workspace: remove worktree, delete branch.
    /// Uses the workspace's `source_repo` to locate the git root.
    pub async fn remove(&self, name: &str, source_repo: &PathBuf) -> anyhow::Result<()> {
        let worktree_path = worktrees_base(source_repo).join(name);
        let branch_name = name.to_string();

        // git worktree remove --force <path>
        let output = Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                worktree_path.to_str().unwrap(),
            ])
            .current_dir(source_repo)
            .output()
            .await
            .context("failed to remove worktree")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("is not a working tree") {
                bail!("git worktree remove failed: {}", stderr.trim());
            }
        }

        // git branch -D <branch>
        let _ = Command::new("git")
            .args(["branch", "-D", &branch_name])
            .current_dir(source_repo)
            .output()
            .await;

        Ok(())
    }
}
