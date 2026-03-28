use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, DialogField, WorkspaceType};
use crate::dialog_state::{DialogState, NewTabMenu};

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
fn render_yn_dialog(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    message: &str,
    border_color: Color,
    hint_color: Color,
) {
    let popup = clear_popup(frame, area, 40, 7);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, Style::default().fg(Color::White))).centered(),
        Line::from(""),
        Line::from(Span::styled(
            "[Y] Yes    [N] No",
            Style::default().fg(hint_color),
        ))
        .centered(),
    ];
    let text = Paragraph::new(lines).block(popup_block(title, border_color));
    frame.render_widget(text, popup);
}

/// Render a labeled text field line.
fn render_text_field<'a>(
    label: &'a str,
    text: &str,
    active: bool,
    cursor: usize,
    fmax: usize,
    field_style: Style,
) -> Line<'a> {
    Line::from(vec![
        Span::styled(label, field_style),
        Span::styled(visible_field(text, active, cursor, fmax), field_style),
    ])
}

fn field_style(active: bool, active_color: Color, inactive_color: Color) -> Style {
    if active {
        Style::default().fg(active_color)
    } else {
        Style::default().fg(inactive_color)
    }
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
    let Some(DialogState::NewWorkspace {
        ref name,
        name_cursor,
        ref dir,
        dir_cursor,
        ref desc,
        desc_cursor,
        ref prompt,
        prompt_cursor,
        ref kanban,
        kanban_cursor,
        ref group,
        group_cursor,
        ws_type,
        active_field,
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = area.width * 70 / 100;
    let hide_name = ws_type != WorkspaceType::Worktree;
    let popup_height = if hide_name { 17 } else { 19 };
    let popup = clear_popup(frame, area, popup_width.max(40), popup_height);
    let theme = &app.theme.dialog;

    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;
    let label_width = 10_u16;
    let fmax = popup.width.saturating_sub(label_width + 2) as usize;

    let type_active = active_field == DialogField::Type;
    let dir_active = active_field == DialogField::Directory;
    let desc_active = active_field == DialogField::Description;
    let prompt_active = active_field == DialogField::Prompt;
    let kanban_active = active_field == DialogField::KanbanPath;
    let group_active = active_field == DialogField::Group;

    let type_text = match ws_type {
        WorkspaceType::Simple => "[Simple]  Worktree   Project",
        WorkspaceType::Worktree => " Simple  [Worktree]  Project",
        WorkspaceType::Project => " Simple   Worktree  [Project]",
    };

    let mut lines: Vec<Line<'_>> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("  Type:   ", field_style(type_active, active_c, inactive_c)),
        Span::styled(type_text, field_style(type_active, active_c, inactive_c)),
    ]));
    lines.push(Line::from(""));

    if !hide_name {
        let name_active = active_field == DialogField::Name;
        lines.push(render_text_field(
            "  Name:   ",
            name,
            name_active,
            name_cursor,
            fmax,
            field_style(name_active, active_c, inactive_c),
        ));
        lines.push(Line::from(""));
    }

    lines.push(render_text_field(
        "  Dir:    ",
        dir,
        dir_active,
        dir_cursor,
        fmax,
        field_style(dir_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(render_text_field(
        "  Desc:   ",
        desc,
        desc_active,
        desc_cursor,
        fmax,
        field_style(desc_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(render_text_field(
        "  Prompt: ",
        prompt,
        prompt_active,
        prompt_cursor,
        fmax,
        field_style(prompt_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(render_text_field(
        "  Kanban: ",
        kanban,
        kanban_active,
        kanban_cursor,
        fmax,
        field_style(kanban_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(render_text_field(
        "  Group:  ",
        group,
        group_active,
        group_cursor,
        fmax,
        field_style(group_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![Span::styled(
        "  [Esc] Cancel",
        Style::default().fg(theme.new_ws_inactive),
    )]));

    let text = Paragraph::new(lines).block(popup_block("New Workspace", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(super) fn render_edit_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::EditWorkspace {
        ref kanban,
        kanban_cursor,
        ref prompt,
        prompt_cursor,
        ref group,
        group_cursor,
        active_field,
        ..
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = area.width * 70 / 100;
    let popup = clear_popup(frame, area, popup_width.max(40), 13);
    let theme = &app.theme.dialog;

    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;
    let label_width = 10_u16;
    let fmax = popup.width.saturating_sub(label_width + 2) as usize;

    let kanban_active = active_field == DialogField::KanbanPath;
    let prompt_active = active_field == DialogField::Prompt;
    let group_active = active_field == DialogField::Group;

    let lines = vec![
        render_text_field(
            "  Kanban: ",
            kanban,
            kanban_active,
            kanban_cursor,
            fmax,
            field_style(kanban_active, active_c, inactive_c),
        ),
        Line::from(""),
        render_text_field(
            "  Prompt: ",
            prompt,
            prompt_active,
            prompt_cursor,
            fmax,
            field_style(prompt_active, active_c, inactive_c),
        ),
        Line::from(""),
        render_text_field(
            "  Group:  ",
            group,
            group_active,
            group_cursor,
            fmax,
            field_style(group_active, active_c, inactive_c),
        ),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )]),
    ];

    let text = Paragraph::new(lines).block(popup_block("Edit Workspace", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(super) fn render_help_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let help_scroll = match app.active_dialog {
        Some(DialogState::Help { scroll }) => scroll,
        _ => 0,
    };

    let theme = &app.theme;
    let cfg = &app.config;
    let popup = clear_popup(frame, area, 55, 75);

    let help_text = vec![
        "".to_string(),
        "  Navigation mode (yellow border)".to_string(),
        format!(
            "    {:<13} Move between panes (h→ws list)",
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
            "    {:<13} Fuzzy workspace search",
            cfg.get_binding("navigation", "workspace_switcher")
        ),
        format!(
            "    {:<13} Previous workspace",
            cfg.get_binding("navigation", "toggle_prev_workspace")
        ),
        format!(
            "    {:<13} Command palette",
            cfg.get_binding("navigation", "command_palette")
        ),
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
        format!(
            "    {:<13} Workspace info",
            cfg.get_binding("navigation", "workspace_info")
        ),
        format!(
            "    {:<13} Open Kanban Board",
            cfg.get_binding("navigation", "kanban")
        ),
        format!(
            "    {:<13} Undo last stage/unstage",
            cfg.get_binding("navigation", "undo")
        ),
        format!(
            "    {:<13} Quick stage (file list focused)",
            cfg.get_binding("navigation", "stage_quick")
        ),
        format!(
            "    {:<13} Quick unstage (file list focused)",
            cfg.get_binding("navigation", "unstage_quick")
        ),
        format!(
            "    {:<13} Dashboard",
            cfg.get_binding("navigation", "dashboard")
        ),
        format!("    {:<13} Logs", cfg.get_binding("navigation", "logs")),
        format!(
            "    {:<13} Git log",
            cfg.get_binding("navigation", "git_log")
        ),
        format!("    {:<13} Quit", cfg.get_binding("navigation", "quit")),
        "".to_string(),
        "  Interaction mode (green border)".to_string(),
        format!(
            "    {:<13} Back to navigation",
            cfg.get_binding("interaction", "exit_interaction")
        ),
        "    Esc           Back (non-terminal panes)".to_string(),
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
        format!(
            "    {:<13} Search in terminal output",
            cfg.get_binding("interaction", "search")
        ),
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
            "    {:<13} Switch + focus main panel",
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
        format!(
            "    {:<13} Toggle multi-select",
            cfg.get_binding("file_list", "toggle_select")
        ),
        format!(
            "    {:<13} Select/deselect all",
            cfg.get_binding("file_list", "select_all")
        ),
        "".to_string(),
        "  Git operations".to_string(),
        format!(
            "    {:<13} Commit (opens dialog)",
            cfg.get_binding("navigation", "commit")
        ),
        format!("    {:<13} Push", cfg.get_binding("navigation", "push")),
        format!(
            "    {:<13} Merge/Rebase into main",
            cfg.get_binding("navigation", "merge")
        ),
        format!(
            "    {:<13} Conflict resolution",
            cfg.get_binding("navigation", "conflicts")
        ),
        format!(
            "    {:<13} Git stash overlay",
            cfg.get_binding("navigation", "stash")
        ),
        "".to_string(),
        "  Git stash overlay".to_string(),
        format!(
            "    {:<13} Save new stash (enter message)",
            cfg.get_binding("git_stash", "save")
        ),
        format!(
            "    {:<13} Pop selected stash",
            cfg.get_binding("git_stash", "pop")
        ),
        format!(
            "    {:<13} Apply selected stash",
            cfg.get_binding("git_stash", "apply")
        ),
        format!(
            "    {:<13} Drop selected stash",
            cfg.get_binding("git_stash", "drop")
        ),
        format!(
            "    {:<13} Show stash diff",
            cfg.get_binding("git_stash", "show")
        ),
        format!(
            "    {:<13} Close",
            cfg.get_binding("git_stash", "exit")
        ),
        "".to_string(),
        "  Conflict resolution overlay".to_string(),
        format!(
            "    {:<13} Navigate files",
            format!(
                "{}/{}",
                cfg.get_binding("conflict_resolution", "up"),
                cfg.get_binding("conflict_resolution", "down")
            )
        ),
        format!(
            "    {:<13} Resolve with ours",
            cfg.get_binding("conflict_resolution", "ours")
        ),
        format!(
            "    {:<13} Resolve with theirs",
            cfg.get_binding("conflict_resolution", "theirs")
        ),
        format!(
            "    {:<13} Mark as resolved (git add)",
            cfg.get_binding("conflict_resolution", "mark_resolved")
        ),
        format!(
            "    {:<13} Edit file in $EDITOR",
            cfg.get_binding("conflict_resolution", "edit")
        ),
        format!(
            "    {:<13} Abort merge/rebase",
            cfg.get_binding("conflict_resolution", "abort")
        ),
        format!(
            "    {:<13} Close",
            cfg.get_binding("conflict_resolution", "exit")
        ),
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
        "  Code Review (requires gh CLI, locked mode)".to_string(),
        "    j/k           Navigate files / scroll diff".to_string(),
        "    Enter         View file diff".to_string(),
        "    h/l           Switch file list / diff pane".to_string(),
        "    n/p           Next/prev file (in diff view)".to_string(),
        "    g/G           Top/bottom of diff".to_string(),
        "    Ctrl+d/u      Page down/up in diff".to_string(),
        "    s             Open submit review dialog".to_string(),
        "    r             Refresh PR data".to_string(),
        "    q             Close review (discard state)".to_string(),
        "    Tab           Cycle verdict (in submit)".to_string(),
        "    Ctrl+D        Discard draft (in submit)".to_string(),
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
    let scroll = help_scroll.min(max_scroll);

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

pub(super) fn render_dashboard_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (selected, scroll_offset) = match app.active_dialog {
        Some(DialogState::Dashboard {
            selected,
            scroll_offset,
        }) => (selected, scroll_offset),
        _ => (0, 0),
    };

    let theme = &app.theme;
    let width = area.width * 85 / 100;
    let height = area.height * 80 / 100;
    let popup = clear_popup(frame, area, width.max(40), height.max(10));

    let inner_height = popup.height.saturating_sub(2); // borders
    let narrow = popup.width < 70;

    // Footer hint = 1 line
    let visible_rows = inner_height.saturating_sub(1) as usize;

    // Build all visual lines, then slice by scroll_offset
    let mut body_lines: Vec<Line<'_>> = Vec::new();

    let total = app.workspaces.len();
    for i in 0..total {
        let ws = &app.workspaces[i];
        let is_active = i == app.active_workspace;
        let is_selected = i == selected;

        // Marker
        let marker = if is_active { "▸ " } else { "  " };

        // Name
        let name: String = ws.name.chars().take(24).collect();

        // Status
        let status_label = ws.status_label();
        let status_color = match &ws.status {
            piki_core::WorkspaceStatus::Idle => Color::DarkGray,
            piki_core::WorkspaceStatus::Busy => Color::Yellow,
            piki_core::WorkspaceStatus::Done => Color::Green,
            piki_core::WorkspaceStatus::Error(_) => Color::Red,
        };

        // Row style
        let base_fg = if is_active {
            theme.workspace_list.name_active
        } else {
            Color::White
        };
        let bg = if is_selected {
            theme.workspace_list.selected_bg
        } else {
            Color::Reset
        };
        let row_style = Style::default().fg(base_fg).bg(bg);
        let status_style = Style::default().fg(status_color).bg(bg);
        let muted = Style::default().fg(Color::DarkGray).bg(bg);

        // Workspace header line
        if narrow {
            let files = ws.file_count();
            body_lines.push(Line::from(vec![
                Span::styled(marker, row_style),
                Span::styled(name, row_style.add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {} ", status_label), status_style),
                Span::styled(format!("{}f", files), muted),
            ]));
        } else {
            let branch: String = ws.branch.chars().take(20).collect();
            let files = ws.file_count();
            let ab = match ws.ahead_behind {
                Some((ahead, behind)) if ahead > 0 || behind > 0 => {
                    format!(" ↑{} ↓{}", ahead, behind)
                }
                _ => String::new(),
            };
            body_lines.push(Line::from(vec![
                Span::styled(marker, row_style),
                Span::styled(
                    format!("{:<24}", name),
                    row_style.add_modifier(Modifier::BOLD),
                ),
                Span::styled(branch, muted),
                Span::styled(format!("  {} ", status_label), status_style),
                Span::styled(format!("{}f", files), muted),
                Span::styled(ab, muted),
            ]));
        }

        // Tab lines (indented under workspace)
        if ws.tabs.is_empty() {
            body_lines.push(Line::from(Span::styled(
                "     (no tabs)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (ti, tab) in ws.tabs.iter().enumerate() {
                let label = tab
                    .markdown_label
                    .as_deref()
                    .unwrap_or(tab.provider.label());

                let alive = tab.pty_session.as_ref().is_some_and(|p| p.peek_alive());
                let (indicator, ind_color) = if tab.markdown_content.is_some() {
                    ("md", Color::Cyan)
                } else if alive {
                    ("●", Color::Green)
                } else if tab.pty_session.is_some() {
                    ("○", Color::DarkGray)
                } else {
                    ("—", Color::DarkGray)
                };

                let is_active_tab = ti == ws.active_tab;
                let tab_fg = if is_active_tab {
                    Color::White
                } else {
                    Color::DarkGray
                };
                let arrow = if is_active_tab { "→ " } else { "  " };

                body_lines.push(Line::from(vec![
                    Span::styled("     ", Style::default()),
                    Span::styled(arrow, Style::default().fg(tab_fg)),
                    Span::styled(label, Style::default().fg(tab_fg)),
                    Span::raw(" "),
                    Span::styled(indicator, Style::default().fg(ind_color)),
                ]));
            }
        }
    }

    let total_lines = body_lines.len();
    let scroll = scroll_offset.min(total_lines.saturating_sub(visible_rows));
    let end = total_lines.min(scroll + visible_rows);

    let mut lines: Vec<Line<'_>> = Vec::new();
    for line in body_lines.into_iter().skip(scroll).take(end - scroll) {
        lines.push(line);
    }

    // Footer hint
    lines.push(Line::from(Span::styled(
        " j/k navigate  Enter switch  Esc close",
        Style::default().fg(Color::DarkGray),
    )));

    // Scroll indicator in title
    let scroll_indicator = if total > 0 {
        format!(" [{}/{}] ", selected + 1, total)
    } else {
        String::new()
    };

    let block = popup_block("Dashboard", theme.help.border)
        .title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(lines).block(block);
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
    let info_hscroll = match app.active_dialog {
        Some(DialogState::WorkspaceInfo { hscroll }) => hscroll,
        _ => 0,
    };

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
        Line::from(vec![
            Span::styled(" Type:    ", label_style),
            Span::raw(match ws.info.workspace_type {
                WorkspaceType::Simple => "Simple",
                WorkspaceType::Worktree => "Worktree",
                WorkspaceType::Project => "Project",
            }),
        ]),
        Line::from(vec![
            Span::styled(" Group:   ", label_style),
            Span::raw(ws.info.group.as_deref().unwrap_or("(none)").to_string()),
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

    let text = Paragraph::new(lines).block(block).scroll((0, info_hscroll));
    frame.render_widget(text, popup);
}

pub(super) fn render_confirm_delete_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let target = match app.active_dialog {
        Some(DialogState::ConfirmDelete { target }) => Some(target),
        _ => None,
    };

    let popup = clear_popup(frame, area, 50, 9);
    let theme = &app.theme.dialog;

    let ws = target.and_then(|idx| app.workspaces.get(idx));
    let ws_name = ws.map(|ws| ws.name.as_str()).unwrap_or("?");
    let is_worktree = ws
        .map(|ws| ws.info.workspace_type == WorkspaceType::Worktree)
        .unwrap_or(false);

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                if is_worktree {
                    "  Delete "
                } else {
                    "  Remove "
                },
                Style::default().fg(theme.delete_text),
            ),
            Span::styled(
                ws_name,
                Style::default()
                    .fg(theme.delete_name)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ?", Style::default().fg(theme.delete_text)),
        ]),
        Line::from(""),
    ];

    if !is_worktree {
        lines.push(Line::from(Span::styled(
            "  [y] Yes, remove from list",
            Style::default().fg(theme.delete_yes),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  [y] Yes, delete worktree and branch",
            Style::default().fg(theme.delete_yes),
        )));
        lines.push(Line::from(Span::styled(
            "  [n] No, keep worktree on disk",
            Style::default().fg(theme.delete_no),
        )));
    }
    lines.push(Line::from(Span::styled(
        "  [Esc] Cancel",
        Style::default().fg(theme.delete_cancel),
    )));

    let text = Paragraph::new(lines).block(popup_block("Delete Workspace", theme.delete_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_confirm_close_tab_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme.dialog;
    let tab_name = match app.active_dialog {
        Some(DialogState::ConfirmCloseTab { target }) => app
            .current_workspace()
            .and_then(|ws| ws.tabs.get(target))
            .map(|t| format!("{:?}", t.provider))
            .unwrap_or_default(),
        _ => String::new(),
    };
    render_yn_dialog(
        frame,
        area,
        "Close Tab",
        &format!("Close tab \"{}\"?", tab_name),
        theme.delete_border,
        theme.delete_cancel,
    );
}

pub(crate) fn render_confirm_quit_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme.dialog;
    render_yn_dialog(
        frame,
        area,
        "Quit",
        "Are you sure you want to quit?",
        theme.delete_border,
        theme.delete_cancel,
    );
}

pub(super) fn render_commit_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let commit_buffer = match app.active_dialog {
        Some(DialogState::CommitMessage { ref buffer }) => buffer.as_str(),
        _ => "",
    };

    let popup_width = area.width * 60 / 100;
    let popup = clear_popup(frame, area, popup_width.max(40), 7);
    let theme = &app.theme.dialog;

    let field_max = popup.width.saturating_sub(14) as usize;
    let cursor = "█";
    let full = format!("{}{}", commit_buffer, cursor);
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

pub(super) fn render_logs_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (log_scroll, level_filter, log_selected, log_hscroll) = match app.active_dialog {
        Some(DialogState::Logs {
            scroll,
            level_filter,
            selected,
            hscroll,
        }) => (scroll, level_filter, selected, hscroll),
        _ => (u16::MAX, 0, usize::MAX, 0),
    };

    let width = area.width * 90 / 100;
    let height = area.height * 85 / 100;
    let popup = clear_popup(frame, area, width.max(40), height.max(10));
    let inner_height = popup.height.saturating_sub(3) as usize; // borders + footer
    let inner_width = popup.width.saturating_sub(2) as usize; // borders

    // Read log entries and filter by level
    let buf = app.log_buffer.lock();
    let filtered: Vec<_> = buf
        .iter()
        .filter(|entry| {
            if level_filter == 0 {
                return true;
            }
            let entry_num = match entry.level {
                tracing::Level::ERROR => 1,
                tracing::Level::WARN => 2,
                tracing::Level::INFO => 3,
                tracing::Level::DEBUG => 4,
                tracing::Level::TRACE => 5,
            };
            entry_num <= level_filter
        })
        .collect();

    let total = filtered.len();
    // Clamp selected to valid range
    let selected = if total == 0 {
        0
    } else {
        log_selected.min(total.saturating_sub(1))
    };

    // Auto-scroll to keep selection visible
    let max_scroll = total.saturating_sub(inner_height);
    let scroll = if log_scroll == u16::MAX {
        // Auto-scroll: ensure selected is at bottom of view
        if total <= inner_height {
            0
        } else {
            selected
                .saturating_sub(inner_height.saturating_sub(1))
                .min(max_scroll)
        }
    } else {
        let mut s = (log_scroll as usize).min(max_scroll);
        // Ensure selected line is visible
        if selected < s {
            s = selected;
        } else if selected >= s + inner_height {
            s = selected.saturating_sub(inner_height.saturating_sub(1));
        }
        s
    };

    let hscroll = log_hscroll as usize;
    // Pad width ensures selected bg fills visible area after Paragraph::scroll
    let pad_width = hscroll + inner_width;

    let mut lines: Vec<Line<'_>> = Vec::new();
    let start = scroll;
    let end = total.min(scroll + inner_height);

    for (view_idx, entry) in filtered[start..end].iter().enumerate() {
        let abs_idx = start + view_idx;
        let is_selected = abs_idx == selected && total > 0;

        let level_color = match entry.level {
            tracing::Level::ERROR => Color::Red,
            tracing::Level::WARN => Color::Yellow,
            tracing::Level::INFO => Color::Green,
            tracing::Level::DEBUG => Color::Cyan,
            tracing::Level::TRACE => Color::DarkGray,
        };
        let level_str = match entry.level {
            tracing::Level::ERROR => "ERROR",
            tracing::Level::WARN => "WARN ",
            tracing::Level::INFO => "INFO ",
            tracing::Level::DEBUG => "DEBUG",
            tracing::Level::TRACE => "TRACE",
        };

        if is_selected {
            // Selected line: full-width background, padded so bg fills visible area after scroll
            let full_text = format!(
                " {} {} {} {}",
                entry.timestamp, level_str, entry.target, entry.message
            );
            let sel_style = Style::default().bg(Color::DarkGray).fg(Color::White);
            lines.push(Line::from(vec![Span::styled(
                format!("{:<width$}", full_text, width = pad_width),
                sel_style,
            )]));
        } else {
            // Normal line with colored spans — Paragraph::scroll handles hscroll
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", entry.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{} ", level_str), Style::default().fg(level_color)),
                Span::styled(
                    format!("{} ", entry.target),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(&entry.message, Style::default().fg(Color::White)),
            ]));
        }
    }

    // Footer
    let filter_label = match level_filter {
        0 => "all",
        1 => ">=error",
        2 => ">=warn",
        3 => ">=info",
        4 => ">=debug",
        5 => ">=trace",
        _ => "all",
    };

    let title = format!(" Logs [{}] ", filter_label);
    let scroll_indicator = if total > 0 {
        format!(" [{}/{}] ", selected + 1, total)
    } else {
        " [0/0] ".to_string()
    };

    let block = popup_block(&title, app.theme.help.border)
        .title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(lines).block(block).scroll((0, log_hscroll));
    frame.render_widget(text, popup);
}

pub(crate) fn render_new_tab_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let menu = match app.active_dialog {
        Some(DialogState::NewTab { menu }) => menu,
        _ => NewTabMenu::Main,
    };

    match menu {
        NewTabMenu::Main => {
            let popup = clear_popup(frame, area, 40, 9);
            let lines = vec![
                Line::from(""),
                Line::from("  Select category:"),
                Line::from(""),
                Line::from("  [1] Shell"),
                Line::from(vec![
                    Span::raw("  [2] AI Agents  "),
                    Span::styled("→", Style::default().fg(Color::DarkGray)),
                ]),
                Line::from(vec![
                    Span::raw("  [3] Tools      "),
                    Span::styled("→", Style::default().fg(Color::DarkGray)),
                ]),
                Line::from(""),
                Line::from("  [Esc] Cancel"),
            ];
            let text = Paragraph::new(lines).block(popup_block("New Tab", Color::Cyan));
            frame.render_widget(text, popup);
        }
        NewTabMenu::Agents => {
            let popup = clear_popup(frame, area, 40, 11);
            let lines = vec![
                Line::from(""),
                Line::from("  [1] Claude Code"),
                Line::from("  [2] Gemini"),
                Line::from("  [3] OpenCode"),
                Line::from("  [4] Kilo"),
                Line::from("  [5] Codex"),
                Line::from(""),
                Line::from("  [Esc] Back"),
            ];
            let text = Paragraph::new(lines).block(popup_block("AI Agents", Color::Cyan));
            frame.render_widget(text, popup);
        }
        NewTabMenu::Tools => {
            let popup = clear_popup(frame, area, 40, 9);
            let lines = vec![
                Line::from(""),
                Line::from("  [1] Kanban Board"),
                Line::from("  [2] Code Review"),
                Line::from("  [3] API Explorer"),
                Line::from(""),
                Line::from("  [Esc] Back"),
            ];
            let text = Paragraph::new(lines).block(popup_block("Tools", Color::Cyan));
            frame.render_widget(text, popup);
        }
    }
}

pub(super) fn render_git_stash_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (entries, selected, _scroll, input_mode, input_buffer, input_cursor) =
        match app.active_dialog {
            Some(DialogState::GitStash {
                ref entries,
                selected,
                scroll,
                input_mode,
                ref input_buffer,
                input_cursor,
            }) => (
                entries,
                selected,
                scroll,
                input_mode,
                input_buffer,
                input_cursor,
            ),
            _ => return,
        };

    let theme = &app.theme;
    let width = area.width * 75 / 100;
    let height = area.height * 70 / 100;
    let popup = clear_popup(frame, area, width.max(40), height.max(10));

    let inner_height = popup.height.saturating_sub(2) as usize; // borders
    // Reserve 2 lines for footer hint (and input if active)
    let footer_lines = if input_mode { 3 } else { 2 };
    let visible_rows = inner_height.saturating_sub(footer_lines);

    let mut lines: Vec<Line<'_>> = Vec::new();

    if entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                "  No stash entries",
                Style::default().fg(Color::DarkGray),
            ))
            .centered(),
        );
        lines.push(Line::from(""));
    } else {
        // Compute scroll to keep selected visible
        let scroll_offset = if selected >= visible_rows {
            selected + 1 - visible_rows
        } else {
            0
        };

        for (i, (stash_ref, message)) in entries.iter().enumerate() {
            if i < scroll_offset {
                continue;
            }
            if lines.len() >= visible_rows {
                break;
            }
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let bg = if is_selected {
                theme.workspace_list.selected_bg
            } else {
                Color::Reset
            };
            let ref_style = Style::default()
                .fg(Color::Yellow)
                .bg(bg)
                .add_modifier(Modifier::BOLD);
            let msg_style = Style::default().fg(Color::White).bg(bg);
            let marker_style = Style::default().fg(Color::Cyan).bg(bg);

            // Truncate message to fit
            let max_msg_len = popup.width.saturating_sub(20) as usize;
            let msg_display: String = if message.len() > max_msg_len {
                format!("{}...", &message[..max_msg_len.saturating_sub(3)])
            } else {
                message.clone()
            };

            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", marker), marker_style),
                Span::styled(format!("{:<12}", stash_ref), ref_style),
                Span::styled(format!(" {}", msg_display), msg_style),
            ]));
        }
    }

    // Pad remaining visible rows
    while lines.len() < visible_rows {
        lines.push(Line::from(""));
    }

    // Input line for stash save
    if input_mode {
        let label_style = Style::default().fg(Color::Cyan);
        let input_display = visible_field(
            input_buffer,
            true,
            input_cursor,
            popup.width.saturating_sub(14) as usize,
        );
        lines.push(Line::from(vec![
            Span::styled("  Message: ", label_style),
            Span::styled(input_display, Style::default().fg(Color::White)),
        ]));
    }

    // Footer hint
    let hint = if input_mode {
        " Enter confirm  Esc cancel"
    } else {
        " s save  p pop  a apply  d drop  Enter show  Esc close"
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )));

    // Title with count
    let title_text = if entries.is_empty() {
        " Git Stash ".to_string()
    } else {
        format!(" Git Stash [{}/{}] ", selected + 1, entries.len())
    };

    let block = Block::default()
        .title(title_text)
        .title_style(Style::default().fg(theme.help.border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.help.border));

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}

pub(super) fn render_git_log_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (log_lines, selected, scroll) = match app.active_dialog {
        Some(DialogState::GitLog {
            ref lines,
            selected,
            scroll,
        }) => (lines, selected, scroll),
        _ => return,
    };

    let theme = &app.theme;
    let width = (area.width * 85 / 100).max(40).min(area.width);
    let height = (area.height * 80 / 100).max(10).min(area.height);
    let popup = clear_popup(frame, area, width, height);

    let inner_height = popup.height.saturating_sub(2); // borders
    if inner_height < 3 {
        return;
    }

    let footer_height: u16 = 1;
    let body_height = (inner_height - footer_height) as usize;

    // Auto-scroll to keep selected visible
    let actual_scroll = if selected < scroll {
        selected
    } else if selected >= scroll + body_height {
        selected - body_height + 1
    } else {
        scroll
    };

    let total = log_lines.len();

    // Build visible lines
    let mut display_lines: Vec<Line<'_>> = Vec::new();
    for (i, entry) in log_lines
        .iter()
        .skip(actual_scroll)
        .take(body_height)
        .enumerate()
    {
        let is_selected = actual_scroll + i == selected;
        let bg = if is_selected {
            theme.workspace_list.selected_bg
        } else {
            Color::Reset
        };
        let style = Style::default().bg(bg);

        // Truncate line to fit popup width (leave 2 for borders + 1 for padding)
        let max_chars = (popup.width.saturating_sub(3)) as usize;
        let truncated: String = entry.raw_line.chars().take(max_chars).collect();

        display_lines.push(Line::from(Span::styled(
            format!(" {:<width$}", truncated, width = max_chars),
            style,
        )));
    }

    // Pad remaining space
    while display_lines.len() < body_height {
        display_lines.push(Line::from(""));
    }

    // Footer hint
    display_lines.push(Line::from(vec![
        Span::styled(
            " [j/k]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" navigate  "),
        Span::styled(
            "[enter]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" view diff  "),
        Span::styled(
            "[esc]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" close"),
    ]));

    // Title with scroll indicator
    let scroll_indicator = if total > 0 {
        format!(" [{}/{}] ", selected + 1, total)
    } else {
        String::new()
    };

    let block = popup_block("Git Log", theme.help.border)
        .title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(display_lines).block(block);
    frame.render_widget(text, popup);
}

pub(super) fn render_conflict_resolution_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (conflict_files, selected) = match app.active_dialog {
        Some(DialogState::ConflictResolution {
            ref files,
            selected,
            ..
        }) => (files, selected),
        _ => return,
    };

    let theme = &app.theme;
    let width = (area.width * 70 / 100).max(50).min(area.width);
    let height = (area.height * 60 / 100).max(12).min(area.height);
    let popup = clear_popup(frame, area, width, height);

    let inner_height = popup.height.saturating_sub(2) as usize; // borders
    let footer_lines = 2;
    let visible_rows = inner_height.saturating_sub(footer_lines);

    let mut lines: Vec<Line<'_>> = Vec::new();

    if conflict_files.is_empty() {
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                "  No conflicted files",
                Style::default().fg(Color::DarkGray),
            ))
            .centered(),
        );
        lines.push(Line::from(""));
    } else {
        // Compute scroll to keep selected visible
        let scroll_offset = if selected >= visible_rows {
            selected + 1 - visible_rows
        } else {
            0
        };

        for (i, f) in conflict_files.iter().enumerate() {
            if i < scroll_offset {
                continue;
            }
            if lines.len() >= visible_rows {
                break;
            }
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let bg = if is_selected {
                theme.workspace_list.selected_bg
            } else {
                Color::Reset
            };
            let marker_style = Style::default().fg(Color::Cyan).bg(bg);
            let icon_style = Style::default()
                .fg(Color::Magenta)
                .bg(bg)
                .add_modifier(Modifier::BOLD);
            let path_style = Style::default().fg(Color::White).bg(bg);

            // Truncate path to fit
            let max_path_len = popup.width.saturating_sub(10) as usize;
            let path_display: String = if f.path.len() > max_path_len {
                format!("...{}", &f.path[f.path.len() - max_path_len.saturating_sub(3)..])
            } else {
                f.path.clone()
            };

            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", marker), marker_style),
                Span::styled("C ", icon_style),
                Span::styled(path_display, path_style),
            ]));
        }
    }

    // Pad remaining visible rows
    while lines.len() < visible_rows {
        lines.push(Line::from(""));
    }

    // Footer hints
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            " [o]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" ours  "),
        Span::styled(
            "[t]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" theirs  "),
        Span::styled(
            "[m]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" mark resolved  "),
        Span::styled(
            "[e]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" edit  "),
        Span::styled(
            "[A]",
            Style::default().fg(Color::Red),
        ),
        Span::raw(" abort  "),
        Span::styled(
            "[esc]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" close"),
    ]));

    // Title with count indicator
    let count_indicator = format!(
        " [{}/{}] ",
        if conflict_files.is_empty() {
            0
        } else {
            selected + 1
        },
        conflict_files.len()
    );

    let block = popup_block("Conflict Resolution", Color::Magenta)
        .title_bottom(Line::from(count_indicator).right_aligned());

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}
