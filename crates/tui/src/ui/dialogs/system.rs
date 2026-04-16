use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::dialog_state::{DialogState, NewTabMenu};

pub(crate) fn render_diff_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width * 90 / 100;
    let height = area.height * 85 / 100;
    let popup = super::clear_popup(frame, area, width, height);

    let file_path = app.diff_file_path.as_deref().unwrap_or("?");
    let border_style = Style::default().fg(app.theme.diff.border);
    crate::ui::diff::render(
        frame,
        popup,
        &app.diff_content,
        app.diff_scroll,
        file_path,
        border_style,
        app.theme.diff.empty_text,
        app.theme.general.scrollbar_thumb,
    );
}

pub(crate) fn render_help_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let help_scroll = match app.active_dialog {
        Some(DialogState::Help { scroll }) => scroll,
        _ => 0,
    };

    let theme = &app.theme;
    let cfg = &app.config;
    let popup = super::clear_popup(frame, area, 55, 75);

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
        format!(
            "    {:<13} AI Chat",
            cfg.get_binding("navigation", "chat_panel")
        ),
        format!("    {:<13} Quit", cfg.get_binding("navigation", "quit")),
        "".to_string(),
        "  AI Chat overlay".to_string(),
        "    Enter         Send message".to_string(),
        "    Tab           Select model".to_string(),
        format!("    {:<13} Settings (URL, system prompt)", cfg.format_binding("ctrl-o")),
        format!("    {:<13} Clear conversation", cfg.format_binding("ctrl-l")),
        "    Esc           Hide (keeps state)".to_string(),
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
        "  Kanban board (interaction mode)".to_string(),
        "    h/l/j/k     Navigate columns and cards".to_string(),
        "    H/L         Move card left/right".to_string(),
        "    n           New card".to_string(),
        "    e           Edit selected card".to_string(),
        "    d           Delete card".to_string(),
        "    D           Dispatch agent (feature/bug/spike branch + AI)".to_string(),
        "    Enter       Toggle card details".to_string(),
        "    r           Refresh board".to_string(),
        "    Esc         Close".to_string(),
        "".to_string(),
        "  Dispatch agent dialog (D on kanban card)".to_string(),
        "    ◄/►/Tab     Cycle agent/provider (includes (None))".to_string(),
        "    Enter       With agent: dispatch to new worktree".to_string(),
        "                With (None): choose workspace (New/Current)".to_string(),
        "    Esc         Cancel / Back".to_string(),
        "".to_string(),
        "  Manage providers (Alt+P in navigation mode)".to_string(),
        "    j/k         Navigate provider list".to_string(),
        "    n           New provider".to_string(),
        "    e / Enter   Edit selected provider".to_string(),
        "    d           Delete selected provider".to_string(),
        "    Esc         Close".to_string(),
        "".to_string(),
        "  Manage agents (A in navigation mode)".to_string(),
        "    j/k         Navigate agent list".to_string(),
        "    n           New agent (step 1: name + provider)".to_string(),
        "    e / Enter   Edit selected agent".to_string(),
        "    d           Delete selected agent".to_string(),
        "    p           Sync agent to repo".to_string(),
        "    i           Import agents from repo".to_string(),
        "    Esc         Close".to_string(),
        "  Import agents overlay (i in manage agents)".to_string(),
        "    j/k         Navigate discovered agents".to_string(),
        "    Space       Toggle selection".to_string(),
        "    a           Toggle select all".to_string(),
        "    Enter       Import selected".to_string(),
        "    Esc         Cancel".to_string(),
        "  Agent role editor (step 2)".to_string(),
        format!("    {:<13} Save agent and close", cfg.format_binding("ctrl-s")),
        "    Esc         Back to step 1 without saving".to_string(),
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
        format!("    {}/{}      Page down/up in diff", cfg.format_binding("ctrl-d"), cfg.format_binding("ctrl-u")),
        "    s             Open submit review dialog".to_string(),
        "    r             Refresh PR data".to_string(),
        "    q             Close review (discard state)".to_string(),
        "    Tab           Cycle verdict (in submit)".to_string(),
        format!("    {:<13} Discard draft (in submit)", cfg.format_binding("ctrl-shift-d")),
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

    let block = super::popup_block("Help", theme.help.border);

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

pub(crate) fn render_about_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let popup = super::clear_popup(frame, area, 50, 20);

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
        .block(super::popup_block("About", app.theme.help.border))
        .alignment(ratatui::layout::Alignment::Center);
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
    super::render_yn_dialog(
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
    super::render_yn_dialog(
        frame,
        area,
        "Quit",
        "Are you sure you want to quit?",
        theme.delete_border,
        theme.delete_cancel,
    );
}

pub(crate) fn render_logs_overlay(frame: &mut Frame, area: Rect, app: &App) {
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
    let popup = super::clear_popup(frame, area, width.max(40), height.max(10));
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

    let block = super::popup_block(&title, app.theme.help.border)
        .title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(lines).block(block).scroll((0, log_hscroll));
    frame.render_widget(text, popup);
}

pub(crate) fn render_new_tab_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let menu = match app.active_dialog {
        Some(DialogState::NewTab { ref menu }) => menu.clone(),
        _ => NewTabMenu::Main,
    };

    match menu {
        NewTabMenu::Main => {
            let popup = super::clear_popup(frame, area, 40, 9);
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
            let text = Paragraph::new(lines).block(super::popup_block("New Tab", Color::Cyan));
            frame.render_widget(text, popup);
        }
        NewTabMenu::Agents { selected } => {
            // Build provider list dynamically from built-in + custom providers
            let providers = app.new_tab_agent_list();
            let height = (providers.len() as u16) + 5; // padding + header + footer
            let popup = super::clear_popup(frame, area, 40, height);
            let mut lines: Vec<Line<'_>> = vec![Line::from("")];
            for (i, provider) in providers.iter().enumerate() {
                let num = if i < 9 {
                    format!("[{}] ", i + 1)
                } else {
                    "    ".to_string()
                };
                let label = provider.label();
                let style = if i == selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(format!("  {num}{label}"), style)));
            }
            lines.push(Line::from(""));
            lines.push(Line::from("  [Esc] Back"));
            let text = Paragraph::new(lines).block(super::popup_block("AI Agents", Color::Cyan));
            frame.render_widget(text, popup);
        }
        NewTabMenu::Tools => {
            let popup = super::clear_popup(frame, area, 40, 9);
            let lines = vec![
                Line::from(""),
                Line::from("  [1] Kanban Board"),
                Line::from("  [2] Code Review"),
                Line::from("  [3] API Explorer"),
                Line::from(""),
                Line::from("  [Esc] Back"),
            ];
            let text = Paragraph::new(lines).block(super::popup_block("Tools", Color::Cyan));
            frame.render_widget(text, popup);
        }
    }
}
