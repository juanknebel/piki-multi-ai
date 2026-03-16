use std::collections::HashMap;

use piki_core::github::{InlineComment, ParsedDiff, PrFile, PrInfo, ReviewVerdict};

/// Which pane has focus in the code review layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewFocus {
    FileList,
    DiffView,
}

/// State for the inline comment editor
pub struct EditingComment {
    pub file_path: String,
    pub line: u32,
    pub side: String,
    pub body: String,
    pub body_cursor: usize,
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
}

/// Full code review state for a workspace
pub struct CodeReviewState {
    pub pr_info: PrInfo,
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
    pub fn new(pr_info: PrInfo, files: Vec<PrFile>) -> Self {
        Self {
            pr_info,
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
}
