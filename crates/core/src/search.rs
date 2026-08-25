//! Project-wide search, shared by the TUI overlays and the desktop's
//! Search-in-Project panel / file finder.
//!
//! Content search shells out to `rg` (fast, respects `.gitignore`) and falls
//! back to `grep -rn` when ripgrep isn't installed. Fixed-string matching —
//! the query is user-typed text, not a regex.
//!
//! The file index ([`list_files`]) walks the tree with the `ignore` crate
//! (ripgrep's walker), so it honours `.gitignore`, `.ignore`, `.git/info/exclude`
//! and the user's global excludes without a hardcoded exclusion list.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::Serialize;

/// Upper bound on the paths a file index holds. Enough for any repo a human
/// fuzzy-searches by hand; past it the walk stops and `truncated` is set.
pub const FILE_INDEX_CAP: usize = 50_000;

/// Workspace-relative file paths, sorted, ready for fuzzy matching.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FileIndex {
    pub files: Vec<String>,
    /// The walk hit the cap and stopped early — the list is incomplete.
    pub truncated: bool,
}

impl FileIndex {
    /// Whether `rel` (workspace-relative, `/`-separated) is in the index.
    pub fn contains(&self, rel: &str) -> bool {
        self.files.binary_search_by(|f| f.as_str().cmp(rel)).is_ok()
    }
}

/// List every regular file under `root`, at most `cap` of them, honouring
/// gitignore rules. Blocking — call from `spawn_blocking`.
///
/// Rules: `.gitignore` / `.ignore` / `.git/info/exclude` / global excludes
/// apply even when `root` is not itself a git repo (`require_git(false)`);
/// hidden entries are *kept* (`.github/`, `.cargo/`, dotfiles are things
/// people edit) except `.git` itself, which is always pruned; symlinks are
/// not followed. Paths are relative to `root`, sorted.
pub fn list_files(root: &Path, cap: usize) -> FileIndex {
    use ignore::{WalkBuilder, WalkState};

    let files = parking_lot::Mutex::new(Vec::new());
    let count = AtomicUsize::new(0);
    let truncated = AtomicBool::new(false);

    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                let Ok(entry) = entry else {
                    return WalkState::Continue;
                };
                if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                    return WalkState::Continue;
                }
                let Ok(rel) = entry.path().strip_prefix(root) else {
                    return WalkState::Continue;
                };
                if count.fetch_add(1, Ordering::Relaxed) >= cap {
                    truncated.store(true, Ordering::Relaxed);
                    return WalkState::Quit;
                }
                files.lock().push(rel.to_string_lossy().into_owned());
                WalkState::Continue
            })
        });

    let mut files = files.into_inner();
    files.sort_unstable();
    files.truncate(cap);
    FileIndex {
        files,
        truncated: truncated.load(Ordering::Relaxed),
    }
}

/// One content match: workspace-relative path, 1-based line, line text.
#[derive(Debug, Clone, Serialize)]
pub struct SearchMatch {
    pub path: String,
    pub line_num: u32,
    pub text: String,
}

/// Search `query` as a literal string under `root`, returning at most
/// `limit` matches. An empty query returns no matches without spawning.
pub async fn project_search(
    root: &Path,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<SearchMatch>> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let output = match tokio::process::Command::new("rg")
        .args([
            "--no-heading",
            "--line-number",
            "--color=never",
            "--fixed-strings",
            "--",
            query,
        ])
        .current_dir(root)
        .output()
        .await
    {
        Ok(out) => out,
        Err(_) => tokio::process::Command::new("grep")
            .args(["-rn", "--color=never", "-I", "-F", "--", query, "."])
            .current_dir(root)
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("neither rg nor grep available: {e}"))?,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_grep_lines(&stdout, limit))
}

/// Parse `file:line_num:text` lines (the shared rg/grep output shape).
fn parse_grep_lines(stdout: &str, limit: usize) -> Vec<SearchMatch> {
    stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(3, ':');
            let raw_path = parts.next()?;
            let path = raw_path.strip_prefix("./").unwrap_or(raw_path);
            let line_num: u32 = parts.next()?.parse().ok()?;
            let text = parts.next().unwrap_or("").to_string();
            Some(SearchMatch {
                path: path.to_string(),
                line_num,
                text,
            })
        })
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rg_output_lines() {
        let out = "src/main.rs:10:fn main() {\n./lib.rs:3:pub mod x;\n";
        let hits = parse_grep_lines(out, 100);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].path, "src/main.rs");
        assert_eq!(hits[0].line_num, 10);
        assert_eq!(hits[0].text, "fn main() {");
        // grep's `./` prefix is stripped so both tools yield the same paths.
        assert_eq!(hits[1].path, "lib.rs");
    }

    #[test]
    fn skips_malformed_lines_and_honors_limit() {
        let out = "no-colons-here\nา:x:y\nsrc/a.rs:1:one\nsrc/b.rs:2:two\n";
        let hits = parse_grep_lines(out, 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "src/a.rs");
    }

    #[test]
    fn text_may_contain_colons() {
        let hits = parse_grep_lines("a.rs:5:let x: Vec<u8> = vec![];\n", 10);
        assert_eq!(hits[0].text, "let x: Vec<u8> = vec![];");
    }

    #[tokio::test]
    async fn empty_query_returns_nothing() {
        let hits = project_search(Path::new("."), "", 10).await.unwrap();
        assert!(hits.is_empty());
    }

    fn touch(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn list_files_honours_gitignore_keeps_dotfiles_and_prunes_dot_git() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, ".gitignore", "target/\n*.log\n");
        touch(root, "src/main.rs", "fn main() {}");
        touch(root, "target/debug/app", "bin");
        touch(root, "build.log", "noise");
        touch(root, ".github/workflows/ci.yml", "on: push");
        touch(root, ".git/HEAD", "ref: refs/heads/main");
        touch(root, ".git/objects/ab/cd", "blob");

        let idx = list_files(root, FILE_INDEX_CAP);
        assert_eq!(
            idx.files,
            vec![".github/workflows/ci.yml", ".gitignore", "src/main.rs"]
        );
        assert!(!idx.truncated);
        assert!(idx.contains("src/main.rs"));
        assert!(!idx.contains("target/debug/app"));
    }

    #[test]
    fn list_files_stops_at_the_cap_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for i in 0..12 {
            touch(root, &format!("f{i:02}.txt"), "");
        }

        let idx = list_files(root, 5);
        assert_eq!(idx.files.len(), 5);
        assert!(idx.truncated);
        assert!(idx.files.windows(2).all(|w| w[0] < w[1]), "sorted");

        let full = list_files(root, 12);
        assert_eq!(full.files.len(), 12);
        assert!(!full.truncated);
    }
}
