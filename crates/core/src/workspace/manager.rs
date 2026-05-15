use std::path::PathBuf;

use anyhow::{Context, bail};
use crate::shell_env;

use crate::domain::{WorkspaceInfo, WorkspaceOrigin, WorkspaceType};
use crate::paths::DataPaths;

// No branch prefix — branch name matches the workspace name exactly.

/// Manages git worktree creation and removal.
pub struct WorkspaceManager {
    paths: DataPaths,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            paths: DataPaths::default_paths(),
        }
    }

    pub fn with_paths(paths: DataPaths) -> Self {
        Self { paths }
    }

    /// Returns the base directory for worktrees:
    /// `<data_dir>/worktrees/<project_dir_name>`
    fn worktrees_base(&self, git_root: &std::path::Path) -> PathBuf {
        let project_name = git_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        self.paths.worktrees_dir(&project_name)
    }

    /// Detect the git root for a given directory
    pub async fn git_root(source_dir: &PathBuf) -> anyhow::Result<PathBuf> {
        let output = shell_env::command("git")
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
        let worktrees_dir = self.worktrees_base(&git_root);
        let worktree_path = worktrees_dir.join(name);
        let branch_name = name.to_string();

        if worktree_path.exists() {
            bail!("workspace '{}' already exists", name);
        }

        // Ensure the worktrees directory exists
        tokio::fs::create_dir_all(&worktrees_dir)
            .await
            .context("failed to create worktrees directory")?;

        // Check if branch exists locally
        let local_check = shell_env::command("git")
            .args(["rev-parse", "--verify", &format!("refs/heads/{}", branch_name)])
            .current_dir(&git_root)
            .output()
            .await
            .context("failed to check local branches")?;

        let local_exists = local_check.status.success();

        // Check if branch exists on remote
        let ls_remote = shell_env::command("git")
            .args(["ls-remote", "--heads", "origin", &branch_name])
            .current_dir(&git_root)
            .output()
            .await
            .context("failed to check remote branches")?;

        let remote_exists = ls_remote.status.success() && !ls_remote.stdout.is_empty();

        let output = if remote_exists {
            // Fetch the remote branch and set up tracking
            let fetch = shell_env::command("git")
                .args(["fetch", "origin", &format!("{}:{}", branch_name, branch_name)])
                .current_dir(&git_root)
                .output()
                .await
                .context("failed to fetch remote branch")?;

            if !fetch.status.success() {
                // If the local branch already exists and diverged, fall back to simple fetch
                let fetch_simple = shell_env::command("git")
                    .args(["fetch", "origin", &branch_name])
                    .current_dir(&git_root)
                    .output()
                    .await
                    .context("failed to fetch remote branch")?;

                if !fetch_simple.status.success() {
                    bail!(
                        "git fetch failed: {}",
                        String::from_utf8_lossy(&fetch_simple.stderr).trim()
                    );
                }
            }

            // Create worktree from the branch
            let worktree_output = shell_env::command("git")
                .args([
                    "worktree",
                    "add",
                    worktree_path.to_str().unwrap(),
                    &branch_name,
                ])
                .current_dir(&git_root)
                .output()
                .await
                .context("failed to create worktree")?;

            // Set upstream tracking
            let _ = shell_env::command("git")
                .args([
                    "branch",
                    &format!("--set-upstream-to=origin/{}", branch_name),
                    &branch_name,
                ])
                .current_dir(&git_root)
                .output()
                .await;

            worktree_output
        } else if local_exists {
            // Branch exists locally but not on remote — reuse it
            shell_env::command("git")
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
            // Branch doesn't exist anywhere — create new
            shell_env::command("git")
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

        tracing::info!(workspace = name, branch = %branch_name, path = %worktree_path.display(), "workspace created");

        let origin = detect_origin_from_repo(&git_root).await;
        let mut info = WorkspaceInfo::new(
            name.to_string(),
            description.to_string(),
            prompt.to_string(),
            kanban_path,
            branch_name,
            worktree_path,
            git_root,
        );
        info.origin = origin;
        Ok(info)
    }

    /// Create a simple workspace pointing to an existing directory.
    /// The directory is **not** required to be inside a git repository; when
    /// no git root can be detected, `source_repo` falls back to `dir` itself,
    /// the branch is set to "main" as a placeholder, and `origin` is `Local`.
    /// When the directory is inside a git repo with a github.com remote, the
    /// resulting workspace is tagged `origin = GitHub { url }`.
    pub async fn create_simple(
        &self,
        name: &str,
        description: &str,
        prompt: &str,
        kanban_path: Option<String>,
        dir: &PathBuf,
    ) -> anyhow::Result<WorkspaceInfo> {
        let (source_repo, branch, origin) = match Self::git_root(dir).await {
            Ok(root) => {
                let branch_output = shell_env::command("git")
                    .args(["rev-parse", "--abbrev-ref", "HEAD"])
                    .current_dir(dir)
                    .output()
                    .await
                    .context("failed to detect branch")?;
                let branch = if branch_output.status.success() {
                    String::from_utf8_lossy(&branch_output.stdout)
                        .trim()
                        .to_string()
                } else {
                    "main".to_string()
                };
                let origin = detect_origin_from_repo(&root).await;
                (root, branch, origin)
            }
            Err(_) => (dir.clone(), "main".to_string(), WorkspaceOrigin::Local),
        };

        let mut info = WorkspaceInfo::new(
            name.to_string(),
            description.to_string(),
            prompt.to_string(),
            kanban_path,
            branch,
            dir.clone(),
            source_repo,
        );
        info.workspace_type = WorkspaceType::Simple;
        info.origin = origin;
        tracing::info!(workspace = name, path = %dir.display(), "simple workspace created");
        Ok(info)
    }

    /// Clone a GitHub repository into a managed destination under the data
    /// directory and register it as a Simple workspace tagged with
    /// `WorkspaceOrigin::GitHub`. The destination is
    /// `<data_dir>/worktrees/<repo_name>/`, where `repo_name` is parsed from
    /// the URL.
    pub async fn create_from_github(
        &self,
        name: &str,
        description: &str,
        prompt: &str,
        kanban_path: Option<String>,
        github_url: &str,
    ) -> anyhow::Result<WorkspaceInfo> {
        let repo_name = parse_github_repo_name(github_url)
            .ok_or_else(|| anyhow::anyhow!("invalid GitHub URL: {}", github_url))?;
        let destination = self.paths.worktrees_dir(&repo_name);

        if destination.exists() {
            bail!(
                "destination '{}' already exists; refusing to overwrite",
                destination.display()
            );
        }
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("failed to create worktrees parent directory")?;
        }

        let output = shell_env::command("git")
            .args(["clone", github_url, destination.to_str().unwrap()])
            .output()
            .await
            .context("failed to run git clone")?;
        if !output.status.success() {
            bail!(
                "git clone failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let branch = Self::detect_main_branch(&destination).await;

        let mut info = WorkspaceInfo::new(
            name.to_string(),
            description.to_string(),
            prompt.to_string(),
            kanban_path,
            branch,
            destination.clone(),
            destination,
        );
        info.workspace_type = WorkspaceType::Simple;
        info.origin = WorkspaceOrigin::GitHub {
            url: github_url.to_string(),
        };
        tracing::info!(
            workspace = name,
            url = %github_url,
            path = %info.path.display(),
            "github workspace cloned"
        );
        Ok(info)
    }

    /// Create a project workspace pointing to a directory with sub-services.
    /// No git operations — the directory doesn't need to be a git repo.
    pub async fn create_project(
        &self,
        name: &str,
        description: &str,
        prompt: &str,
        kanban_path: Option<String>,
        dir: &std::path::Path,
    ) -> anyhow::Result<WorkspaceInfo> {
        let mut info = WorkspaceInfo::new(
            name.to_string(),
            description.to_string(),
            prompt.to_string(),
            kanban_path,
            String::new(),
            dir.to_path_buf(),
            dir.to_path_buf(),
        );
        info.workspace_type = WorkspaceType::Project;
        tracing::info!(workspace = name, path = %dir.display(), "project workspace created");
        Ok(info)
    }

    /// Detect the main branch name (main, master, etc.) for a repository.
    pub async fn detect_main_branch(source_repo: &PathBuf) -> String {
        // Try symbolic-ref of origin/HEAD
        if let Ok(output) = shell_env::command("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
            .current_dir(source_repo)
            .output()
            .await
            && output.status.success()
        {
            let refname = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // refs/remotes/origin/main -> main
            if let Some(branch) = refname.strip_prefix("refs/remotes/origin/") {
                return branch.to_string();
            }
        }
        // Fallback: check if "main" branch exists, otherwise "master"
        if let Ok(output) = shell_env::command("git")
            .args(["rev-parse", "--verify", "refs/heads/main"])
            .current_dir(source_repo)
            .output()
            .await
            && output.status.success()
        {
            return "main".to_string();
        }
        "master".to_string()
    }

    /// Remove a workspace: remove worktree, delete branch.
    /// Uses the workspace's `source_repo` to locate the git root.
    pub async fn remove(&self, name: &str, source_repo: &PathBuf) -> anyhow::Result<()> {
        let worktree_path = self.worktrees_base(source_repo).join(name);
        let branch_name = name.to_string();

        // git worktree remove --force <path>
        let output = shell_env::command("git")
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
        let _ = shell_env::command("git")
            .args(["branch", "-D", &branch_name])
            .current_dir(source_repo)
            .output()
            .await;

        tracing::info!(workspace = name, "workspace removed");

        Ok(())
    }
}

/// List immediate sub-directories of `path`, excluding hidden directories
/// (those whose name starts with `.`). Result is sorted lexicographically.
///
/// Used by Project-type workspaces to surface navigable child directories.
pub async fn list_subdirs(path: &std::path::Path) -> Vec<String> {
    let mut dirs = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(path).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(ft) = entry.file_type().await
                && ft.is_dir()
                && let Some(name) = entry.file_name().to_str()
                && !name.starts_with('.')
            {
                dirs.push(name.to_string());
            }
        }
    }
    dirs.sort();
    dirs
}

/// Inspect `git -C <root> remote get-url origin`. Returns
/// `WorkspaceOrigin::GitHub { url }` when the URL contains "github.com",
/// otherwise `WorkspaceOrigin::Local`.
async fn detect_origin_from_repo(root: &std::path::Path) -> WorkspaceOrigin {
    if let Ok(out) = shell_env::command("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(root)
        .output()
        .await
        && out.status.success()
    {
        let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if url.contains("github.com") {
            return WorkspaceOrigin::GitHub { url };
        }
    }
    WorkspaceOrigin::Local
}

/// Extract the repository name from a GitHub URL (or any clone-style URL).
/// Returns `None` when the trailing segment is empty.
///
/// Handles HTTPS (`https://github.com/foo/bar[.git][/]`), SCP-style SSH
/// (`git@github.com:foo/bar.git`), and `ssh://` URLs.
pub fn parse_github_repo_name(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let trimmed = trimmed.split(['?', '#']).next().unwrap_or(trimmed);
    let last = trimmed.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod url_tests {
    use super::parse_github_repo_name;

    #[test]
    fn parses_https() {
        assert_eq!(
            parse_github_repo_name("https://github.com/foo/bar.git").as_deref(),
            Some("bar")
        );
        assert_eq!(
            parse_github_repo_name("https://github.com/foo/bar").as_deref(),
            Some("bar")
        );
        assert_eq!(
            parse_github_repo_name("https://github.com/foo/bar/").as_deref(),
            Some("bar")
        );
    }

    #[test]
    fn parses_ssh() {
        assert_eq!(
            parse_github_repo_name("git@github.com:foo/bar.git").as_deref(),
            Some("bar")
        );
        assert_eq!(
            parse_github_repo_name("ssh://git@github.com/foo/bar.git").as_deref(),
            Some("bar")
        );
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_github_repo_name("").is_none());
        // A trailing-slash-only URL collapses to the host as the last segment;
        // that's fine here — git clone will fail with a clearer error than we
        // could synthesize.
        assert_eq!(
            parse_github_repo_name("https://github.com/").as_deref(),
            Some("github.com")
        );
    }
}
