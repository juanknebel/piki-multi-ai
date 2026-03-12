use std::path::PathBuf;

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
    let output = tokio::process::Command::new("git")
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
    let output = tokio::process::Command::new("git")
        .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .current_dir(worktree_path)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split('\t').collect();
    if parts.len() == 2 {
        let ahead = parts[0].parse().unwrap_or(0);
        let behind = parts[1].parse().unwrap_or(0);
        tracing::debug!(path = %worktree_path.display(), ahead, behind, "ahead_behind");
        Some((ahead, behind))
    } else {
        None
    }
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
}
