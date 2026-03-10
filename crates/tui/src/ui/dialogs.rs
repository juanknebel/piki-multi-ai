use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, DialogField};

/// Helper to create a centered rect with fixed width (chars) and height (lines)
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Clear background for a centered popup, returning the popup Rect.
fn clear_popup(frame: &mut Frame, area: Rect, width: u16, height: u16) -> Rect {
    let popup = centered_rect(width, height, area);
    frame.render_widget(ratatui::widgets::Clear, popup);
    popup
}

/// Build a standard bordered block for popups.
fn popup_block(title: &str, border_color: Color) -> Block<'static> {
    Block::default()
        .title(format!(" {} ", title))
        .title_style(Style::default().fg(border_color))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
}

/// Auto-scrolling text field display. Shows cursor block when active.
fn visible_field(text: &str, active: bool, cursor: usize, field_max: usize) -> String {
    if !active {
        if text.len() > field_max && field_max > 2 {
            return format!("…{}", &text[text.len() - (field_max - 1)..]);
        }
        return text.to_string();
    }
    let before: String = text.chars().take(cursor).collect();
    let after: String = text.chars().skip(cursor).collect();
    let full = format!("{}█{}", before, after);
    if full.chars().count() > field_max && field_max > 2 {
        let chars: Vec<char> = full.chars().collect();
        let cursor_display = before.chars().count();
        let start = (cursor_display + 2).saturating_sub(field_max);
        let visible: String = chars[start..chars.len().min(start + field_max - 1)]
            .iter()
            .collect();
        format!("…{}", visible)
    } else {
        full
    }
}

/// Generic Y/N confirmation dialog.
fn render_yn_dialog(frame: &mut Frame, area: Rect, title: &str, message: &str, border_color: Color, hint_color: Color) {
    let popup = clear_popup(frame, area, 40, 7);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, Style::default().fg(Color::White))).centered(),
        Line::from(""),
        Line::from(Span::styled("[Y] Yes    [N] No", Style::default().fg(hint_color))).centered(),
    ];
    let text = Paragraph::new(lines).block(popup_block(title, border_color));
    frame.render_widget(text, popup);
}

pub(super) fn render_diff_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width * 90 / 100;
    let height = area.height * 85 / 100;
    let popup = clear_popup(frame, area, width, height);

    let file_path = app.diff_file_path.as_deref().unwrap_or("?");
    let border_style = Style::default().fg(app.theme.diff.border);
    super::diff::render(
        frame,
        popup,
        &app.diff_content,
        app.diff_scroll,
        file_path,
        border_style,
        app.theme.diff.empty_text,
    );
}

pub(super) fn render_new_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup_width = area.width * 70 / 100;
    let popup = clear_popup(frame, area, popup_width.max(40), 17);
    let theme = &app.theme.dialog;

    let field_style = |active: bool| {
        if active {
            Style::default().fg(theme.new_ws_active)
        } else {
            Style::default().fg(theme.new_ws_inactive)
        }
    };
    let label_width = 10_u16;
    let fmax = popup.width.saturating_sub(label_width + 2) as usize;

    let name_active = app.active_dialog_field == DialogField::Name;
    let dir_active = app.active_dialog_field == DialogField::Directory;
    let desc_active = app.active_dialog_field == DialogField::Description;
    let prompt_active = app.active_dialog_field == DialogField::Prompt;
    let kanban_active = app.active_dialog_field == DialogField::KanbanPath;

    let lines = vec![
        Line::from(vec![
            Span::styled("  Name:   ", field_style(name_active)),
            Span::styled(visible_field(&app.input_buffer, name_active, app.input_cursor, fmax), field_style(name_active)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Dir:    ", field_style(dir_active)),
            Span::styled(visible_field(&app.dir_input_buffer, dir_active, app.dir_input_cursor, fmax), field_style(dir_active)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Desc:   ", field_style(desc_active)),
            Span::styled(visible_field(&app.desc_input_buffer, desc_active, app.desc_input_cursor, fmax), field_style(desc_active)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Prompt: ", field_style(prompt_active)),
            Span::styled(visible_field(&app.prompt_input_buffer, prompt_active, app.prompt_input_cursor, fmax), field_style(prompt_active)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Kanban: ", field_style(kanban_active)),
            Span::styled(visible_field(&app.kanban_input_buffer, kanban_active, app.kanban_input_cursor, fmax), field_style(kanban_active)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("  [Esc] Cancel", Style::default().fg(theme.new_ws_inactive))]),
    ];

    let text = Paragraph::new(lines).block(popup_block("New Workspace", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(super) fn render_edit_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup_width = area.width * 70 / 100;
    let popup = clear_popup(frame, area, popup_width.max(40), 11);
    let theme = &app.theme.dialog;

    let field_style = |active: bool| {
        if active {
            Style::default().fg(theme.new_ws_active)
        } else {
            Style::default().fg(theme.new_ws_inactive)
        }
    };
    let label_width = 10_u16;
    let fmax = popup.width.saturating_sub(label_width + 2) as usize;

    let kanban_active = app.active_dialog_field == DialogField::KanbanPath;
    let prompt_active = app.active_dialog_field == DialogField::Prompt;

    let lines = vec![
        Line::from(vec![
            Span::styled("  Kanban: ", field_style(kanban_active)),
            Span::styled(visible_field(&app.kanban_input_buffer, kanban_active, app.kanban_input_cursor, fmax), field_style(kanban_active)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Prompt: ", field_style(prompt_active)),
            Span::styled(visible_field(&app.prompt_input_buffer, prompt_active, app.prompt_input_cursor, fmax), field_style(prompt_active)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("  [Esc] Cancel", Style::default().fg(theme.new_ws_inactive))]),
    ];

    let text = Paragraph::new(lines).block(popup_block("Edit Workspace", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(super) fn render_help_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let cfg = &app.config;
    let popup = clear_popup(frame, area, 55, 62);

    let help_text = vec![
        "".to_string(),
        "  Navigation mode (yellow border)".to_string(),
        format!(
            "    {:<13} Move between panes",
            format!(
                "{}/{}/{}/{}",
                cfg.get_binding("navigation", "up"),
                cfg.get_binding("navigation", "down"),
                cfg.get_binding("navigation", "left"),
                cfg.get_binding("navigation", "right")
            )
        ),
        format!(
            "    {:<13} Interact with pane",
            cfg.get_binding("navigation", "enter_pane")
        ),
        format!(
            "    {:<13} New workspace",
            cfg.get_binding("navigation", "new_workspace")
        ),
        format!(
            "    {:<13} Clone workspace",
            cfg.get_binding("navigation", "clone_workspace")
        ),
        format!(
            "    {:<13} Edit workspace",
            cfg.get_binding("navigation", "edit_workspace")
        ),
        format!(
            "    {:<13} Delete workspace",
            cfg.get_binding("navigation", "delete_workspace")
        ),
        format!(
            "    {:<13} Next/Prev workspace",
            format!(
                "{}/{}",
                cfg.get_binding("navigation", "next_workspace"),
                cfg.get_binding("navigation", "prev_workspace")
            )
        ),
        format!("    {:<13} Go to workspace N", "1-9"),
        format!(
            "    {:<13} Next/Prev tab",
            format!(
                "{}/{}",
                cfg.get_binding("navigation", "next_tab"),
                cfg.get_binding("navigation", "prev_tab")
            )
        ),
        format!(
            "    {:<13} New tab",
            cfg.get_binding("navigation", "new_tab")
        ),
        format!(
            "    {:<13} Close tab",
            cfg.get_binding("navigation", "close_tab")
        ),
        format!(
            "    {:<13} Toggle help",
            cfg.get_binding("navigation", "help")
        ),
        format!("    {:<13} About", cfg.get_binding("navigation", "about")),
        format!("    {:<13} Quit", cfg.get_binding("navigation", "quit")),
        "".to_string(),
        "  Interaction mode (green border)".to_string(),
        format!(
            "    {:<13} Back to navigation",
            cfg.get_binding("interaction", "exit_interaction")
        ),
        "".to_string(),
        "  Terminal pane (navigation mode)".to_string(),
        format!(
            "    {:<13} Scroll up/down (3 lines)",
            format!(
                "{}/{}",
                cfg.get_binding("navigation", "scroll_up"),
                cfg.get_binding("navigation", "scroll_down")
            )
        ),
        format!(
            "    {:<13} Scroll by page",
            format!(
                "{}/{}",
                cfg.get_binding("navigation", "page_up"),
                cfg.get_binding("navigation", "page_down")
            )
        ),
        "    Mouse scroll  Scroll up/down".to_string(),
        "".to_string(),
        "  Terminal pane (interaction mode)".to_string(),
        "    All keys sent to active tab".to_string(),
        "".to_string(),
        "  File list pane".to_string(),
        format!(
            "    {:<13} Select file",
            format!(
                "{}/{}",
                cfg.get_binding("file_list", "up"),
                cfg.get_binding("file_list", "down")
            )
        ),
        format!("    {:<13} Open diff", cfg.get_binding("file_list", "diff")),
        "".to_string(),
        "  Workspace list pane (interaction mode)".to_string(),
        format!(
            "    {:<13} Select workspace",
            format!(
                "{}/{}",
                cfg.get_binding("workspace_list", "up"),
                cfg.get_binding("workspace_list", "down")
            )
        ),
        format!(
            "    {:<13} Switch to workspace",
            cfg.get_binding("workspace_list", "select")
        ),
        format!(
            "    {:<13} Delete workspace",
            cfg.get_binding("workspace_list", "delete")
        ),
        format!(
            "    {:<13} Back to navigation",
            cfg.get_binding("interaction", "exit_interaction")
        ),
        "".to_string(),
        "  Diff view".to_string(),
        format!(
            "    {:<13} Scroll",
            format!(
                "{}/{}",
                cfg.get_binding("diff", "up"),
                cfg.get_binding("diff", "down")
            )
        ),
        format!(
            "    {:<13} Page down/up",
            format!(
                "{}/{}",
                cfg.get_binding("diff", "page_up"),
                cfg.get_binding("diff", "page_down")
            )
        ),
        format!(
            "    {:<13} Top/Bottom",
            format!(
                "{}/{}",
                cfg.get_binding("diff", "scroll_top"),
                cfg.get_binding("diff", "scroll_bottom")
            )
        ),
        format!(
            "    {:<13} Next/Prev file",
            format!(
                "{}/{}",
                cfg.get_binding("diff", "next_file"),
                cfg.get_binding("diff", "prev_file")
            )
        ),
        format!("    {:<13} Close diff", cfg.get_binding("diff", "exit")),
        "".to_string(),
        format!(
            "  Fuzzy search ({} or {})",
            cfg.get_binding("navigation", "fuzzy_search"),
            cfg.get_binding("navigation", "fuzzy_search_alt")
        )
        .to_string(),
        "    Type          Filter files".to_string(),
        format!(
            "    {:<13} Select result",
            format!(
                "{}/{}",
                cfg.get_binding("fuzzy", "up"),
                cfg.get_binding("fuzzy", "down")
            )
        ),
        format!("    {:<13} Open diff", cfg.get_binding("fuzzy", "diff")),
        format!(
            "    {:<13} Open in $EDITOR",
            cfg.get_binding("fuzzy", "editor")
        ),
        format!(
            "    {:<13} Inline editor",
            cfg.get_binding("fuzzy", "inline_edit")
        ),
        format!(
            "    {:<13} Open markdown viewer",
            cfg.get_binding("fuzzy", "markdown")
        ),
        format!(
            "    {:<13} Open in mdr (external)",
            cfg.get_binding("fuzzy", "mdr")
        ),
        format!("    {:<13} Close", cfg.get_binding("fuzzy", "exit")),
        "".to_string(),
        "  File list (interaction mode)".to_string(),
        format!(
            "    {:<13} Open in $EDITOR",
            cfg.get_binding("file_list", "edit_external")
        ),
        format!(
            "    {:<13} Inline editor",
            cfg.get_binding("file_list", "edit_inline")
        ),
        format!(
            "    {:<13} Stage file (git add)",
            cfg.get_binding("file_list", "stage")
        ),
        format!(
            "    {:<13} Unstage file (git reset)",
            cfg.get_binding("file_list", "unstage")
        ),
        "".to_string(),
        "  Git operations".to_string(),
        format!(
            "    {:<13} Commit (opens dialog)",
            cfg.get_binding("navigation", "commit")
        ),
        format!("    {:<13} Push", cfg.get_binding("navigation", "push")),
        "".to_string(),
        "  Inline editor".to_string(),
        format!("    {:<13} Save", cfg.get_binding("editor", "save")),
        format!("    {:<13} Close", cfg.get_binding("editor", "exit")),
        "".to_string(),
        "  Pane resize".to_string(),
        format!(
            "    {:<13} Resize sidebar width",
            format!(
                "{} / {}",
                cfg.get_binding("navigation", "sidebar_shrink"),
                cfg.get_binding("navigation", "sidebar_grow")
            )
        ),
        format!(
            "    {:<13} Resize workspace/file split",
            format!(
                "{} / {}",
                cfg.get_binding("navigation", "split_up"),
                cfg.get_binding("navigation", "split_down")
            )
        ),
        "    Mouse drag    Drag pane borders to resize".to_string(),
        "".to_string(),
        "  Clipboard".to_string(),
        "    Mouse drag    Select text in terminal".to_string(),
        format!(
            "    {:<13} Copy visible terminal content",
            cfg.get_binding("interaction", "copy")
        ),
        format!(
            "    {:<13} Paste from clipboard (terminal)",
            cfg.get_binding("interaction", "paste")
        ),
    ];

    let block = popup_block("Help", theme.help.border);

    let total_lines = help_text.len() as u16;
    let inner_height = popup.height.saturating_sub(2); // borders
    let max_scroll = total_lines.saturating_sub(inner_height);
    let scroll = app.help_scroll.min(max_scroll);

    let scroll_indicator = if max_scroll > 0 {
        format!(
            " [{}/{} ↑{}/{}↓] ",
            scroll + 1,
            max_scroll + 1,
            cfg.get_binding("help", "up"),
            cfg.get_binding("help", "down")
        )
    } else {
        String::new()
    };

    let block = block.title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(help_text.join("\n"))
        .block(block)
        .scroll((scroll, 0));
    frame.render_widget(text, popup);
}

pub(super) fn render_about_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let popup = clear_popup(frame, area, 50, 20);

    let version = env!("CARGO_PKG_VERSION");
    let version_line = format!("piki-multi-ai v{version}");
    let about_lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(""),
        Line::from("██████╗ ██╗██╗  ██╗██╗"),
        Line::from("██╔══██╗██║██║ ██╔╝██║"),
        Line::from("██████╔╝██║█████╔╝ ██║"),
        Line::from("██╔═══╝ ██║██╔═██╗ ██║"),
        Line::from("██║     ██║██║  ██╗██║"),
        Line::from("╚═╝     ╚═╝╚═╝  ╚═╝╚═╝"),
        Line::from(""),
        Line::from(version_line),
        Line::from(""),
        Line::from("Author: Juan Knebel"),
        Line::from("Contact: juanknebel@gmail.com"),
        Line::from("Web: github.com/juanknebel/piki-multi-ai"),
        Line::from("License: GPL-2.0"),
        Line::from(""),
        Line::from("Press Esc to close"),
    ];

    let text = Paragraph::new(about_lines)
        .block(popup_block("About", app.theme.help.border))
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(text, popup);
}

pub(super) fn render_workspace_info_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let ws = &app.workspaces[app.selected_workspace];

    let label_style = Style::default()
        .add_modifier(Modifier::BOLD)
        .fg(theme.help.border);
    let project_name = ws
        .source_repo
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ws.source_repo.to_string_lossy().to_string());

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" Branch:  ", label_style),
            Span::raw(ws.branch.clone()),
        ]),
        Line::from(vec![
            Span::styled(" Project: ", label_style),
            Span::raw(project_name),
        ]),
        Line::from(vec![
            Span::styled("          ", label_style),
            Span::raw(ws.source_repo.to_string_lossy().to_string()),
        ]),
        Line::from(vec![
            Span::styled(" Path:    ", label_style),
            Span::raw(ws.path.to_string_lossy().to_string()),
        ]),
        Line::from(vec![
            Span::styled(" Kanban:  ", label_style),
            Span::raw(
                ws.kanban_path
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
            ),
        ]),
        Line::from(""),
    ];

    if !ws.description.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " Description: ",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.help.border),
        )]));
        lines.push(Line::from(format!("  {}", ws.description)));
        lines.push(Line::from(""));
    }

    if !ws.prompt.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " Prompt: ",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.help.border),
        )]));
        // Wrap prompt text
        let max_width = 56usize;
        for chunk in ws.prompt.as_bytes().chunks(max_width) {
            let s = String::from_utf8_lossy(chunk);
            lines.push(Line::from(format!("  {}", s)));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        " Esc to close · h/l to scroll · mouse select to copy",
        Style::default().fg(Color::DarkGray),
    )));

    let height = (lines.len() as u16 + 2).min(area.height);
    let popup = clear_popup(frame, area, 70, height);

    let block = popup_block(&ws.name, theme.help.border);

    let text = Paragraph::new(lines)
        .block(block)
        .scroll((0, app.info_hscroll));
    frame.render_widget(text, popup);
}

pub(super) fn render_confirm_delete_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup = clear_popup(frame, area, 50, 9);
    let theme = &app.theme.dialog;

    let ws_name = app
        .delete_target
        .and_then(|idx| app.workspaces.get(idx))
        .map(|ws| ws.name.as_str())
        .unwrap_or("?");

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Delete ", Style::default().fg(theme.delete_text)),
            Span::styled(
                ws_name,
                Style::default()
                    .fg(theme.delete_name)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ?", Style::default().fg(theme.delete_text)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [y] Yes, delete worktree and branch",
            Style::default().fg(theme.delete_yes),
        )),
        Line::from(Span::styled(
            "  [n] No, keep worktree on disk",
            Style::default().fg(theme.delete_no),
        )),
        Line::from(Span::styled(
            "  [Esc] Cancel",
            Style::default().fg(theme.delete_cancel),
        )),
    ];

    let text = Paragraph::new(lines).block(popup_block("Delete Workspace", theme.delete_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_confirm_close_tab_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme.dialog;
    let tab_name = app
        .close_tab_target
        .and_then(|idx| {
            app.current_workspace()
                .and_then(|ws| ws.tabs.get(idx))
                .map(|t| format!("{:?}", t.provider))
        })
        .unwrap_or_default();
    render_yn_dialog(frame, area, "Close Tab", &format!("Close tab \"{}\"?", tab_name), theme.delete_border, theme.delete_cancel);
}

pub(crate) fn render_confirm_quit_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme.dialog;
    render_yn_dialog(frame, area, "Quit", "Are you sure you want to quit?", theme.delete_border, theme.delete_cancel);
}

pub(super) fn render_commit_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup_width = area.width * 60 / 100;
    let popup = clear_popup(frame, area, popup_width.max(40), 7);
    let theme = &app.theme.dialog;

    let field_max = popup.width.saturating_sub(14) as usize;
    let cursor = "█";
    let full = format!("{}{}", app.commit_msg_buffer, cursor);
    let visible = if full.len() > field_max && field_max > 2 {
        format!("…{}", &full[full.len() - (field_max - 1)..])
    } else {
        full
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Message: ", Style::default().fg(theme.new_ws_active)),
            Span::styled(visible, Style::default().fg(theme.new_ws_active)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [Enter] Commit  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )),
    ];

    let text = Paragraph::new(lines).block(popup_block("Commit", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(super) fn render_confirm_merge_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup = clear_popup(frame, area, 50, 9);
    let theme = &app.theme.dialog;

    let branch_name = app
        .current_workspace()
        .map(|ws| ws.branch.as_str())
        .unwrap_or("?");

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Merge ", Style::default().fg(theme.new_ws_active)),
            Span::styled(
                branch_name,
                Style::default()
                    .fg(theme.new_ws_active)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" into main?", Style::default().fg(theme.new_ws_active)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [m] Merge commit",
            Style::default().fg(theme.new_ws_inactive),
        )),
        Line::from(Span::styled(
            "  [r] Rebase + fast-forward",
            Style::default().fg(theme.new_ws_inactive),
        )),
        Line::from(Span::styled(
            "  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )),
    ];

    let text = Paragraph::new(lines).block(popup_block("Merge", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_new_tab_dialog(frame: &mut Frame, area: Rect) {
    let popup = clear_popup(frame, area, 40, 12);

    let lines = vec![
        Line::from(""),
        Line::from("  Select provider:"),
        Line::from(""),
        Line::from("  [1] Claude Code"),
        Line::from("  [2] Gemini"),
        Line::from("  [3] Codex"),
        Line::from("  [4] Shell"),
        Line::from("  [5] Kanban Board"),
        Line::from("  [6] Pomodoro Timer"),
        Line::from(""),
        Line::from("  [Esc] Cancel"),
    ];

    let text = Paragraph::new(lines).block(popup_block("New Tab", Color::Cyan));
    frame.render_widget(text, popup);
}

pub(super) fn render_pomodoro_config_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup = clear_popup(frame, area, 45, 13);
    let theme = &app.theme.dialog;

    let field_style = |active: bool| {
        if active {
            Style::default().fg(theme.new_ws_active)
        } else {
            Style::default().fg(theme.new_ws_inactive)
        }
    };
    let fmax = 10usize;

    let work_active = app.active_dialog_field == DialogField::PomodoroWork;
    let short_active = app.active_dialog_field == DialogField::PomodoroShort;
    let long_active = app.active_dialog_field == DialogField::PomodoroLong;
    let cycles_active = app.active_dialog_field == DialogField::PomodoroCycles;

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Work (min):    ", field_style(work_active)),
            Span::styled(visible_field(&app.pomodoro_input_work, work_active, app.pomodoro_input_work_cursor, fmax), field_style(work_active)),
        ]),
        Line::from(vec![
            Span::styled("  Short (min):   ", field_style(short_active)),
            Span::styled(visible_field(&app.pomodoro_input_short, short_active, app.pomodoro_input_short_cursor, fmax), field_style(short_active)),
        ]),
        Line::from(vec![
            Span::styled("  Long (min):    ", field_style(long_active)),
            Span::styled(visible_field(&app.pomodoro_input_long, long_active, app.pomodoro_input_long_cursor, fmax), field_style(long_active)),
        ]),
        Line::from(vec![
            Span::styled("  Cycles:        ", field_style(cycles_active)),
            Span::styled(visible_field(&app.pomodoro_input_cycles, cycles_active, app.pomodoro_input_cycles_cursor, fmax), field_style(cycles_active)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("  [Tab] Switch field  [Enter] Start  [Esc] Cancel", Style::default().fg(theme.new_ws_inactive))]),
    ];

    let text = Paragraph::new(lines).block(popup_block("Pomodoro Config", theme.new_ws_border));
    frame.render_widget(text, popup);
}
