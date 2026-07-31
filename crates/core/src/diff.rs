//! Diff parsing: unified-diff text into the shapes a UI can render.
//!
//! These are pure string parsers with nothing platform- or Tauri-specific
//! about them. They lived in the desktop app's command layer, beside — but
//! separate from — `github::parse_unified_diff`, where the TUI could not
//! reach them and where, until the desktop crate joined CI, nothing compiled
//! them under test. 230 lines of index arithmetic and marker matching with
//! no coverage at all.

use serde::Serialize;

/// A file diff laid out as aligned left/right columns.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct SideBySideDiff {
    pub left_title: String,
    pub right_title: String,
    pub file_path: String,
    pub hunks: Vec<DiffHunk>,
    pub stats: DiffStats,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub pairs: Vec<DiffPair>,
}

/// One rendered row: a left side, a right side, or both.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct DiffPair {
    pub left: Option<DiffSide>,
    pub right: Option<DiffSide>,
    /// "context", "modified", "added" or "deleted".
    pub pair_type: String,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct DiffSide {
    pub line_num: u32,
    pub content: String,
}

/// A file with merge-conflict markers, split into regions.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct ConflictDiff {
    pub file_path: String,
    pub ours_title: String,
    pub theirs_title: String,
    pub regions: Vec<ConflictRegion>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct ConflictRegion {
    /// "common" or "conflict".
    pub region_type: String,
    pub ours_lines: Vec<String>,
    pub theirs_lines: Vec<String>,
    pub base_lines: Vec<String>,
}

/// A diff line tagged for flat (non-columnar) rendering.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct LegacyDiffLine {
    pub content: String,
    pub line_type: String,
}

pub fn parse_side_by_side(
    raw: &str,
    left_title: &str,
    right_title: &str,
    file_path: &str,
) -> SideBySideDiff {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;
    let mut old_num: u32 = 0;
    let mut new_num: u32 = 0;
    let mut pending_dels: Vec<(u32, String)> = Vec::new();
    let mut pending_adds: Vec<(u32, String)> = Vec::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;

    let flush_pending =
        |dels: &mut Vec<(u32, String)>, adds: &mut Vec<(u32, String)>, hunk: &mut DiffHunk| {
            let max_len = dels.len().max(adds.len());
            for i in 0..max_len {
                let left = dels.get(i).map(|(n, c)| DiffSide {
                    line_num: *n,
                    content: c.clone(),
                });
                let right = adds.get(i).map(|(n, c)| DiffSide {
                    line_num: *n,
                    content: c.clone(),
                });
                let pair_type = match (&left, &right) {
                    (Some(_), Some(_)) => "modified",
                    (Some(_), None) => "deleted",
                    (None, Some(_)) => "added",
                    (None, None) => unreachable!(),
                };
                hunk.pairs.push(DiffPair {
                    left,
                    right,
                    pair_type: pair_type.to_string(),
                });
            }
            dels.clear();
            adds.clear();
        };

    for line in raw.lines() {
        // Skip file headers
        if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("new file")
            || line.starts_with("deleted file")
        {
            continue;
        }

        if line.starts_with("@@") {
            // Flush previous hunk
            if let Some(ref mut hunk) = current_hunk {
                flush_pending(&mut pending_dels, &mut pending_adds, hunk);
                hunks.push(hunk.clone());
            }

            // Parse @@ -old_start,count +new_start,count @@
            if let Some(rest) = line.strip_prefix("@@ ") {
                let parts: Vec<&str> = rest.splitn(3, ' ').collect();
                if parts.len() >= 2 {
                    if let Some(old_spec) = parts[0].strip_prefix('-') {
                        old_num = old_spec
                            .split(',')
                            .next()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(1);
                    }
                    if let Some(new_spec) = parts[1].strip_prefix('+') {
                        let clean = new_spec.split("@@").next().unwrap_or(new_spec);
                        new_num = clean
                            .split(',')
                            .next()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(1);
                    }
                }
            }

            current_hunk = Some(DiffHunk {
                header: line.to_string(),
                pairs: Vec::new(),
            });
            continue;
        }

        let hunk = match current_hunk.as_mut() {
            Some(h) => h,
            None => continue,
        };

        if let Some(content) = line.strip_prefix('+') {
            pending_adds.push((new_num, content.to_string()));
            new_num += 1;
            additions += 1;
        } else if let Some(content) = line.strip_prefix('-') {
            pending_dels.push((old_num, content.to_string()));
            old_num += 1;
            deletions += 1;
        } else {
            // Context line — flush any pending adds/dels first
            flush_pending(&mut pending_dels, &mut pending_adds, hunk);

            let content = line.strip_prefix(' ').unwrap_or(line);
            hunk.pairs.push(DiffPair {
                left: Some(DiffSide {
                    line_num: old_num,
                    content: content.to_string(),
                }),
                right: Some(DiffSide {
                    line_num: new_num,
                    content: content.to_string(),
                }),
                pair_type: "context".to_string(),
            });
            old_num += 1;
            new_num += 1;
        }
    }

    // Flush last hunk
    if let Some(ref mut hunk) = current_hunk {
        flush_pending(&mut pending_dels, &mut pending_adds, hunk);
        hunks.push(hunk.clone());
    }

    SideBySideDiff {
        left_title: left_title.to_string(),
        right_title: right_title.to_string(),
        file_path: file_path.to_string(),
        hunks,
        stats: DiffStats {
            additions,
            deletions,
        },
    }
}

pub fn parse_conflict_markers(content: &str) -> Vec<ConflictRegion> {
    let mut regions = Vec::new();
    let mut common_lines: Vec<String> = Vec::new();
    let mut ours_lines: Vec<String> = Vec::new();
    let mut theirs_lines: Vec<String> = Vec::new();
    let mut in_ours = false;
    let mut in_theirs = false;

    for line in content.lines() {
        if line.starts_with("<<<<<<<") {
            if !common_lines.is_empty() {
                regions.push(ConflictRegion {
                    region_type: "common".to_string(),
                    ours_lines: common_lines.clone(),
                    theirs_lines: common_lines.clone(),
                    base_lines: common_lines.clone(),
                });
                common_lines.clear();
            }
            in_ours = true;
            in_theirs = false;
        } else if line.starts_with("=======") {
            in_ours = false;
            in_theirs = true;
        } else if line.starts_with(">>>>>>>") {
            in_theirs = false;
            regions.push(ConflictRegion {
                region_type: "conflict".to_string(),
                ours_lines: ours_lines.clone(),
                theirs_lines: theirs_lines.clone(),
                base_lines: Vec::new(),
            });
            ours_lines.clear();
            theirs_lines.clear();
        } else if in_ours {
            ours_lines.push(line.to_string());
        } else if in_theirs {
            theirs_lines.push(line.to_string());
        } else {
            common_lines.push(line.to_string());
        }
    }

    if !common_lines.is_empty() {
        regions.push(ConflictRegion {
            region_type: "common".to_string(),
            ours_lines: common_lines.clone(),
            theirs_lines: common_lines.clone(),
            base_lines: common_lines,
        });
    }

    regions
}

pub fn parse_legacy_diff_lines(output: &str) -> Vec<LegacyDiffLine> {
    output
        .lines()
        .map(|line| {
            let line_type = if line.starts_with('+') && !line.starts_with("+++") {
                "add"
            } else if line.starts_with('-') && !line.starts_with("---") {
                "del"
            } else if line.starts_with("@@") {
                "hunk"
            } else if line.starts_with("diff ")
                || line.starts_with("index ")
                || line.starts_with("---")
                || line.starts_with("+++")
            {
                "header"
            } else {
                "context"
            };
            LegacyDiffLine {
                content: line.to_string(),
                line_type: line_type.to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sbs(raw: &str) -> SideBySideDiff {
        parse_side_by_side(raw, "before", "after", "src/lib.rs")
    }

    const SIMPLE: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
index 111..222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,3 +10,3 @@ fn thing() {
 unchanged
-old line
+new line
";

    #[test]
    fn side_by_side_skips_file_headers_and_keeps_the_hunk_header() {
        let d = sbs(SIMPLE);
        assert_eq!(d.hunks.len(), 1);
        assert!(d.hunks[0].header.starts_with("@@ -10,3 +10,3 @@"));
        assert_eq!(d.file_path, "src/lib.rs");
        assert_eq!(d.left_title, "before");
    }

    #[test]
    fn line_numbers_start_from_the_hunk_header() {
        let d = sbs(SIMPLE);
        let pairs = &d.hunks[0].pairs;
        // Context row carries the same number on both sides.
        assert_eq!(pairs[0].pair_type, "context");
        assert_eq!(pairs[0].left.as_ref().unwrap().line_num, 10);
        assert_eq!(pairs[0].right.as_ref().unwrap().line_num, 10);
        // The -/+ pair lines up as one "modified" row on line 11.
        assert_eq!(pairs[1].pair_type, "modified");
        assert_eq!(pairs[1].left.as_ref().unwrap().content, "old line");
        assert_eq!(pairs[1].right.as_ref().unwrap().content, "new line");
    }

    #[test]
    fn stats_count_additions_and_deletions() {
        let d = sbs(SIMPLE);
        assert_eq!(
            d.stats,
            DiffStats {
                additions: 1,
                deletions: 1
            }
        );
    }

    /// Unequal runs pad the shorter side, which is what makes the columns
    /// line up: 1 deletion against 2 additions is one modified row plus one
    /// added row.
    #[test]
    fn uneven_runs_pad_the_shorter_side() {
        let d = sbs("@@ -1,1 +1,2 @@\n-only old\n+new a\n+new b\n");
        let pairs = &d.hunks[0].pairs;
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].pair_type, "modified");
        assert_eq!(pairs[1].pair_type, "added");
        assert!(pairs[1].left.is_none());
        assert_eq!(pairs[1].right.as_ref().unwrap().content, "new b");
    }

    #[test]
    fn a_pure_deletion_leaves_the_right_side_empty() {
        let d = sbs("@@ -5,2 +5,0 @@\n-gone one\n-gone two\n");
        let pairs = &d.hunks[0].pairs;
        assert_eq!(pairs.len(), 2);
        assert!(pairs.iter().all(|p| p.pair_type == "deleted"));
        assert!(pairs.iter().all(|p| p.right.is_none()));
        assert_eq!(pairs[0].left.as_ref().unwrap().line_num, 5);
        assert_eq!(pairs[1].left.as_ref().unwrap().line_num, 6);
    }

    #[test]
    fn multiple_hunks_are_kept_separate() {
        let d = sbs("@@ -1,1 +1,1 @@\n-a\n+b\n@@ -50,1 +50,1 @@\n-c\n+d\n");
        assert_eq!(d.hunks.len(), 2);
        assert_eq!(d.hunks[1].pairs[0].left.as_ref().unwrap().line_num, 50);
        assert_eq!(
            d.stats,
            DiffStats {
                additions: 2,
                deletions: 2
            }
        );
    }

    /// Lines before any `@@` have no hunk to belong to and are dropped
    /// rather than panicking.
    #[test]
    fn content_before_the_first_hunk_is_ignored() {
        let d = sbs("+stray addition\n-stray deletion\n");
        assert!(d.hunks.is_empty());
    }

    #[test]
    fn empty_input_yields_no_hunks() {
        let d = sbs("");
        assert!(d.hunks.is_empty());
        assert_eq!(
            d.stats,
            DiffStats {
                additions: 0,
                deletions: 0
            }
        );
    }

    // ── Conflict markers ──

    const CONFLICTED: &str = "\
before
<<<<<<< HEAD
ours one
ours two
=======
theirs one
>>>>>>> branch
after
";

    #[test]
    fn conflict_regions_split_common_from_conflicting() {
        let regions = parse_conflict_markers(CONFLICTED);
        assert_eq!(regions.len(), 3);
        assert_eq!(regions[0].region_type, "common");
        assert_eq!(regions[0].ours_lines, ["before"]);
        assert_eq!(regions[1].region_type, "conflict");
        assert_eq!(regions[1].ours_lines, ["ours one", "ours two"]);
        assert_eq!(regions[1].theirs_lines, ["theirs one"]);
        assert_eq!(regions[2].region_type, "common");
        assert_eq!(regions[2].ours_lines, ["after"]);
    }

    #[test]
    fn a_common_region_shows_the_same_text_on_both_sides() {
        let regions = parse_conflict_markers("just text\n");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].ours_lines, regions[0].theirs_lines);
        assert_eq!(regions[0].base_lines, ["just text"]);
    }

    #[test]
    fn a_file_without_markers_is_one_common_region() {
        let regions = parse_conflict_markers("a\nb\nc\n");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].region_type, "common");
        assert_eq!(regions[0].ours_lines.len(), 3);
    }

    #[test]
    fn conflict_at_the_very_start_emits_no_empty_common_region() {
        let regions = parse_conflict_markers("<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\n");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].region_type, "conflict");
    }

    // ── Legacy flat lines ──

    #[test]
    fn legacy_lines_are_tagged_by_prefix() {
        let lines = parse_legacy_diff_lines(SIMPLE);
        let kinds: Vec<&str> = lines.iter().map(|l| l.line_type.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "header", // diff --git
                "header", // index
                "header", // ---
                "header", // +++
                "hunk",   // @@
                "context", "del", "add",
            ]
        );
    }

    /// `+++`/`---` are file headers, not an addition/deletion — the check
    /// that distinguishes them is easy to break.
    #[test]
    fn triple_markers_are_headers_not_changes() {
        let lines = parse_legacy_diff_lines("+++ b/x\n--- a/x\n");
        assert_eq!(lines[0].line_type, "header");
        assert_eq!(lines[1].line_type, "header");
    }

    #[test]
    fn legacy_preserves_content_verbatim() {
        let lines = parse_legacy_diff_lines("+added\n");
        assert_eq!(lines[0].content, "+added", "the marker stays in content");
    }
}
