use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::{App, AppMode};
use crate::code_review::{EditingComment, ReviewFocus};
use piki_core::github::{DiffLineType, InlineComment, ReviewVerdict};

/// Check if the active tab is a CodeReview tab with loaded state.
/// Used to intercept ALL input before any navigation/interaction dispatch.
pub(super) fn is_code_review_locked(app: &App) -> bool {
    app.current_workspace()
        .and_then(|ws| ws.current_tab())
        .is_some_and(|tab| tab.provider == piki_core::AIProvider::CodeReview)
        && app
            .current_workspace()
            .and_then(|ws| ws.code_review.as_ref())
            .is_some()
}

/// Top-level key handler when code review is locked.
/// ALL keys route here — nothing leaks to navigation mode.
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

    // q → close code review tab (discard review state)
    if key.code == KeyCode::Char('q') {
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
        app.interacting = false;
        return None;
    }

    // Everything else goes to the code review interaction handler
    let ws = app.workspaces.get_mut(app.active_workspace)?;
    let cr = ws.code_review.as_mut()?;

    let result = match cr.focus {
        ReviewFocus::FileList => handle_file_list_keys(cr, key),
        ReviewFocus::DiffView => handle_diff_view_keys(cr, key),
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
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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
            // Save comment
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
                cr.selected_file =
                    (cr.selected_file + cr.files.len() - 1) % cr.files.len();
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
) -> Option<Action> {
    let line_count = cr
        .current_diff()
        .map(|d| d.lines.len())
        .unwrap_or(0);

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
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if line_count > 0 {
                cr.cursor_line = (cr.cursor_line + 20).min(line_count.saturating_sub(1));
                ensure_cursor_visible(cr);
            }
            None
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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
                            let file_path =
                                cr.current_file_path().unwrap_or("").to_string();
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
                            });
                        }
                    }
                    DiffLineType::Deletion => {
                        if let Some(ln) = diff_line.old_line {
                            let file_path =
                                cr.current_file_path().unwrap_or("").to_string();
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
                cr.selected_file =
                    (cr.selected_file + cr.files.len() - 1) % cr.files.len();
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
    let split_rows =
        crate::ui::code_review::compute_split_rows(diff, &cr.draft, file_path);
    for (row_idx, srow) in split_rows.iter().enumerate() {
        if srow.contains_diff_idx(cr.cursor_line) {
            return row_idx;
        }
    }
    // Fallback: cursor beyond all rows
    split_rows.len().saturating_sub(1)
}
