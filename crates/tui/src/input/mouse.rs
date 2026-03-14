use std::time::Instant;

use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::DefaultTerminal;

use crate::action::Action;
use crate::app::{self, ActivePane, App, AppMode};
use crate::clipboard;
use crate::dialog_state::DialogState;
use crate::helpers::{
    rect_contains, resize_all_ptys, scrollback_max, subtab_index_at,
};

/// Handle all mouse events. Returns an Action if one needs async execution.
pub(crate) fn handle_mouse_event(
    app: &mut App,
    mouse: crossterm::event::MouseEvent,
    terminal: &mut DefaultTerminal,
) -> Option<Action> {
    let col = mouse.column;
    let row = mouse.row;

    match mouse.kind {
        MouseEventKind::ScrollUp => match app.mode {
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
                if rect_contains(app.ws_list_area, col, row) {
                    app.select_prev_sidebar_row();
                } else if rect_contains(app.file_list_area, col, row) {
                    app.prev_file();
                } else if rect_contains(app.main_content_area, col, row)
                    && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                    && let Some(tab) = ws.current_tab_mut()
                {
                    if tab.markdown_content.is_some() {
                        tab.markdown_scroll = tab.markdown_scroll.saturating_sub(3);
                    } else if let Some(ref parser) = tab.pty_parser {
                        let max = scrollback_max(parser);
                        tab.term_scroll = (tab.term_scroll + 3).min(max);
                    }
                }
            }
            _ => {}
        },
        MouseEventKind::ScrollDown => match app.mode {
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
                if rect_contains(app.ws_list_area, col, row) {
                    app.select_next_sidebar_row();
                } else if rect_contains(app.file_list_area, col, row) {
                    app.next_file();
                } else if rect_contains(app.main_content_area, col, row)
                    && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                    && let Some(tab) = ws.current_tab_mut()
                {
                    if tab.markdown_content.is_some() {
                        tab.markdown_scroll = tab.markdown_scroll.saturating_add(3);
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

            // Dismiss overlays on click
            match app.mode {
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
                // Click on file list
                else if rect_contains(app.file_list_area, col, row) {
                    app.active_pane = ActivePane::GitStatus;
                    let inner_y = app.file_list_area.y + 1;
                    if row >= inner_y {
                        let relative_row = (row - inner_y) as usize;
                        if let Some(ws) = app.current_workspace()
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
            } else if let Some(ref mut sel) = app.selection {
                sel.active = false;
                let (sr, sc, er, ec) = sel.normalized();
                if (sr != er || sc != ec)
                    && let Some(ws) = app.workspaces.get(app.active_workspace)
                    && let Some(tab) = ws.current_tab()
                    && let Some(ref parser) = tab.pty_parser
                {
                    let mut guard = parser.lock();
                    guard.screen_mut().set_scrollback(tab.term_scroll);
                    let text = guard.screen().contents_between(sr, sc, er, ec + 1);
                    guard.screen_mut().set_scrollback(0);
                    if let Err(e) = clipboard::copy_to_clipboard(&text) {
                        app.status_message = Some(format!("Copy failed: {}", e));
                    } else {
                        app.status_message = Some("Selection copied".into());
                    }
                }
            }
        }
        _ => {}
    }
    None
}
