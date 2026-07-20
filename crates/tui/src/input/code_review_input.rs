use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{App, AppMode};
use crate::code_review::{CommentTarget, EditingComment, ReviewFocus};
use crate::config::has_ctrl;
use piki_core::github::{DiffLineType, InlineComment, ReviewVerdict};

/// Check if the active tab is a CodeReview tab with loaded state and the
/// main panel has focus. Used to intercept ALL input before any
/// navigation/interaction dispatch — gated on focus (not just
/// `active_workspace`) so browsing the sidebar with j/k past a review
/// workspace's row never hijacks the keyboard; see `ui::layout::is_code_review_active`.
pub(super) fn is_code_review_locked(app: &App) -> bool {
    app.active_pane == crate::app::ActivePane::MainPanel
        && app
            .current_workspace()
            .and_then(|ws| ws.current_tab())
            .is_some_and(|tab| tab.provider == piki_core::AIProvider::CodeReview)
        && app
            .current_workspace()
            .and_then(|ws| ws.code_review.as_ref())
            .is_some()
}

/// Top-level key handler when code review is locked.
/// ALL keys route here — nothing leaks to the prefix dispatcher.
pub(super) fn handle_code_review_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    // If editing a comment, route there first
    if app
        .current_workspace()
        .and_then(|ws| ws.code_review.as_ref())
        .is_some_and(|cr| cr.editing_comment.is_some())
    {
        return handle_comment_editing_input(app, key);
    }

    // If submit overlay is open, route there
    if app
        .current_workspace()
        .and_then(|ws| ws.code_review.as_ref())
        .is_some_and(|cr| cr.show_submit)
    {
        return handle_submit_review_input(app, key);
    }

    // If the discard-and-delete confirmation is open, route there
    if app
        .current_workspace()
        .and_then(|ws| ws.code_review.as_ref())
        .is_some_and(|cr| cr.confirm_close)
    {
        return handle_review_confirm_close_input(app, key);
    }

    // q → close code review. For an ephemeral review workspace this deletes
    // its checkout from disk, so ask first instead of doing it silently.
    if key.code == KeyCode::Char('q') {
        let is_ephemeral = app
            .workspaces
            .get(app.active_workspace)
            .is_some_and(|ws| ws.info.ephemeral);
        if is_ephemeral {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                && let Some(cr) = ws.code_review.as_mut()
            {
                cr.confirm_close = true;
            }
            return None;
        }
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
            ws.code_review = None;
            if ws
                .current_tab()
                .is_some_and(|t| t.provider == piki_core::AIProvider::CodeReview)
            {
                ws.close_tab(ws.active_tab);
            }
        }
        app.mode = AppMode::Normal;
        return None;
    }

    // [ / ] resize the file-list | diff split
    if key.code == KeyCode::Char('[') {
        app.code_review_split_pct = app.code_review_split_pct.saturating_sub(5).max(10);
        app.save_layout_prefs();
        return None;
    }
    if key.code == KeyCode::Char(']') {
        app.code_review_split_pct = (app.code_review_split_pct + 5).min(90);
        app.save_layout_prefs();
        return None;
    }

    // Everything else goes to the code review interaction handler
    let ws = app.workspaces.get_mut(app.active_workspace)?;
    let cr = ws.code_review.as_mut()?;

    let platform = app.config.platform;
    let result = match cr.focus {
        ReviewFocus::FileList => handle_file_list_keys(cr, key),
        ReviewFocus::DiffView => handle_diff_view_keys(cr, key, platform),
    };

    // If show_submit was toggled on, switch to SubmitReview mode
    if let Some(ws) = app.workspaces.get(app.active_workspace)
        && let Some(cr) = &ws.code_review
        && cr.show_submit
    {
        app.mode = AppMode::SubmitReview;
    }

    result
}

/// Handle input when the "discard and delete checkout" confirmation is
/// showing (only reachable for ephemeral review workspaces — see `q` above).
pub(super) fn handle_review_confirm_close_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            // DeleteWorkspace's ephemeral branch always removes the
            // checkout directory from disk — same path the sidebar's own
            // delete-workspace confirmation uses.
            app.mode = AppMode::Normal;
            Some(Action::DeleteWorkspace(app.active_workspace, None))
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                && let Some(cr) = ws.code_review.as_mut()
            {
                cr.confirm_close = false;
            }
            None
        }
        _ => None,
    }
}

/// Handle input when the submit review overlay is visible
pub(super) fn handle_submit_review_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let ws = app.workspaces.get_mut(app.active_workspace)?;
    let cr = ws.code_review.as_mut()?;

    // Clear any previous submit error on interaction
    cr.submit_error = None;

    match key.code {
        KeyCode::Tab => {
            cr.draft.verdict = cr.draft.verdict.next();
            None
        }
        KeyCode::Enter => Some(Action::SubmitPrReview),
        KeyCode::Esc => {
            cr.show_submit = false;
            app.mode = AppMode::Normal;
            app.active_dialog = None;
            None
        }
        KeyCode::Char('d') if has_ctrl(key.modifiers, app.config.platform) => {
            // Discard draft (including comments)
            cr.draft.verdict = ReviewVerdict::Comment;
            cr.draft.body.clear();
            cr.draft.body_cursor = 0;
            cr.draft.comments.clear();
            cr.show_submit = false;
            app.mode = AppMode::Normal;
            app.active_dialog = None;
            None
        }
        KeyCode::Char(c) => {
            let cursor = cr.draft.body_cursor.min(cr.draft.body.len());
            cr.draft.body.insert(cursor, c);
            cr.draft.body_cursor = cursor + 1;
            None
        }
        KeyCode::Backspace => {
            if cr.draft.body_cursor > 0 {
                let cursor = cr.draft.body_cursor.min(cr.draft.body.len());
                cr.draft.body.remove(cursor - 1);
                cr.draft.body_cursor = cursor - 1;
            }
            None
        }
        KeyCode::Left => {
            if cr.draft.body_cursor > 0 {
                cr.draft.body_cursor -= 1;
            }
            None
        }
        KeyCode::Right => {
            if cr.draft.body_cursor < cr.draft.body.len() {
                cr.draft.body_cursor += 1;
            }
            None
        }
        KeyCode::Home => {
            cr.draft.body_cursor = 0;
            None
        }
        KeyCode::End => {
            cr.draft.body_cursor = cr.draft.body.len();
            None
        }
        _ => None,
    }
}

/// Handle input when editing an inline comment
fn handle_comment_editing_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let ws = app.workspaces.get_mut(app.active_workspace)?;
    let cr = ws.code_review.as_mut()?;
    let ec = cr.editing_comment.as_mut()?;

    match key.code {
        KeyCode::Enter => {
            match ec.target {
                CommentTarget::NewInline => {
                    let comment = InlineComment {
                        path: ec.file_path.clone(),
                        line: ec.line,
                        side: ec.side.clone(),
                        body: ec.body.clone(),
                    };
                    // Remove existing comment at same path+line (dedup)
                    cr.draft
                        .comments
                        .retain(|c| !(c.path == comment.path && c.line == comment.line));
                    if !comment.body.is_empty() {
                        cr.draft.comments.push(comment);
                    }
                }
                CommentTarget::Reply { comment_id } => {
                    if ec.body.is_empty() {
                        cr.reply_drafts.remove(&comment_id);
                    } else {
                        cr.reply_drafts.insert(comment_id, ec.body.clone());
                    }
                }
            }
            cr.editing_comment = None;
            None
        }
        KeyCode::Esc => {
            cr.editing_comment = None;
            None
        }
        KeyCode::Char(c) => {
            let cursor = ec.body_cursor.min(ec.body.len());
            ec.body.insert(cursor, c);
            ec.body_cursor = cursor + 1;
            None
        }
        KeyCode::Backspace => {
            if ec.body_cursor > 0 {
                let cursor = ec.body_cursor.min(ec.body.len());
                ec.body.remove(cursor - 1);
                ec.body_cursor = cursor - 1;
            }
            None
        }
        KeyCode::Left => {
            if ec.body_cursor > 0 {
                ec.body_cursor -= 1;
            }
            None
        }
        KeyCode::Right => {
            if ec.body_cursor < ec.body.len() {
                ec.body_cursor += 1;
            }
            None
        }
        KeyCode::Home => {
            ec.body_cursor = 0;
            None
        }
        KeyCode::End => {
            ec.body_cursor = ec.body.len();
            None
        }
        _ => None,
    }
}

fn handle_file_list_keys(
    cr: &mut crate::code_review::CodeReviewState,
    key: KeyEvent,
) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if !cr.files.is_empty() {
                cr.selected_file = (cr.selected_file + 1) % cr.files.len();
                let visible = 20usize;
                if cr.selected_file >= cr.file_scroll + visible {
                    cr.file_scroll = cr.selected_file.saturating_sub(visible - 1);
                }
                if cr.selected_file < cr.file_scroll {
                    cr.file_scroll = cr.selected_file;
                }
            }
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if !cr.files.is_empty() {
                cr.selected_file = (cr.selected_file + cr.files.len() - 1) % cr.files.len();
                if cr.selected_file < cr.file_scroll {
                    cr.file_scroll = cr.selected_file;
                }
            }
            None
        }
        KeyCode::Enter => {
            let idx = cr.selected_file;
            cr.focus = ReviewFocus::DiffView;
            cr.diff_scroll = 0;
            cr.cursor_line = 0;
            Some(Action::LoadPrFileDiff(idx))
        }
        KeyCode::Char('l') | KeyCode::Right => {
            cr.focus = ReviewFocus::DiffView;
            None
        }
        KeyCode::Char('s') => {
            cr.show_submit = true;
            None
        }
        KeyCode::Char('r') => Some(Action::LoadPrReview),
        _ => None,
    }
}

fn handle_diff_view_keys(
    cr: &mut crate::code_review::CodeReviewState,
    key: KeyEvent,
    platform: crate::config::Platform,
) -> Option<Action> {
    let line_count = cr.current_diff().map(|d| d.lines.len()).unwrap_or(0);

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if line_count > 0 {
                cr.cursor_line = (cr.cursor_line + 1).min(line_count.saturating_sub(1));
                ensure_cursor_visible(cr);
            }
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            cr.cursor_line = cr.cursor_line.saturating_sub(1);
            ensure_cursor_visible(cr);
            None
        }
        KeyCode::Char('d') if has_ctrl(key.modifiers, platform) => {
            if line_count > 0 {
                cr.cursor_line = (cr.cursor_line + 20).min(line_count.saturating_sub(1));
                ensure_cursor_visible(cr);
            }
            None
        }
        KeyCode::Char('u') if has_ctrl(key.modifiers, platform) => {
            cr.cursor_line = cr.cursor_line.saturating_sub(20);
            ensure_cursor_visible(cr);
            None
        }
        KeyCode::Char('g') => {
            cr.cursor_line = 0;
            cr.diff_scroll = 0;
            None
        }
        KeyCode::Char('G') => {
            if line_count > 0 {
                cr.cursor_line = line_count.saturating_sub(1);
                ensure_cursor_visible(cr);
            }
            None
        }
        KeyCode::Char('h') | KeyCode::Left => {
            cr.focus = ReviewFocus::FileList;
            None
        }
        KeyCode::Char('c') => {
            // Open comment editor on current cursor line
            if let Some(diff) = cr.current_diff()
                && let Some(diff_line) = diff.lines.get(cr.cursor_line)
            {
                match diff_line.line_type {
                    DiffLineType::Addition | DiffLineType::Context => {
                        if let Some(ln) = diff_line.new_line {
                            let file_path = cr.current_file_path().unwrap_or("").to_string();
                            let existing_body = cr
                                .draft
                                .comment_at_line(&file_path, ln)
                                .map(|c| c.body.clone())
                                .unwrap_or_default();
                            let cursor = existing_body.len();
                            cr.editing_comment = Some(EditingComment {
                                file_path,
                                line: ln,
                                side: "RIGHT".to_string(),
                                body: existing_body,
                                body_cursor: cursor,
                                target: CommentTarget::NewInline,
                            });
                        }
                    }
                    DiffLineType::Deletion => {
                        if let Some(ln) = diff_line.old_line {
                            let file_path = cr.current_file_path().unwrap_or("").to_string();
                            let existing_body = cr
                                .draft
                                .comment_at_line(&file_path, ln)
                                .map(|c| c.body.clone())
                                .unwrap_or_default();
                            let cursor = existing_body.len();
                            cr.editing_comment = Some(EditingComment {
                                file_path,
                                line: ln,
                                side: "LEFT".to_string(),
                                body: existing_body,
                                body_cursor: cursor,
                                target: CommentTarget::NewInline,
                            });
                        }
                    }
                    _ => {
                        // Can't comment on headers
                    }
                }
            }
            None
        }
        KeyCode::Char('R') => {
            // Reply to the existing comment thread anchored at the cursor
            // line, if any — no-op when the line has no thread.
            if let Some(diff) = cr.current_diff()
                && let Some(diff_line) = diff.lines.get(cr.cursor_line)
            {
                let (ln, side) = match diff_line.line_type {
                    DiffLineType::Deletion => (diff_line.old_line, "LEFT"),
                    DiffLineType::Addition | DiffLineType::Context => {
                        (diff_line.new_line, "RIGHT")
                    }
                    _ => (None, ""),
                };
                if let Some(ln) = ln
                    && let Some(root) = cr.thread_root_at(cr.current_file_path().unwrap_or(""), ln, side)
                {
                    let comment_id = root.id;
                    let file_path = cr.current_file_path().unwrap_or("").to_string();
                    let existing_body = cr.reply_drafts.get(&comment_id).cloned().unwrap_or_default();
                    let cursor = existing_body.len();
                    cr.editing_comment = Some(EditingComment {
                        file_path,
                        line: ln,
                        side: side.to_string(),
                        body: existing_body,
                        body_cursor: cursor,
                        target: CommentTarget::Reply { comment_id },
                    });
                }
            }
            None
        }
        KeyCode::Char('d') => {
            // Delete comment on current cursor line
            if let Some(diff) = cr.current_diff().cloned()
                && let Some(diff_line) = diff.lines.get(cr.cursor_line)
            {
                let file_path = cr.current_file_path().unwrap_or("").to_string();
                let target_line = match diff_line.line_type {
                    DiffLineType::Deletion => diff_line.old_line,
                    _ => diff_line.new_line,
                };
                if let Some(ln) = target_line {
                    cr.draft
                        .comments
                        .retain(|c| !(c.path == file_path && c.line == ln));
                }
            }
            None
        }
        KeyCode::Char('n') => {
            if !cr.files.is_empty() {
                cr.selected_file = (cr.selected_file + 1) % cr.files.len();
                cr.diff_scroll = 0;
                cr.cursor_line = 0;
                return Some(Action::LoadPrFileDiff(cr.selected_file));
            }
            None
        }
        KeyCode::Char('p') => {
            if !cr.files.is_empty() {
                cr.selected_file = (cr.selected_file + cr.files.len() - 1) % cr.files.len();
                cr.diff_scroll = 0;
                cr.cursor_line = 0;
                return Some(Action::LoadPrFileDiff(cr.selected_file));
            }
            None
        }
        KeyCode::Char('s') => {
            cr.show_submit = true;
            None
        }
        _ => None,
    }
}

/// Adjust diff_scroll to keep cursor_line visible in an estimated viewport.
/// We use a conservative viewport height of 30 lines; the actual height
/// is handled during rendering, but this keeps scroll roughly correct.
fn ensure_cursor_visible(cr: &mut crate::code_review::CodeReviewState) {
    // We need to account for comment decoration lines when computing the visual position.
    // For simplicity, we compute the visual row of the cursor line.
    let visual_row = compute_visual_row_for_cursor(cr);
    let viewport = 30usize; // conservative estimate

    if visual_row < cr.diff_scroll {
        cr.diff_scroll = visual_row;
    } else if visual_row >= cr.diff_scroll + viewport {
        cr.diff_scroll = visual_row.saturating_sub(viewport - 1);
    }
}

/// Compute the visual row index for the current cursor_line,
/// accounting for comment decoration lines and split-row pairing.
fn compute_visual_row_for_cursor(cr: &crate::code_review::CodeReviewState) -> usize {
    let diff = match cr.current_diff() {
        Some(d) => d,
        None => return cr.cursor_line,
    };
    let file_path = cr.current_file_path().unwrap_or("");
    let split_rows = crate::ui::code_review::compute_split_rows(diff, cr, file_path);
    for (row_idx, srow) in split_rows.iter().enumerate() {
        if srow.contains_diff_idx(cr.cursor_line) {
            return row_idx;
        }
    }
    // Fallback: cursor beyond all rows
    split_rows.len().saturating_sub(1)
}

#[cfg(test)]
mod confirm_close_tests {
    use super::*;
    use crate::app::{ActivePane, Workspace};
    use crate::code_review::CodeReviewState;
    use crate::test_support::test_app;
    use piki_core::github::PrInfo;

    fn pr_info() -> PrInfo {
        PrInfo {
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
        }
    }

    /// Push a workspace with an already-open CodeReview tab, focused on the
    /// main panel (the only way `is_code_review_locked` engages).
    fn push_review_workspace(app: &mut App, ephemeral: bool) {
        let mut info = piki_core::WorkspaceInfo::new(
            "o/r#1".to_string(),
            String::new(),
            String::new(),
            None,
            std::path::PathBuf::from("/tmp/review"),
            std::path::PathBuf::from("/tmp/review"),
        );
        info.ephemeral = ephemeral;
        let mut ws = Workspace::from_info(info);
        let tab_idx = ws.add_tab(piki_core::AIProvider::CodeReview, true, None);
        ws.active_tab = tab_idx;
        ws.code_review = Some(CodeReviewState::new(
            pr_info(),
            std::path::PathBuf::from("/tmp/review"),
            "origin/main".into(),
            "o/r".into(),
            vec![],
        ));
        app.workspaces.push(ws);
        app.active_workspace = app.workspaces.len() - 1;
        app.active_pane = ActivePane::MainPanel;
    }

    #[test]
    fn q_on_ephemeral_opens_confirm_instead_of_closing() {
        let mut app = test_app();
        push_review_workspace(&mut app, true);

        let action = handle_code_review_key(&mut app, crate::test_support::key(KeyCode::Char('q')));

        assert!(action.is_none());
        let cr = app.workspaces[0].code_review.as_ref().unwrap();
        assert!(cr.confirm_close, "q on an ephemeral review must ask before deleting");
        // Tab must still be there — nothing was closed yet.
        assert_eq!(app.workspaces[0].tabs.len(), 1);
    }

    #[test]
    fn q_on_non_ephemeral_closes_immediately() {
        let mut app = test_app();
        push_review_workspace(&mut app, false);

        let action = handle_code_review_key(&mut app, crate::test_support::key(KeyCode::Char('q')));

        assert!(action.is_none());
        assert!(app.workspaces[0].code_review.is_none());
        assert_eq!(app.workspaces[0].tabs.len(), 0);
    }

    #[test]
    fn confirm_close_yes_emits_delete_workspace() {
        let mut app = test_app();
        push_review_workspace(&mut app, true);
        app.workspaces[0].code_review.as_mut().unwrap().confirm_close = true;

        let action = handle_review_confirm_close_input(&mut app, crate::test_support::key(KeyCode::Char('y')));

        assert!(matches!(action, Some(Action::DeleteWorkspace(0, None))));
    }

    #[test]
    fn confirm_close_no_cancels_without_deleting() {
        let mut app = test_app();
        push_review_workspace(&mut app, true);
        app.workspaces[0].code_review.as_mut().unwrap().confirm_close = true;

        let action = handle_review_confirm_close_input(&mut app, crate::test_support::key(KeyCode::Char('n')));

        assert!(action.is_none());
        assert!(!app.workspaces[0].code_review.as_ref().unwrap().confirm_close);
        assert_eq!(app.workspaces.len(), 1, "workspace must survive a cancel");
    }
}
