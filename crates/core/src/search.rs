//! Project-wide content search, shared by the TUI overlay and the desktop's
//! Search-in-Project panel.
//!
//! Shells out to `rg` (fast, respects `.gitignore`) and falls back to
//! `grep -rn` when ripgrep isn't installed. Fixed-string matching — the query
//! is user-typed text, not a regex.

use std::path::Path;

use serde::Serialize;

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
}
