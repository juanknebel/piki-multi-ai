use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action_catalog::{Context, HELP_ORDER, catalog};
use crate::app::App;
use crate::config::Config;
use crate::dialog_state::{DialogState, NewTabMenu};

/// Width of the key column in the help browser.
const KEY_COL: usize = 13;

/// The whole help body, derived from the action catalog: one section per
/// [`Context`], one line per key. Nothing here is hand-maintained — adding an
/// entry to the catalog is what makes it show up.
pub(crate) fn help_lines(cfg: &Config) -> Vec<String> {
    let mut out = Vec::new();

    for &ctx in HELP_ORDER {
        let entries: Vec<&crate::action_catalog::ActionMeta> =
            catalog().iter().filter(|a| a.context == ctx).collect();
        if entries.is_empty() {
            continue;
        }

        let heading = match ctx.opened_by() {
            Some(id) => format!("  {} ({})", ctx.title(), cfg.get_binding("app", id)),
            None => format!("  {}", ctx.title()),
        };
        out.push(String::new());
        out.push(heading);

        if ctx == Context::Global {
            out.push(format!(
                "    Press {} first (tmux-style).",
                cfg.prefix_display()
            ));
        }

        // Global is the only context with enough entries to need sub-grouping;
        // it reuses the which-key categories.
        let mut current_category = "";
        for a in entries {
            if ctx == Context::Global && !a.category.is_empty() && a.category != current_category {
                current_category = a.category;
                out.push(format!("    {current_category}"));
            }
            let key = a.keys.display(cfg);
            let indent = if ctx == Context::Global && !current_category.is_empty() {
                "      "
            } else {
                "    "
            };
            out.push(format!("{indent}{key:<KEY_COL$} {}", a.label));
        }
    }

    out
}

pub(crate) fn render_help_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (help_scroll, filter) = match &app.active_dialog {
        Some(DialogState::Help { scroll, filter }) => (*scroll, filter.as_str()),
        _ => (0, ""),
    };

    let theme = &app.theme;
    let cfg = &app.config;
    let popup = super::clear_popup(frame, area, 55, 75);

    let help_text = help_lines(cfg);

    // Live filter: keep only content lines matching the query (case-insensitive).
    // A leading status line always shows so the box reads as searchable.
    let lines: Vec<String> = if filter.is_empty() {
        let mut out = vec![
            "  Type to filter · ↑↓ PgUp/PgDn scroll · Esc close".to_string(),
            String::new(),
        ];
        out.extend(help_text);
        out
    } else {
        let q = filter.to_lowercase();
        let matches: Vec<String> = help_text
            .into_iter()
            .filter(|l| !l.trim().is_empty() && l.to_lowercase().contains(&q))
            .collect();
        let mut out = vec![
            format!("  Filter: {}   ({} matches)", filter, matches.len()),
            String::new(),
        ];
        out.extend(matches);
        out
    };

    let title = if filter.is_empty() {
        "Help".to_string()
    } else {
        format!("Help  /{filter}")
    };
    let block = super::popup_block(&title, theme.help.border);

    let total_lines = lines.len() as u16;
    let inner_height = popup.height.saturating_sub(2); // borders
    let max_scroll = total_lines.saturating_sub(inner_height);
    let scroll = help_scroll.min(max_scroll);

    let scroll_indicator = if max_scroll > 0 {
        format!(" [{}/{} ↑↓] ", scroll + 1, max_scroll + 1)
    } else {
        String::new()
    };

    let block = block.title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(lines.join("\n"))
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
        Line::from("Press Esc or a to close"),
    ];

    let text = Paragraph::new(about_lines)
        .block(super::popup_block("About", app.theme.help.border))
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(text, popup);
}

/// Warn that a bridged agent (Claude Code, Antigravity) opened without the
/// tools its hooks need. The tab itself is fine — only its *status* degrades to
/// the byte-silence heuristic — so this is informational, not a failure.
pub(crate) fn render_missing_prereqs_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (agent, missing) = match app.active_dialog {
        Some(DialogState::MissingPrereqs {
            ref agent,
            ref missing,
        }) => (agent.clone(), missing.join(", ")),
        _ => return,
    };
    let popup = super::clear_popup(frame, area, 58, 13);
    let theme = &app.theme;

    let lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Missing: ",
                Style::default().fg(theme.general.muted_text),
            ),
            Span::styled(
                missing,
                Style::default()
                    .fg(theme.status.needs_you)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(format!(
            "{agent} runs fine, but piki can't read its status:"
        )),
        Line::from("the Agents pane will show bare liveness (alive/exited)"),
        Line::from("instead of running / idle / done."),
        Line::from(""),
        Line::from(Span::styled(
            "Install it and reopen the tab to get the full status.",
            Style::default().fg(theme.general.muted_text),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Esc or Enter to dismiss",
            Style::default().fg(theme.general.muted_text),
        )),
    ];

    let text = Paragraph::new(lines)
        .block(super::popup_block(
            "Agent status unavailable",
            theme.status.needs_you,
        ))
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
    let (log_scroll, level_filter, log_selected, log_hscroll, search_active, search_buffer, search_cursor, auto_refresh) =
        match &app.active_dialog {
            Some(DialogState::Logs {
                scroll,
                level_filter,
                selected,
                hscroll,
                search_active,
                search_buffer,
                search_cursor,
                auto_refresh,
            }) => (*scroll, *level_filter, *selected, *hscroll, *search_active, search_buffer.as_str(), *search_cursor, *auto_refresh),
            _ => (u16::MAX, 0, usize::MAX, 0, false, "", 0, true),
        };

    let width = area.width * 90 / 100;
    let height = area.height * 85 / 100;
    let popup = super::clear_popup(frame, area, width.max(40), height.max(10));
    let show_search = search_active || !search_buffer.is_empty();
    let search_row_height = if show_search { 1u16 } else { 0 };
    let inner_height = popup.height.saturating_sub(3 + search_row_height) as usize;
    let inner_width = popup.width.saturating_sub(2) as usize;

    // Read log entries filtered by level and search query
    let buf = app.log_buffer.lock();
    let search_lower = search_buffer.to_lowercase();
    let filtered: Vec<_> = buf
        .iter()
        .filter(|entry| {
            if level_filter != 0 {
                let entry_num = match entry.level {
                    tracing::Level::ERROR => 1,
                    tracing::Level::WARN => 2,
                    tracing::Level::INFO => 3,
                    tracing::Level::DEBUG => 4,
                    tracing::Level::TRACE => 5,
                };
                if entry_num > level_filter {
                    return false;
                }
            }
            if !search_lower.is_empty() {
                return entry.message.to_lowercase().contains(&search_lower)
                    || entry.target.to_lowercase().contains(&search_lower);
            }
            true
        })
        .collect();

    let total = filtered.len();
    let effective_selected = if auto_refresh { total.saturating_sub(1) } else { log_selected };
    // Clamp selected to valid range
    let selected = if total == 0 {
        0
    } else {
        effective_selected.min(total.saturating_sub(1))
    };

    // Auto-scroll to keep selection visible
    let max_scroll = total.saturating_sub(inner_height);
    let scroll = if log_scroll == u16::MAX || auto_refresh {
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

    let theme = &app.theme;
    for (view_idx, entry) in filtered[start..end].iter().enumerate() {
        let abs_idx = start + view_idx;
        let is_selected = abs_idx == selected && total > 0;

        let level_color = match entry.level {
            tracing::Level::ERROR => theme.palette.err,
            tracing::Level::WARN => theme.palette.warn,
            tracing::Level::INFO => theme.palette.info,
            tracing::Level::DEBUG => theme.palette.fg3,
            tracing::Level::TRACE => theme.palette.fg3,
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
            let sel_style = Style::default()
                .bg(theme.workspace_list.selected_bg)
                .fg(theme.palette.fg0);
            lines.push(Line::from(vec![Span::styled(
                format!("{:<width$}", full_text, width = pad_width),
                sel_style,
            )]));
        } else {
            // Normal line with colored spans — Paragraph::scroll handles hscroll
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", entry.timestamp),
                    Style::default().fg(theme.palette.fg2),
                ),
                Span::styled(format!("{} ", level_str), Style::default().fg(level_color)),
                Span::styled(
                    format!("{} ", entry.target),
                    Style::default().fg(theme.palette.fg2),
                ),
                Span::styled(&entry.message, Style::default().fg(theme.palette.fg0)),
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

    let auto_refresh_marker = if auto_refresh { " ~" } else { "" };
    let title = format!(" Logs [{}]{} ", filter_label, auto_refresh_marker);
    let scroll_indicator = if total > 0 {
        format!(" [{}/{}] ", selected + 1, total)
    } else {
        " [0/0] ".to_string()
    };

    let (content_area, search_area) = if show_search {
        let [content, search] = ratatui::layout::Layout::vertical([
            ratatui::layout::Constraint::Min(0),
            ratatui::layout::Constraint::Length(1),
        ])
        .areas(popup);
        (content, Some(search))
    } else {
        (popup, None)
    };

    let block = super::popup_block(&title, app.theme.help.border)
        .title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(lines).block(block).scroll((0, log_hscroll));
    frame.render_widget(text, content_area);

    if let Some(search_rect) = search_area {
        let prefix = " / ";
        let prompt_style = Style::default().fg(app.theme.palette.iris);
        let search_style = Style::default().fg(app.theme.palette.fg0);
        let search_line = Line::from(vec![
            Span::styled(prefix, prompt_style),
            Span::styled(search_buffer, search_style),
        ]);
        frame.render_widget(
            Paragraph::new(search_line).style(Style::default().bg(Color::Reset)),
            search_rect,
        );
        if search_active {
            let cursor_x = (search_rect.x + prefix.len() as u16 + search_cursor as u16)
                .min(search_rect.x + search_rect.width.saturating_sub(1));
            frame.set_cursor_position((cursor_x, search_rect.y));
        }
    }
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
                    Span::styled("→", Style::default().fg(app.theme.palette.fg3)),
                ]),
                Line::from(vec![
                    Span::raw("  [3] Tools      "),
                    Span::styled("→", Style::default().fg(app.theme.palette.fg3)),
                ]),
                Line::from(""),
                Line::from("  [Esc] cancel"),
            ];
            let text = Paragraph::new(lines)
                .block(super::popup_block("New Tab", app.theme.palette.iris));
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
                    Style::default()
                        .fg(app.theme.palette.bg0)
                        .bg(app.theme.palette.iris)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(format!("  {num}{label}"), style)));
            }
            lines.push(Line::from(""));
            lines.push(Line::from("  [Esc] back"));
            let text = Paragraph::new(lines)
                .block(super::popup_block("AI Agents", app.theme.palette.iris));
            frame.render_widget(text, popup);
        }
        NewTabMenu::Tools => {
            let popup = super::clear_popup(frame, area, 40, 10);
            let lines = vec![
                Line::from(""),
                Line::from("  [1] Kanban Board"),
                Line::from("  [2] Code Review"),
                Line::from("  [3] API Explorer"),
                Line::from("  [4] Git (lazygit)"),
                Line::from(""),
                Line::from("  [Esc] back"),
            ];
            let text = Paragraph::new(lines)
                .block(super::popup_block("Tools", app.theme.palette.iris));
            frame.render_widget(text, popup);
        }
    }
}
