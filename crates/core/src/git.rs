use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::domain::{ChangedFile, FileStatus};

/// Parse `git status --porcelain=v1` output into ChangedFile list.
///
/// Format: `XY path` where X = index status, Y = working tree status.
/// For renames: `XY old_path -> new_path`
pub fn parse_porcelain_status(output: &str) -> Vec<ChangedFile> {
    output
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let x = line.as_bytes()[0];
            let y = line.as_bytes()[1];
            // Path starts at index 3 (after "XY ")
            let path_part = &line[3..];

            // Untracked
            if x == b'?' && y == b'?' {
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status: FileStatus::Untracked,
                });
            }

            // Conflicts: UU, AA, DD, AU, UA, DU, UD
            if x == b'U' || y == b'U' || (x == b'A' && y == b'A') || (x == b'D' && y == b'D') {
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status: FileStatus::Conflicted,
                });
            }

            // Renamed (in index)
            if x == b'R' {
                // Path format: "old_path -> new_path"
                let display_path = if let Some((_old, new)) = path_part.split_once(" -> ") {
                    new.to_string()
                } else {
                    path_part.to_string()
                };
                return Some(ChangedFile {
                    path: display_path,
                    status: FileStatus::Renamed,
                });
            }

            // Staged + modified in working tree
            if x != b' ' && x != b'?' && y == b'M' {
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status: FileStatus::StagedModified,
                });
            }

            // Staged only (index has changes, working tree clean)
            if y == b' ' {
                let status = match x {
                    b'M' => FileStatus::Staged,
                    b'A' => FileStatus::Added,
                    b'D' => FileStatus::Deleted,
                    _ => return None,
                };
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status,
                });
            }

            // Working tree changes only (unstaged)
            if x == b' ' {
                let status = match y {
                    b'M' => FileStatus::Modified,
                    b'D' => FileStatus::Deleted,
                    _ => return None,
                };
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status,
                });
            }

            None
        })
        .collect()
}

/// Run `git status --porcelain=v1` in a worktree and return changed files
pub async fn get_changed_files(worktree_path: &PathBuf) -> anyhow::Result<Vec<ChangedFile>> {
    let output = crate::shell_env::command("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(worktree_path)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files = parse_porcelain_status(&stdout);
    tracing::debug!(path = %worktree_path.display(), count = files.len(), "get_changed_files");
    Ok(files)
}

/// Get ahead/behind counts relative to upstream.
/// Returns None if there's no upstream configured.
pub async fn get_ahead_behind(worktree_path: &PathBuf) -> Option<(usize, usize)> {
    let output = crate::shell_env::command("git")
        .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .current_dir(worktree_path)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = parse_ahead_behind(&stdout);
    if let Some((ahead, behind)) = parsed {
        tracing::debug!(path = %worktree_path.display(), ahead, behind, "ahead_behind");
    }
    parsed
}

/// Parse the stdout of `git rev-list --left-right --count HEAD...@{upstream}`
/// (`"<ahead>\t<behind>"`) into `(ahead, behind)`.
pub fn parse_ahead_behind(stdout: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = stdout.trim().split('\t').collect();
    if parts.len() == 2 {
        let ahead = parts[0].trim().parse().unwrap_or(0);
        let behind = parts[1].trim().parse().unwrap_or(0);
        Some((ahead, behind))
    } else {
        None
    }
}

/// Get the current branch name for a worktree.
/// Returns None if not a git repo, detached HEAD, or on any failure — never a stale value.
pub async fn get_current_branch(worktree_path: &PathBuf) -> Option<String> {
    let output = crate::shell_env::command("git")
        .args(["branch", "--show-current"])
        .current_dir(worktree_path)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

/// One entry of the branch switcher list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchInfo {
    /// `main` for a local branch, `origin/main` for a remote-tracking one.
    pub name: String,
    /// Checked out in this worktree.
    pub current: bool,
    /// A remote-tracking branch with no local counterpart (checkout creates
    /// a tracking branch).
    pub remote: bool,
    /// Upstream of a local branch (`origin/main`), when configured.
    pub upstream: Option<String>,
}

/// `--format` for `git branch --list`: `HEAD marker <TAB> full refname <TAB> upstream`.
pub const BRANCH_LIST_FORMAT: &str = "%(HEAD)%09%(refname)%09%(upstream:short)";

/// Parse `git branch --list [--all] --format=BRANCH_LIST_FORMAT` output.
///
/// Local branches come first in git's order, then remote-tracking branches
/// that have no local counterpart — a remote is dropped when a local branch
/// tracks it (`upstream`) or simply shares its short name (`origin/foo` vs
/// `foo`), since checking it out would collide. `<remote>/HEAD` symrefs are
/// skipped.
pub fn parse_branch_list(output: &str) -> Vec<BranchInfo> {
    let mut locals: Vec<BranchInfo> = Vec::new();
    let mut remotes: Vec<BranchInfo> = Vec::new();

    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let head = parts.next().unwrap_or("").trim();
        let refname = parts.next().unwrap_or("").trim();
        let upstream = parts.next().unwrap_or("").trim();
        if refname.is_empty() {
            continue;
        }

        if let Some(name) = refname.strip_prefix("refs/heads/") {
            locals.push(BranchInfo {
                name: name.to_string(),
                current: head == "*",
                remote: false,
                upstream: (!upstream.is_empty()).then(|| upstream.to_string()),
            });
        } else if let Some(name) = refname.strip_prefix("refs/remotes/") {
            if name.ends_with("/HEAD") {
                continue;
            }
            remotes.push(BranchInfo {
                name: name.to_string(),
                current: false,
                remote: true,
                upstream: None,
            });
        }
    }

    let tracked: HashSet<&str> = locals
        .iter()
        .filter_map(|b| b.upstream.as_deref())
        .collect();
    let local_names: HashSet<&str> = locals.iter().map(|b| b.name.as_str()).collect();
    remotes.retain(|r| {
        if tracked.contains(r.name.as_str()) {
            return false;
        }
        // `origin/feature/x` → `feature/x`
        match r.name.split_once('/') {
            Some((_, short)) => !local_names.contains(short),
            None => true,
        }
    });

    locals.extend(remotes);
    locals
}

/// List branches of a worktree (see [`parse_branch_list`] for the shape).
pub async fn list_branches(
    worktree_path: &PathBuf,
    include_remotes: bool,
) -> anyhow::Result<Vec<BranchInfo>> {
    let mut args = vec!["branch", "--list"];
    if include_remotes {
        args.push("--all");
    }
    let format = format!("--format={BRANCH_LIST_FORMAT}");
    args.push(&format);

    let output = crate::shell_env::command("git")
        .args(&args)
        .current_dir(worktree_path)
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(
            "git branch failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(parse_branch_list(&String::from_utf8_lossy(&output.stdout)))
}

/// Full message (subject + body) of the commit at HEAD, trimmed.
pub async fn last_commit_message(worktree_path: &PathBuf) -> anyhow::Result<String> {
    let output = crate::shell_env::command("git")
        .args(["log", "-1", "--pretty=%B"])
        .current_dir(worktree_path)
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Human summary of a successful `git pull`, from the ahead/behind counts
/// before and after it and git's stdout.
pub fn pull_summary(
    before: Option<(usize, usize)>,
    after: Option<(usize, usize)>,
    stdout: &str,
) -> String {
    if stdout.contains("Already up to date") {
        return "Already up to date".to_string();
    }
    let behind_before = before.map(|(_, b)| b).unwrap_or(0);
    let behind_after = after.map(|(_, b)| b).unwrap_or(0);
    let pulled = behind_before.saturating_sub(behind_after);
    if pulled > 0 {
        return format!(
            "Pulled {pulled} commit{}",
            if pulled == 1 { "" } else { "s" }
        );
    }
    stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.to_string())
        .unwrap_or_else(|| "Pulled".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_porcelain_modified_unstaged() {
        let input = " M src/main.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, FileStatus::Modified);
    }

    #[test]
    fn test_parse_porcelain_staged() {
        let input = "M  src/main.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, FileStatus::Staged);
    }

    #[test]
    fn test_parse_porcelain_staged_modified() {
        let input = "MM src/main.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, FileStatus::StagedModified);
    }

    #[test]
    fn test_parse_porcelain_added() {
        let input = "A  src/new.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/new.rs");
        assert_eq!(files[0].status, FileStatus::Added);
    }

    #[test]
    fn test_parse_porcelain_deleted() {
        let input = " D old_file.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "old_file.rs");
        assert_eq!(files[0].status, FileStatus::Deleted);
    }

    #[test]
    fn test_parse_porcelain_untracked() {
        let input = "?? new_file.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "new_file.rs");
        assert_eq!(files[0].status, FileStatus::Untracked);
    }

    #[test]
    fn test_parse_porcelain_conflicts() {
        let input = "UU conflict.rs\nAA both_added.rs\nDD both_deleted.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].status, FileStatus::Conflicted);
        assert_eq!(files[1].status, FileStatus::Conflicted);
        assert_eq!(files[2].status, FileStatus::Conflicted);
    }

    #[test]
    fn test_parse_porcelain_renamed() {
        let input = "R  old.rs -> new.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Renamed);
        assert_eq!(files[0].path, "new.rs");
    }

    #[test]
    fn test_parse_porcelain_empty() {
        let files = parse_porcelain_status("");
        assert!(files.is_empty());
    }

    #[test]
    fn test_parse_porcelain_mixed() {
        let input = " M src/app.rs\nA  src/new.rs\n?? untracked.txt\nMM both.rs\nD  deleted.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 5);
        assert_eq!(files[0].status, FileStatus::Modified);
        assert_eq!(files[1].status, FileStatus::Added);
        assert_eq!(files[2].status, FileStatus::Untracked);
        assert_eq!(files[3].status, FileStatus::StagedModified);
        assert_eq!(files[4].status, FileStatus::Deleted);
    }

    #[test]
    fn test_parse_porcelain_malformed() {
        let input = "x\n\n M valid.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "valid.rs");
    }

    #[test]
    fn test_parse_ahead_behind() {
        assert_eq!(parse_ahead_behind("2\t3\n"), Some((2, 3)));
        assert_eq!(parse_ahead_behind("0\t0"), Some((0, 0)));
        assert_eq!(parse_ahead_behind(""), None);
        assert_eq!(parse_ahead_behind("garbage"), None);
    }

    #[test]
    fn test_parse_branch_list_marks_current_and_upstream() {
        let out = "*\trefs/heads/main\torigin/main\n \trefs/heads/feature\t\n";
        let branches = parse_branch_list(out);
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].current);
        assert!(!branches[0].remote);
        assert_eq!(branches[0].upstream.as_deref(), Some("origin/main"));
        assert_eq!(branches[1].name, "feature");
        assert!(!branches[1].current);
        assert_eq!(branches[1].upstream, None);
    }

    #[test]
    fn test_parse_branch_list_remotes_without_local_counterpart() {
        let out = concat!(
            "*\trefs/heads/main\torigin/main\n",
            " \trefs/heads/feature\t\n",
            " \trefs/remotes/origin/HEAD\t\n",
            " \trefs/remotes/origin/main\t\n",
            " \trefs/remotes/origin/feature\t\n",
            " \trefs/remotes/origin/feature/nested\t\n",
            " \trefs/remotes/upstream/release\t\n",
        );
        let names: Vec<(String, bool)> = parse_branch_list(out)
            .into_iter()
            .map(|b| (b.name, b.remote))
            .collect();
        // origin/HEAD skipped; origin/main tracked by main; origin/feature
        // shares its short name with local `feature`.
        assert_eq!(
            names,
            vec![
                ("main".to_string(), false),
                ("feature".to_string(), false),
                ("origin/feature/nested".to_string(), true),
                ("upstream/release".to_string(), true),
            ]
        );
    }

    #[test]
    fn test_parse_branch_list_detached_head_has_no_current() {
        // `git branch` on a detached HEAD lists `(HEAD detached at …)` with
        // no refs/heads prefix — it must not produce an entry.
        let out = "*\t(HEAD detached at abc123)\t\n \trefs/heads/main\torigin/main\n";
        let branches = parse_branch_list(out);
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");
        assert!(!branches[0].current);
    }

    #[test]
    fn test_pull_summary() {
        assert_eq!(
            pull_summary(Some((0, 0)), Some((0, 0)), "Already up to date.\n"),
            "Already up to date"
        );
        assert_eq!(
            pull_summary(Some((1, 3)), Some((1, 0)), "Updating a..b\nFast-forward\n"),
            "Pulled 3 commits"
        );
        assert_eq!(
            pull_summary(Some((0, 1)), Some((0, 0)), ""),
            "Pulled 1 commit"
        );
        // No upstream info before the pull: fall back to git's first line.
        assert_eq!(
            pull_summary(None, Some((0, 0)), "\nUpdating a..b\nFast-forward\n"),
            "Updating a..b"
        );
        assert_eq!(pull_summary(None, None, ""), "Pulled");
    }

    /// End-to-end against a throwaway repo: exercises the real `--format`
    /// string, not just the parser.
    #[tokio::test]
    async fn test_list_branches_in_temp_repo() {
        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("git not installed; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                // Isolate from the developer's global config (signing, hooks…).
                .env("HOME", &root)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .expect("run git");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-q"]);
        git(&["checkout", "-q", "-b", "main"]);
        git(&["commit", "-q", "--allow-empty", "-m", "init"]);
        git(&["branch", "feature"]);

        let branches = list_branches(&root, true).await.unwrap();
        let main = branches.iter().find(|b| b.name == "main").unwrap();
        let feature = branches.iter().find(|b| b.name == "feature").unwrap();
        assert!(main.current);
        assert!(!feature.current);
        assert!(branches.iter().all(|b| !b.remote));

        let msg = last_commit_message(&root).await.unwrap();
        assert_eq!(msg, "init");
    }
}
