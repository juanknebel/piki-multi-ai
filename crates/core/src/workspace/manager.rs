use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use anyhow::{Context, bail};
use crate::shell_env;

use crate::domain::{DirEntry, EntryKind, WorkspaceInfo, WorkspaceOrigin, WorkspaceType};
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
            worktree_path,
            git_root,
        );
        info.origin = origin;
        Ok(info)
    }

    /// Create a simple workspace pointing to an existing directory.
    /// The directory is **not** required to be inside a git repository; when
    /// no git root can be detected, `source_repo` falls back to `dir` itself
    /// and `origin` is `Local`.
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
        let (source_repo, origin) = match Self::git_root(dir).await {
            Ok(root) => {
                let origin = detect_origin_from_repo(&root).await;
                (root, origin)
            }
            Err(_) => (dir.clone(), WorkspaceOrigin::Local),
        };

        let mut info = WorkspaceInfo::new(
            name.to_string(),
            description.to_string(),
            prompt.to_string(),
            kanban_path,
            dir.clone(),
            source_repo,
        );
        info.workspace_type = WorkspaceType::Simple;
        info.origin = origin;
        tracing::info!(workspace = name, path = %dir.display(), "simple workspace created");
        Ok(info)
    }

    /// Clone a GitHub repository into a user-chosen destination directory
    /// and register it as a Simple workspace tagged with
    /// `WorkspaceOrigin::GitHub`. The actual clone lands at
    /// `destination_dir.join(repo_name)`, where `repo_name` is parsed from
    /// the URL. The dialog typically pre-fills `destination_dir` with
    /// [`DataPaths::repos_dir`] as a hint.
    pub async fn create_from_github(
        &self,
        name: &str,
        description: &str,
        prompt: &str,
        kanban_path: Option<String>,
        github_url: &str,
        destination_dir: &std::path::Path,
    ) -> anyhow::Result<WorkspaceInfo> {
        let repo_name = parse_github_repo_name(github_url)
            .ok_or_else(|| anyhow::anyhow!("invalid GitHub URL: {}", github_url))?;

        // Auto-create the dialog-default `<data_dir>/repos` parent so the
        // first-run flow Just Works; for any other user-chosen path we
        // require it to exist already — surfacing a typo rather than
        // silently creating an unrelated directory tree.
        let is_default_repos_dir = destination_dir == self.paths.repos_dir();
        if !destination_dir.exists() {
            if is_default_repos_dir {
                tokio::fs::create_dir_all(destination_dir)
                    .await
                    .context("failed to create default repos directory")?;
            } else {
                bail!(
                    "destination folder '{}' does not exist",
                    destination_dir.display()
                );
            }
        } else if !destination_dir.is_dir() {
            bail!(
                "destination '{}' is not a directory",
                destination_dir.display()
            );
        }

        let destination = destination_dir.join(&repo_name);
        if destination.exists() {
            bail!(
                "'{}' already exists; refusing to overwrite",
                destination.display()
            );
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

        let mut info = WorkspaceInfo::new(
            name.to_string(),
            description.to_string(),
            prompt.to_string(),
            kanban_path,
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

    /// List git worktrees that already exist on disk for the repository
    /// containing `source_dir`, excluding the main/bare checkout. Used to
    /// offer "load an existing worktree" as an alternative to `create()`.
    pub async fn list_worktrees(&self, source_dir: &PathBuf) -> anyhow::Result<Vec<ExistingWorktree>> {
        let git_root = Self::git_root(source_dir).await?;
        let output = shell_env::command("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&git_root)
            .output()
            .await
            .context("failed to run git worktree list")?;

        if !output.status.success() {
            bail!(
                "git worktree list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let text = String::from_utf8_lossy(&output.stdout);
        Ok(parse_worktree_list(&text, &git_root))
    }

    /// Register an already-existing worktree directory as a new workspace,
    /// without shelling out to `git worktree add`. `worktree_path` is
    /// expected to come from `list_worktrees()`.
    pub async fn import_existing_worktree(
        &self,
        name: &str,
        worktree_path: PathBuf,
        source_repo: PathBuf,
    ) -> anyhow::Result<WorkspaceInfo> {
        if !worktree_path.exists() {
            bail!(
                "worktree '{}' no longer exists on disk",
                worktree_path.display()
            );
        }

        let origin = detect_origin_from_repo(&source_repo).await;
        let mut info = WorkspaceInfo::new(
            name.to_string(),
            String::new(),
            String::new(),
            None,
            worktree_path,
            source_repo,
        );
        info.origin = origin;
        info.workspace_type = WorkspaceType::Worktree;
        Ok(info)
    }
}

/// One entry from `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingWorktree {
    pub path: PathBuf,
    pub branch: String,
}

/// Parse `git worktree list --porcelain` output into `ExistingWorktree`
/// entries, excluding the main/bare worktree at `git_root`.
fn parse_worktree_list(text: &str, git_root: &std::path::Path) -> Vec<ExistingWorktree> {
    let mut result = Vec::new();
    let mut cur_path: Option<PathBuf> = None;
    let mut cur_branch: Option<String> = None;
    let mut is_bare = false;

    for line in text.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(path) = cur_path.take()
                && !is_bare
                && path != git_root
            {
                result.push(ExistingWorktree {
                    path,
                    branch: cur_branch.take().unwrap_or_else(|| "(detached)".to_string()),
                });
            }
            cur_branch = None;
            is_bare = false;
            continue;
        }
        if let Some(p) = line.strip_prefix("worktree ") {
            cur_path = Some(PathBuf::from(p));
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            cur_branch = Some(b.to_string());
        } else if line == "bare" {
            is_bare = true;
        }
    }

    result
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

/// Lists the immediate children of `path` (one level, non-recursive).
///
/// Directories sort first, then symlinks, then files; each group is sorted
/// case-insensitively by name. Dot-prefixed entries are omitted unless
/// `show_hidden` is set. Returns an empty vec if `path` can't be read.
pub async fn read_dir_entries(path: &std::path::Path, show_hidden: bool) -> Vec<DirEntry> {
    let mut entries: Vec<DirEntry> = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(path).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') && !show_hidden {
                continue;
            }
            // `metadata()` follows symlinks (one syscall for type+size+mtime);
            // fall back to the entry's own (symlink) metadata for broken links
            // so they aren't silently dropped from the listing.
            let (meta, was_symlink) = match tokio::fs::metadata(entry.path()).await {
                Ok(m) => (Some(m), false),
                Err(_) => (entry.metadata().await.ok(), true),
            };
            let Some(meta) = meta else { continue };
            let kind = if was_symlink {
                EntryKind::Symlink
            } else if meta.is_dir() {
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            entries.push(DirEntry {
                name,
                kind,
                size: meta.len(),
                mtime,
            });
        }
    }
    entries.sort_by(|a, b| {
        let rank = |k: &EntryKind| match k {
            EntryKind::Dir => 0,
            EntryKind::Symlink => 1,
            EntryKind::File => 2,
        };
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
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

#[cfg(test)]
mod worktree_list_tests {
    use super::{ExistingWorktree, parse_worktree_list};
    use std::path::PathBuf;

    #[test]
    fn excludes_main_worktree_and_parses_branches() {
        let git_root = PathBuf::from("/repo");
        let text = "worktree /repo\n\
                     HEAD deadbeef\n\
                     branch refs/heads/main\n\
                     \n\
                     worktree /repo-worktrees/feature-a\n\
                     HEAD cafebabe\n\
                     branch refs/heads/feature-a\n\
                     \n";
        let result = parse_worktree_list(text, &git_root);
        assert_eq!(
            result,
            vec![ExistingWorktree {
                path: PathBuf::from("/repo-worktrees/feature-a"),
                branch: "feature-a".to_string(),
            }]
        );
    }

    #[test]
    fn excludes_bare_worktree() {
        let git_root = PathBuf::from("/repo.git");
        let text = "worktree /repo.git\n\
                     bare\n\
                     \n\
                     worktree /repo-worktrees/feature-a\n\
                     branch refs/heads/feature-a\n\
                     \n";
        let result = parse_worktree_list(text, &git_root);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/repo-worktrees/feature-a"));
    }

    #[test]
    fn detached_worktree_reports_placeholder_branch() {
        let git_root = PathBuf::from("/repo");
        let text = "worktree /repo\n\
                     branch refs/heads/main\n\
                     \n\
                     worktree /repo-worktrees/detached\n\
                     HEAD cafebabe\n\
                     detached\n\
                     \n";
        let result = parse_worktree_list(text, &git_root);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].branch, "(detached)");
    }

    #[test]
    fn empty_output_yields_empty_list() {
        assert!(parse_worktree_list("", &PathBuf::from("/repo")).is_empty());
    }
}
