use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::DefaultTerminal;

use crate::action::Action;
use crate::app::{self, ActivePane, ApiResponseDisplay, App, AppMode};
use crate::clipboard;
use crate::dialog_state::DialogState;
use crate::helpers::{rect_contains, resize_all_ptys, scrollback_max, subtab_index_at};

/// Encode a mouse scroll event as terminal escape bytes based on the protocol encoding.
/// `button` is 64 for scroll-up, 65 for scroll-down. `col`/`row` are 1-based PTY coordinates.
fn encode_mouse_scroll(
    button: u8,
    col: u16,
    row: u16,
    encoding: vt100::MouseProtocolEncoding,
) -> Vec<u8> {
    match encoding {
        vt100::MouseProtocolEncoding::Sgr => format!("\x1b[<{button};{col};{row}M").into_bytes(),
        vt100::MouseProtocolEncoding::Utf8 => {
            let mut buf = vec![b'\x1b', b'[', b'M', button + 32];
            // UTF-8 encode col and row (each + 32)
            let mut tmp = [0u8; 4];
            let ch_col = char::from_u32((col as u32) + 32).unwrap_or('!');
            let len = ch_col.encode_utf8(&mut tmp).len();
            buf.extend_from_slice(&tmp[..len]);
            let ch_row = char::from_u32((row as u32) + 32).unwrap_or('!');
            let len = ch_row.encode_utf8(&mut tmp).len();
            buf.extend_from_slice(&tmp[..len]);
            buf
        }
        // Default (X10-compatible) encoding
        _ => {
            vec![
                b'\x1b',
                b'[',
                b'M',
                button + 32,
                (col as u8).saturating_add(32),
                (row as u8).saturating_add(32),
            ]
        }
    }
}

/// Try to forward a scroll event to the PTY if the child is in alternate screen with mouse capture.
/// Returns `true` if the event was forwarded, `false` if normal scrollback handling should be used.
/// `button`: 64 = scroll up, 65 = scroll down.
fn try_forward_scroll_to_pty(app: &mut App, col: u16, row: u16, button: u8) -> bool {
    let inner = match app.terminal_inner_area {
        Some(r) => r,
        None => return false,
    };

    let ws = match app.workspaces.get_mut(app.active_workspace) {
        Some(ws) => ws,
        None => return false,
    };
    let tab = match ws.current_tab_mut() {
        Some(t) => t,
        None => return false,
    };
    let parser = match tab.pty_parser {
        Some(ref p) => Arc::clone(p),
        None => return false,
    };

    let guard = parser.lock();
    let screen = guard.screen();
    let alt = screen.alternate_screen();
    let mouse_mode = screen.mouse_protocol_mode();
    let mouse_enc = screen.mouse_protocol_encoding();
    drop(guard);

    if !alt || matches!(mouse_mode, vt100::MouseProtocolMode::None) {
        return false;
    }

    // Translate from outer terminal coords to 1-based PTY coords
    let pty_col = col.saturating_sub(inner.x) + 1;
    let pty_row = row.saturating_sub(inner.y) + 1;

    let bytes = encode_mouse_scroll(button, pty_col, pty_row, mouse_enc);

    if let Some(ref mut session) = tab.pty_session {
        let _ = session.write(&bytes);
    }
    true
}

/// Handle mouse events when code review is locked (full-screen mode).
/// Supports scrolling (moves cursor in diff view) and click to set cursor/focus.
fn handle_code_review_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) -> Option<Action> {
    let ws = app.workspaces.get_mut(app.active_workspace)?;
    let cr = ws.code_review.as_mut()?;

    let line_count = cr.current_diff().map(|d| d.lines.len()).unwrap_or(0);

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            match cr.focus {
                crate::code_review::ReviewFocus::FileList => {
                    if !cr.files.is_empty() {
                        cr.selected_file = (cr.selected_file + cr.files.len() - 1) % cr.files.len();
                        if cr.selected_file < cr.file_scroll {
                            cr.file_scroll = cr.selected_file;
                        }
                    }
                }
                crate::code_review::ReviewFocus::DiffView => {
                    if line_count > 0 {
                        cr.cursor_line = cr.cursor_line.saturating_sub(3);
                        cr.diff_scroll = cr.diff_scroll.saturating_sub(3);
                    }
                }
            }
            None
        }
        MouseEventKind::ScrollDown => {
            match cr.focus {
                crate::code_review::ReviewFocus::FileList => {
                    if !cr.files.is_empty() {
                        cr.selected_file = (cr.selected_file + 1) % cr.files.len();
                        let visible = 20usize;
                        if cr.selected_file >= cr.file_scroll + visible {
                            cr.file_scroll = cr.selected_file.saturating_sub(visible - 1);
                        }
                    }
                }
                crate::code_review::ReviewFocus::DiffView => {
                    if line_count > 0 {
                        cr.cursor_line = (cr.cursor_line + 3).min(line_count.saturating_sub(1));
                        cr.diff_scroll = cr.diff_scroll.saturating_add(3);
                    }
                }
            }
            None
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // Click on left 25% = file list, right 75% = diff view
            let total_width = app.main_content_area.width.max(1);
            let files_end = app.main_content_area.x + total_width / 4;
            if mouse.column < files_end {
                cr.focus = crate::code_review::ReviewFocus::FileList;
            } else {
                cr.focus = crate::code_review::ReviewFocus::DiffView;
                // Map click row to diff line index (approximate: row + scroll offset)
                // The header area is 2 lines + 1 border line from the diff block
                let diff_area_top = app.main_content_area.y + 3; // header(2) + border(1)
                if mouse.row > diff_area_top && line_count > 0 {
                    let clicked_visual_row =
                        (mouse.row - diff_area_top - 1) as usize + cr.diff_scroll;
                    // This is a visual row; for simplicity, map directly to diff line index
                    // (ignoring comment decorations for click targeting)
                    cr.cursor_line = clicked_visual_row.min(line_count.saturating_sub(1));
                }
            }
            None
        }
        _ => None,
    }
}

/// Handle all mouse events. Returns an Action if one needs async execution.
pub(crate) fn handle_mouse_event(
    app: &mut App,
    mouse: crossterm::event::MouseEvent,
    terminal: &mut DefaultTerminal,
) -> Option<Action> {
    // Code review locked mode — only allow scroll/click within the review
    if super::code_review_input::is_code_review_locked(app) {
        return handle_code_review_mouse(app, mouse);
    }

    let col = mouse.column;
    let row = mouse.row;

    match mouse.kind {
        MouseEventKind::ScrollUp => match app.mode {
            AppMode::Logs => {
                if let Some(DialogState::Logs {
                    ref mut selected, ..
                }) = app.active_dialog
                {
                    *selected = selected.saturating_sub(3);
                }
            }
            AppMode::Help => {
                if let Some(DialogState::Help { ref mut scroll }) = app.active_dialog {
                    *scroll = scroll.saturating_sub(3);
                }
            }
            AppMode::Diff => {
                app.diff_scroll = app.diff_scroll.saturating_sub(3);
            }
            AppMode::FuzzySearch => {
                if let Some(ref mut state) = app.fuzzy {
                    state.selected = state.selected.saturating_sub(1);
                }
            }
            AppMode::Normal | AppMode::InlineEdit => {
                let api_resp_area = app.api_response_inner_area;
                if rect_contains(app.ws_list_area, col, row) {
                    app.select_prev_sidebar_row();
                } else if rect_contains(app.file_list_area, col, row) {
                    app.prev_file();
                } else if rect_contains(app.main_content_area, col, row)
                    && !try_forward_scroll_to_pty(app, col, row, 64)
                    && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                    && let Some(tab) = ws.current_tab_mut()
                {
                    if tab.markdown_content.is_some() {
                        tab.markdown_scroll = tab.markdown_scroll.saturating_sub(3);
                    } else if let Some(ref mut api) = tab.api_state {
                        if api_resp_area.is_some_and(|r| row >= r.y.saturating_sub(1)) {
                            api.response_scroll = api.response_scroll.saturating_sub(3);
                        } else {
                            api.editor.scroll_offset = api.editor.scroll_offset.saturating_sub(1);
                        }
                    } else if let Some(ref parser) = tab.pty_parser {
                        let max = scrollback_max(parser);
                        tab.term_scroll = (tab.term_scroll + 3).min(max);
                    }
                }
            }
            _ => {}
        },
        MouseEventKind::ScrollDown => match app.mode {
            AppMode::Logs => {
                if let Some(DialogState::Logs {
                    ref mut selected, ..
                }) = app.active_dialog
                {
                    *selected = selected.saturating_add(3);
                }
            }
            AppMode::Help => {
                if let Some(DialogState::Help { ref mut scroll }) = app.active_dialog {
                    *scroll = scroll.saturating_add(3);
                }
            }
            AppMode::Diff => {
                app.diff_scroll = app.diff_scroll.saturating_add(3);
            }
            AppMode::FuzzySearch => {
                if let Some(ref mut state) = app.fuzzy {
                    let count = state.nucleo.snapshot().matched_item_count() as usize;
                    if count > 0 {
                        state.selected = (state.selected + 1).min(count - 1);
                    }
                }
            }
            AppMode::Normal | AppMode::InlineEdit => {
                let api_resp_area = app.api_response_inner_area;
                if rect_contains(app.ws_list_area, col, row) {
                    app.select_next_sidebar_row();
                } else if rect_contains(app.file_list_area, col, row) {
                    app.next_file();
                } else if rect_contains(app.main_content_area, col, row)
                    && !try_forward_scroll_to_pty(app, col, row, 65)
                    && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                    && let Some(tab) = ws.current_tab_mut()
                {
                    if tab.markdown_content.is_some() {
                        tab.markdown_scroll = tab.markdown_scroll.saturating_add(3);
                    } else if let Some(ref mut api) = tab.api_state {
                        if api_resp_area.is_some_and(|r| row >= r.y.saturating_sub(1)) {
                            api.response_scroll = api.response_scroll.saturating_add(3);
                        } else {
                            let max = api.editor.lines.len().saturating_sub(1);
                            api.editor.scroll_offset = (api.editor.scroll_offset + 1).min(max);
                        }
                    } else {
                        tab.term_scroll = tab.term_scroll.saturating_sub(3);
                    }
                }
            }
            _ => {}
        },
        MouseEventKind::Down(MouseButton::Left) => {
            // Detect double-click
            let now = Instant::now();
            let is_double_click = app.last_click.is_some_and(|(t, c, r)| {
                now.duration_since(t).as_millis() < 400 && c == col && r == row
            });
            app.last_click = Some((now, col, row));

            // Dismiss overlays on click (except Logs — handle click-to-select)
            match app.mode {
                AppMode::Logs => {
                    // Click to select a row within the logs popup
                    if let Some(DialogState::Logs {
                        ref mut scroll,
                        ref mut selected,
                        level_filter,
                        ..
                    }) = app.active_dialog
                    {
                        let term_size = terminal.size().unwrap_or_default();
                        let popup_width = (term_size.width * 90 / 100).max(40);
                        let popup_height = (term_size.height * 85 / 100).max(10);
                        let popup_x = (term_size.width.saturating_sub(popup_width)) / 2;
                        let popup_y = (term_size.height.saturating_sub(popup_height)) / 2;
                        let content_top = popup_y + 1;
                        let content_bottom = popup_y + popup_height.saturating_sub(2);
                        let inner_height = (content_bottom.saturating_sub(content_top)) as usize;
                        if row >= content_top
                            && row < content_bottom
                            && col >= popup_x
                            && col < popup_x + popup_width
                        {
                            // Compute total filtered entries for scroll resolution
                            let total = {
                                let buf = app.log_buffer.lock();
                                buf.iter()
                                    .filter(|e| {
                                        if level_filter == 0 {
                                            return true;
                                        }
                                        let n = match e.level {
                                            tracing::Level::ERROR => 1,
                                            tracing::Level::WARN => 2,
                                            tracing::Level::INFO => 3,
                                            tracing::Level::DEBUG => 4,
                                            tracing::Level::TRACE => 5,
                                        };
                                        n <= level_filter
                                    })
                                    .count()
                            };
                            let last = total.saturating_sub(1);
                            let max_scroll = total.saturating_sub(inner_height);
                            // Resolve current selected
                            let cur_sel = (*selected).min(last);
                            let scroll_val = if *scroll == u16::MAX {
                                if total <= inner_height {
                                    0
                                } else {
                                    cur_sel
                                        .saturating_sub(inner_height.saturating_sub(1))
                                        .min(max_scroll)
                                }
                            } else {
                                (*scroll as usize).min(max_scroll)
                            };
                            let clicked_row = (row - content_top) as usize;
                            let new_sel = (scroll_val + clicked_row).min(last);
                            *selected = new_sel;
                            *scroll = scroll_val as u16;
                        }
                    }
                    return None;
                }
                AppMode::Help => {
                    app.mode = AppMode::Normal;
                    return None;
                }
                AppMode::About => {
                    app.mode = AppMode::Normal;
                    return None;
                }
                AppMode::WorkspaceInfo => {
                    app.mode = AppMode::Normal;
                    return None;
                }
                _ => {}
            }

            // Check resize borders first
            let on_sidebar_border = col >= app.sidebar_x.saturating_sub(1)
                && col <= app.sidebar_x + 1
                && row < app.left_area_rect.y + app.left_area_rect.height + app.left_area_rect.y;
            let on_left_split_border = row >= app.left_split_y.saturating_sub(1)
                && row <= app.left_split_y
                && col < app.sidebar_x;

            if on_sidebar_border {
                app.resize_drag = Some(app::ResizeDrag::Sidebar);
            } else if on_left_split_border {
                app.resize_drag = Some(app::ResizeDrag::LeftSplit);
            } else if app.mode == AppMode::Normal {
                // Click on sub-tabs
                if rect_contains(app.subtabs_area, col, row) {
                    let subtabs_area = app.subtabs_area;
                    if let Some((idx, on_close)) = subtab_index_at(app, col, subtabs_area) {
                        if on_close {
                            if let Some(ws) = app.current_workspace()
                                && ws.tabs.get(idx).is_some_and(|t| t.closable)
                            {
                                app.active_dialog =
                                    Some(DialogState::ConfirmCloseTab { target: idx });
                                app.mode = AppMode::ConfirmCloseTab;
                            }
                        } else if let Some(ws) = app.current_workspace_mut() {
                            ws.active_tab = idx;
                        }
                    }
                }
                // Click on workspace list
                else if rect_contains(app.ws_list_area, col, row) {
                    app.active_pane = ActivePane::WorkspaceList;
                    let inner_y = app.ws_list_area.y + 1;
                    if row >= inner_y {
                        let relative_row = (row - inner_y) as usize;
                        let sidebar_items = app.sidebar_items();
                        let mut cumulative_height = 0;
                        for (i, item) in sidebar_items.iter().enumerate() {
                            let item_height = match item {
                                crate::app::SidebarItem::GroupHeader { .. } => 1,
                                crate::app::SidebarItem::Workspace { .. } => 3,
                            };
                            if relative_row < cumulative_height + item_height {
                                app.selected_sidebar_row = i;
                                match item {
                                    crate::app::SidebarItem::GroupHeader { .. } => {
                                        app.toggle_selected_group();
                                    }
                                    crate::app::SidebarItem::Workspace { index } => {
                                        app.selected_workspace = *index;
                                        app.switch_workspace(*index);
                                    }
                                }
                                break;
                            }
                            cumulative_height += item_height;
                        }
                    }
                }
                // Click on file list / services list
                else if rect_contains(app.file_list_area, col, row) {
                    app.active_pane = ActivePane::GitStatus;
                    let inner_y = app.file_list_area.y + 1;
                    if row >= inner_y {
                        let relative_row = (row - inner_y) as usize;
                        let is_project = app.current_workspace().is_some_and(|ws| {
                            ws.info.workspace_type == piki_core::WorkspaceType::Project
                        });
                        if is_project {
                            // Extract needed data before mutating app
                            let click_data = app.current_workspace().and_then(|ws| {
                                if relative_row < ws.sub_directories.len() {
                                    Some((
                                        ws.path
                                            .join(&ws.sub_directories[relative_row])
                                            .display()
                                            .to_string(),
                                        ws.prompt.clone(),
                                        ws.kanban_path.clone().unwrap_or_default(),
                                        ws.info.group.clone().unwrap_or_default(),
                                    ))
                                } else {
                                    None
                                }
                            });
                            if let Some((full_dir, prompt, kanban, group)) = click_data {
                                app.selected_file = relative_row;
                                if is_double_click {
                                    app.active_dialog = Some(DialogState::NewWorkspace {
                                        name: String::new(),
                                        name_cursor: 0,
                                        dir_cursor: full_dir.chars().count(),
                                        dir: full_dir,
                                        desc: String::new(),
                                        desc_cursor: 0,
                                        prompt_cursor: prompt.chars().count(),
                                        prompt,
                                        kanban_cursor: kanban.chars().count(),
                                        kanban,
                                        group_cursor: group.chars().count(),
                                        group,
                                        ws_type: piki_core::WorkspaceType::Simple,
                                        active_field: crate::app::DialogField::Type,
                                    });
                                    app.mode = AppMode::NewWorkspace;
                                }
                            }
                        } else if let Some(ws) = app.current_workspace()
                            && relative_row < ws.changed_files.len()
                        {
                            app.selected_file = relative_row;
                            // Double-click opens diff
                            if is_double_click {
                                return Some(Action::OpenDiff(relative_row));
                            }
                        }
                    }
                }
                // Click on main panel — start text selection
                else if rect_contains(app.main_content_area, col, row) {
                    app.active_pane = ActivePane::MainPanel;
                    app.interacting = true;
                    if let Some(inner) = app.terminal_inner_area
                        && rect_contains(inner, col, row)
                    {
                        let cell_row = row - inner.y;
                        let cell_col = col - inner.x;
                        app.selection = Some(app::Selection::new(cell_row, cell_col));
                    }
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(drag) = app.resize_drag {
                let total = terminal.size().unwrap_or_default();
                match drag {
                    app::ResizeDrag::Sidebar => {
                        let pct = ((col as u32) * 100 / total.width.max(1) as u32) as u16;
                        app.sidebar_pct = pct.clamp(10, 90);
                        resize_all_ptys(app);
                    }
                    app::ResizeDrag::LeftSplit => {
                        let left_top = app.left_area_rect.y;
                        let left_height = app.left_area_rect.height;
                        if left_height > 0 {
                            let rel = row.saturating_sub(left_top) as u32;
                            let pct = (rel * 100 / left_height as u32) as u16;
                            app.left_split_pct = pct.clamp(10, 90);
                        }
                    }
                }
            } else if let Some(ref mut sel) = app.selection
                && let Some(inner) = app.terminal_inner_area
            {
                let cell_row = row
                    .saturating_sub(inner.y)
                    .min(inner.height.saturating_sub(1));
                let cell_col = col
                    .saturating_sub(inner.x)
                    .min(inner.width.saturating_sub(1));
                sel.end_row = cell_row;
                sel.end_col = cell_col;
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if app.resize_drag.is_some() {
                app.resize_drag = None;
                app.save_layout_prefs();
            } else if let Some(ref mut sel) = app.selection {
                sel.active = false;
                let (sr, sc, er, ec) = sel.normalized();
                if sr != er || sc != ec {
                    let copied = if let Some(ws) = app.workspaces.get(app.active_workspace)
                        && let Some(tab) = ws.current_tab()
                        && let Some(ref parser) = tab.pty_parser
                    {
                        let mut guard = parser.lock();
                        guard.screen_mut().set_scrollback(tab.term_scroll);
                        let text = guard.screen().contents_between(sr, sc, er, ec + 1);
                        guard.screen_mut().set_scrollback(0);
                        Some(text)
                    } else if let Some(ws) = app.workspaces.get(app.active_workspace)
                        && let Some(tab) = ws.current_tab()
                        && let Some(ref api) = tab.api_state
                        && app.api_response_inner_area.is_some()
                    {
                        let lines = build_response_text_lines(&api.responses);
                        let scroll = api.response_scroll as usize;
                        let text = extract_text_from_lines(&lines, sr, sc, er, ec, scroll);
                        if text.is_empty() { None } else { Some(text) }
                    } else {
                        None
                    };

                    if let Some(text) = copied {
                        if let Err(e) = clipboard::copy_to_clipboard(&text) {
                            app.status_message = Some(format!("Copy failed: {}", e));
                        } else {
                            app.status_message = Some("Selection copied".into());
                        }
                    }
                }
            }
        }
        _ => {}
    }
    None
}

/// Build plain-text lines matching the render layout of API responses.
/// Each response gets a header line, body lines, and a separator between responses.
fn build_response_text_lines(responses: &[ApiResponseDisplay]) -> Vec<String> {
    let mut lines = Vec::new();
    let total = responses.len();
    for (idx, resp) in responses.iter().enumerate() {
        // Header line (matches render_responses layout)
        let header = if resp.status == 0 {
            format!("── Response #{} (error) — {}ms ", idx + 1, resp.elapsed_ms)
        } else if total == 1 {
            format!("── {} — {}ms ", resp.status, resp.elapsed_ms)
        } else {
            format!("── #{} ({}) — {}ms ", idx + 1, resp.status, resp.elapsed_ms)
        };
        lines.push(header);

        // Body lines
        for line in resp.body.lines() {
            lines.push(line.to_string());
        }

        // Separator between responses
        if idx + 1 < total {
            lines.push(String::new());
        }
    }
    lines
}

/// Extract text from rendered lines between selection coordinates, accounting for scroll.
fn extract_text_from_lines(
    lines: &[String],
    start_row: u16,
    start_col: u16,
    end_row: u16,
    end_col: u16,
    scroll: usize,
) -> String {
    let sr = start_row as usize + scroll;
    let er = end_row as usize + scroll;
    let sc = start_col as usize;
    let ec = end_col as usize;

    let mut result = String::new();
    for row in sr..=er {
        if row >= lines.len() {
            break;
        }
        let line = &lines[row];
        let chars: Vec<char> = line.chars().collect();
        let col_start = if row == sr { sc } else { 0 };
        let col_end = if row == er {
            ec.min(chars.len().saturating_sub(1))
        } else {
            chars.len().saturating_sub(1)
        };
        if col_start <= col_end && col_start < chars.len() {
            let end = (col_end + 1).min(chars.len());
            let slice: String = chars[col_start..end].iter().collect();
            result.push_str(&slice);
        }
        if row < er {
            result.push('\n');
        }
    }
    result
}
