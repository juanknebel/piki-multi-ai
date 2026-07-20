use std::collections::HashMap;
use std::path::PathBuf;

use piki_core::github::{
    ExistingComment, InlineComment, ParsedDiff, PrCheckout, PrFile, PrInfo, ReviewVerdict,
};

/// Result of resolving a picked PR into a ready-to-open review: the checkout
/// (path, base spec, PR metadata) plus the file list, fetched once the
/// checkout is in place. Handed from the background task (`OpenPrReview`)
/// to the poller in `event_loop.rs`.
pub struct ReviewSessionData {
    pub checkout: PrCheckout,
    pub files: Vec<PrFile>,
    /// Best-effort — empty if the fetch failed; never blocks opening the
    /// review (mirrors the Desktop frontend's `code-review.ts` behavior).
    pub existing_comments: Vec<ExistingComment>,
}

/// Which pane has focus in the code review layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewFocus {
    FileList,
    DiffView,
}

/// What an in-progress comment edit will produce on save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentTarget {
    /// A new (or edited) inline comment of our own, added to the draft.
    NewInline,
    /// A reply to an existing review comment thread, keyed by its root
    /// comment id — submitted separately from the review itself.
    Reply { comment_id: u64 },
}

/// State for the inline comment editor
pub struct EditingComment {
    pub file_path: String,
    pub line: u32,
    pub side: String,
    pub body: String,
    pub body_cursor: usize,
    pub target: CommentTarget,
}

/// Persistent draft — survives overlay open/close, only cleared on submit or discard
pub struct ReviewDraft {
    pub verdict: ReviewVerdict,
    pub body: String,
    pub body_cursor: usize,
    pub comments: Vec<InlineComment>,
}

impl Default for ReviewDraft {
    fn default() -> Self {
        Self {
            verdict: ReviewVerdict::Comment,
            body: String::new(),
            body_cursor: 0,
            comments: Vec::new(),
        }
    }
}

impl ReviewDraft {
    /// Count comments for a specific file path.
    pub fn comment_count_for_file(&self, path: &str) -> usize {
        self.comments.iter().filter(|c| c.path == path).count()
    }

    /// Get the comment at a specific file path and line, if any.
    pub fn comment_at_line(&self, path: &str, line: u32) -> Option<&InlineComment> {
        self.comments
            .iter()
            .find(|c| c.path == path && c.line == line)
    }

    /// Get the comment at a specific file path, line, and side (LEFT/RIGHT).
    pub fn comment_at_line_and_side(
        &self,
        path: &str,
        line: u32,
        side: &str,
    ) -> Option<&InlineComment> {
        self.comments
            .iter()
            .find(|c| c.path == path && c.line == line && c.side == side)
    }
}

/// Full code review state for a workspace
pub struct CodeReviewState {
    pub pr_info: PrInfo,
    /// Checkout directory for this PR (an app-managed `review-checkouts`
    /// worktree, not necessarily `ws.path` of a user-created workspace).
    /// Own field rather than derived from the workspace so review actions
    /// never depend on the workspace's active branch/HEAD.
    pub review_dir: PathBuf,
    /// What to diff against, e.g. `"origin/main"`.
    pub base_spec: String,
    pub repo_nwo: String,
    /// Existing review comments from other reviewers (best-effort — a load
    /// failure doesn't block opening the review; see `action/review.rs`).
    pub existing_comments: Vec<ExistingComment>,
    /// Draft replies to existing threads, keyed by the root comment's id.
    /// Submitted individually after the review POST in `SubmitPrReview`.
    pub reply_drafts: HashMap<u64, String>,
    pub files: Vec<PrFile>,
    pub selected_file: usize,
    pub focus: ReviewFocus,
    /// Cached per-file diffs (path -> parsed diff)
    pub file_diffs: HashMap<String, ParsedDiff>,
    pub diff_scroll: usize,
    pub file_scroll: usize,
    pub cursor_line: usize,
    pub editing_comment: Option<EditingComment>,
    pub draft: ReviewDraft,
    /// Whether the submit overlay is visible
    pub show_submit: bool,
    /// Error message to display inside the submit overlay
    pub submit_error: Option<String>,
    pub loading: bool,
}

impl CodeReviewState {
    pub fn new(
        pr_info: PrInfo,
        review_dir: PathBuf,
        base_spec: String,
        repo_nwo: String,
        files: Vec<PrFile>,
    ) -> Self {
        Self {
            pr_info,
            review_dir,
            base_spec,
            repo_nwo,
            existing_comments: Vec::new(),
            reply_drafts: HashMap::new(),
            files,
            selected_file: 0,
            focus: ReviewFocus::FileList,
            file_diffs: HashMap::new(),
            diff_scroll: 0,
            file_scroll: 0,
            cursor_line: 0,
            editing_comment: None,
            draft: ReviewDraft::default(),
            show_submit: false,
            submit_error: None,
            loading: false,
        }
    }

    /// Get the currently selected file path
    pub fn current_file_path(&self) -> Option<&str> {
        self.files.get(self.selected_file).map(|f| f.path.as_str())
    }

    /// Get the cached parsed diff for the current file
    pub fn current_diff(&self) -> Option<&ParsedDiff> {
        self.current_file_path()
            .and_then(|p| self.file_diffs.get(p))
    }

    /// The root existing comment anchored at `(file_path, line, side)`, if
    /// any — the target for the `R` (reply) key. Outdated comments
    /// (`line == None`) are never surfaced here.
    pub fn thread_root_at(&self, file_path: &str, line: u32, side: &str) -> Option<&ExistingComment> {
        self.existing_comments.iter().find(|c| {
            c.path == file_path && c.line == Some(line) && c.side == side && c.in_reply_to_id.is_none()
        })
    }

    /// Rendered thread lines for `(file_path, line, side)`: existing
    /// comments (root + replies, outdated ones skipped) followed by our own
    /// draft inline comment, if any. Empty means no decoration is drawn.
    pub fn thread_lines(&self, file_path: &str, line: u32, side: &str) -> Vec<String> {
        let mut out = Vec::new();
        for root in self.existing_comments.iter().filter(|c| {
            c.path == file_path && c.line == Some(line) && c.side == side && c.in_reply_to_id.is_none()
        }) {
            out.push(format!("{}: {}", root.author, root.body));
            for reply in self
                .existing_comments
                .iter()
                .filter(|c| c.in_reply_to_id == Some(root.id))
            {
                out.push(format!("  \u{21b3} {}: {}", reply.author, reply.body));
            }
            if let Some(draft_reply) = self.reply_drafts.get(&root.id) {
                out.push(format!("  \u{21b3} you (pending): {}", draft_reply));
            }
        }
        if let Some(own) = self.draft.comment_at_line_and_side(file_path, line, side) {
            out.push(format!("you: {}", own.body));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piki_core::github::PrInfo;

    fn test_state() -> CodeReviewState {
        let pr_info = PrInfo {
            number: 1,
            title: "test".into(),
            body: String::new(),
            state: "OPEN".into(),
            review_decision: None,
            url: "https://github.com/o/r/pull/1".into(),
            head_ref_name: "feature".into(),
            base_ref_name: "main".into(),
            additions: 0,
            deletions: 0,
            review_requests: vec![],
            latest_reviews: vec![],
            head_ref_oid: "abc".into(),
        };
        CodeReviewState::new(
            pr_info,
            std::path::PathBuf::from("/tmp/review"),
            "origin/main".into(),
            "o/r".into(),
            vec![],
        )
    }

    fn existing(id: u64, line: u32, side: &str, author: &str, body: &str, in_reply_to: Option<u64>) -> ExistingComment {
        ExistingComment {
            id,
            path: "src/main.rs".into(),
            line: Some(line),
            original_line: Some(line),
            side: side.into(),
            body: body.into(),
            author: author.into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            in_reply_to_id: in_reply_to,
        }
    }

    #[test]
    fn thread_lines_empty_when_no_comments() {
        let state = test_state();
        assert!(state.thread_lines("src/main.rs", 10, "RIGHT").is_empty());
    }

    #[test]
    fn thread_lines_includes_root_and_replies_in_order() {
        let mut state = test_state();
        state.existing_comments = vec![
            existing(1, 10, "RIGHT", "octocat", "why not use X?", None),
            existing(2, 10, "RIGHT", "author", "good point", Some(1)),
        ];
        let lines = state.thread_lines("src/main.rs", 10, "RIGHT");
        assert_eq!(lines, vec![
            "octocat: why not use X?".to_string(),
            "  \u{21b3} author: good point".to_string(),
        ]);
    }

    #[test]
    fn thread_lines_shows_pending_reply_draft() {
        let mut state = test_state();
        state.existing_comments = vec![existing(1, 10, "RIGHT", "octocat", "why not use X?", None)];
        state.reply_drafts.insert(1, "because Y".to_string());
        let lines = state.thread_lines("src/main.rs", 10, "RIGHT");
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("you (pending): because Y"));
    }

    #[test]
    fn thread_lines_skips_outdated_comments() {
        let mut state = test_state();
        let mut outdated = existing(1, 10, "RIGHT", "octocat", "stale", None);
        outdated.line = None; // outdated anchor
        state.existing_comments = vec![outdated];
        assert!(state.thread_lines("src/main.rs", 10, "RIGHT").is_empty());
    }

    #[test]
    fn thread_root_at_ignores_replies() {
        let mut state = test_state();
        state.existing_comments = vec![
            existing(1, 10, "RIGHT", "octocat", "root", None),
            existing(2, 10, "RIGHT", "author", "reply", Some(1)),
        ];
        let root = state.thread_root_at("src/main.rs", 10, "RIGHT").unwrap();
        assert_eq!(root.id, 1);
    }

    #[test]
    fn thread_root_at_none_when_only_own_draft() {
        let mut state = test_state();
        state.draft.comments.push(piki_core::github::InlineComment {
            path: "src/main.rs".into(),
            line: 10,
            side: "RIGHT".into(),
            body: "mine".into(),
        });
        assert!(state.thread_root_at("src/main.rs", 10, "RIGHT").is_none());
        // But it still shows up in thread_lines.
        assert_eq!(state.thread_lines("src/main.rs", 10, "RIGHT"), vec!["you: mine".to_string()]);
    }
}
