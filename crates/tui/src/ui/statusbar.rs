use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{ActivePane, App, AppMode, ToastLevel};

pub(crate) fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme.status_bar;
    let content = if let Some(ref toast) = app.toast {
        let (bg, fg, icon) = match toast.level {
            ToastLevel::Info => (theme.navigate_bg, theme.mode_fg, "ℹ"),
            ToastLevel::Success => (theme.interact_bg, theme.mode_fg, "✓"),
            ToastLevel::Error => (theme.error_bg, theme.error_fg, "✗"),
        };
        Span::styled(
            format!(" {} {} ", icon, toast.message),
            Style::default().bg(bg).fg(fg),
        )
    } else if let Some(msg) = &app.status_message {
        Span::styled(
            format!(" {} ", msg),
            Style::default().bg(theme.error_bg).fg(theme.error_fg),
        )
    } else {
        let ctrl = if app.config.platform.is_macos() {
            "⌘"
        } else {
            "C"
        };
        match app.mode {
            AppMode::Diff => Span::styled(
                format!(
                    " DIFF: {} | [{}-g] back | [↑↓] scroll | [n/p] file",
                    app.diff_file_path.as_deref().unwrap_or("?"),
                    ctrl,
                ),
                Style::default().bg(theme.diff_bg).fg(theme.diff_fg),
            ),
            AppMode::FuzzySearch => Span::styled(
                format!(
                    " SEARCH | type to filter | Enter = diff | {}-e = editor | Esc = close",
                    ctrl,
                ),
                Style::default().bg(theme.navigate_bg).fg(theme.mode_fg),
            ),
            AppMode::InlineEdit => Span::styled(
                format!(
                    " EDIT: {} | {}-s = save | Esc = close",
                    app.editing_file
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    ctrl,
                ),
                Style::default().bg(theme.interact_bg).fg(theme.mode_fg),
            ),
            _ => {
                render_normal_status(frame, area, app);
                return;
            }
        }
    };

    let bar = Paragraph::new(Line::from(content));
    frame.render_widget(bar, area);
}

fn render_normal_status(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme.status_bar;
    let mode_label = if app.interacting {
        "INTERACT"
    } else {
        "NAVIGATE"
    };
    let mode_bg = if app.interacting {
        theme.interact_bg
    } else {
        theme.navigate_bg
    };
    let text_style = Style::default().bg(mode_bg).fg(theme.mode_fg);
    let sep = Span::styled(" │ ", Style::default().bg(mode_bg).fg(theme.separator_fg));

    let Some(ws) = app.current_workspace() else {
        let bar = Paragraph::new(Line::from(vec![
            Span::styled(format!(" [{}]", mode_label), text_style),
            sep,
            Span::styled("No active workspace", text_style),
        ]));
        frame.render_widget(bar, area);
        return;
    };

    let mut left: Vec<Span> = vec![Span::styled(format!(" [{}]", mode_label), text_style)];

    let is_project = ws.info.workspace_type == piki_core::WorkspaceType::Project;
    if is_project {
        left.push(sep.clone());
        left.push(Span::styled("project", text_style));
        left.push(sep.clone());
        left.push(Span::styled(
            format!("{} services", ws.file_count()),
            text_style,
        ));
    } else {
        left.push(sep.clone());
        left.push(Span::styled(format!("⎇ {}", ws.branch), text_style));
        left.push(sep.clone());
        left.push(Span::styled(
            format!("{} files", ws.file_count()),
            text_style,
        ));

        match ws.ahead_behind {
            Some((ahead, behind)) if ahead > 0 && behind > 0 => {
                left.push(sep.clone());
                left.push(Span::styled(
                    format!("↑{} ↓{}", ahead, behind),
                    text_style,
                ));
            }
            Some((ahead, 0)) if ahead > 0 => {
                left.push(sep.clone());
                left.push(Span::styled(format!("↑{}", ahead), text_style));
            }
            Some((0, behind)) if behind > 0 => {
                left.push(sep.clone());
                left.push(Span::styled(format!("↓{}", behind), text_style));
            }
            _ => {}
        }
    }

    let tab_label = ws
        .current_tab()
        .map(|t| t.provider.label())
        .unwrap_or("—");
    left.push(sep.clone());
    left.push(Span::styled(
        format!("{}: {}", tab_label, ws.status_label()),
        text_style,
    ));

    // Right section: workspace counter and scroll indicator
    let mut right: Vec<Span> = vec![Span::styled(
        format!("ws {}/{}", app.active_workspace + 1, app.workspaces.len()),
        text_style,
    )];

    let tab_scroll = ws.current_tab().map(|t| t.term_scroll).unwrap_or(0);
    if tab_scroll > 0 {
        right.push(sep.clone());
        right.push(Span::styled(
            format!("SCROLL -{} ", tab_scroll),
            text_style,
        ));
    } else {
        right.push(Span::styled(" ", text_style));
    }

    // Pad between left and right to push right section to the edge
    let left_width: usize = left.iter().map(|s| s.width()).sum();
    let right_width: usize = right.iter().map(|s| s.width()).sum();
    let total = area.width as usize;

    let mut spans = left;
    if total > left_width + right_width {
        let pad = total - left_width - right_width;
        spans.push(Span::styled(
            " ".repeat(pad),
            Style::default().bg(mode_bg),
        ));
    } else {
        spans.push(sep);
    }
    spans.extend(right);

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}

pub(super) fn render_sysinfo_bar(frame: &mut Frame, area: Rect, app: &App) {
    let text = app.sysinfo.lock().clone();

    let spans: Vec<Span> = text
        .split(" | ")
        .flat_map(|part| {
            if let Some((label, value)) = part.split_once(' ') {
                vec![
                    Span::styled(
                        format!(" [{}] ", label),
                        Style::default().fg(app.theme.footer.key),
                    ),
                    Span::styled(
                        format!("{} ", value),
                        Style::default().fg(app.theme.footer.description),
                    ),
                ]
            } else {
                vec![Span::styled(
                    format!(" {} ", part),
                    Style::default().fg(app.theme.footer.description),
                )]
            }
        })
        .collect();

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}

pub(crate) fn footer_keys(app: &App) -> Vec<(String, &'static str)> {
    let cfg = &app.config;
    match app.mode {
        AppMode::CommandPalette | AppMode::WorkspaceSwitcher => vec![
            ("up/down".to_string(), "select"),
            ("enter".to_string(), "execute"),
            ("esc".to_string(), "close"),
        ],
        AppMode::FuzzySearch => vec![
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("fuzzy", "up"),
                    cfg.get_binding("fuzzy", "down")
                ),
                "select",
            ),
            (cfg.get_binding("fuzzy", "diff"), "diff"),
            (cfg.get_binding("fuzzy", "editor"), "editor"),
            (cfg.get_binding("fuzzy", "inline_edit"), "inline edit"),
            (cfg.get_binding("fuzzy", "markdown"), "markdown"),
            (cfg.get_binding("fuzzy", "mdr"), "mdr"),
            (cfg.get_binding("fuzzy", "exit"), "close"),
        ],
        AppMode::InlineEdit => vec![
            (cfg.get_binding("editor", "save"), "save"),
            (cfg.get_binding("editor", "exit"), "close"),
        ],
        AppMode::NewWorkspace => vec![
            (
                cfg.get_binding("new_workspace", "switch_field"),
                "switch field",
            ),
            (cfg.get_binding("new_workspace", "create"), "create"),
            (cfg.get_binding("new_workspace", "exit"), "cancel"),
        ],
        AppMode::CommitMessage => vec![
            (cfg.get_binding("commit", "commit"), "commit"),
            (cfg.get_binding("commit", "exit"), "cancel"),
        ],
        AppMode::ConfirmMerge => vec![
            (cfg.get_binding("merge", "merge"), "merge"),
            (cfg.get_binding("merge", "rebase"), "rebase"),
            (cfg.get_binding("merge", "exit"), "cancel"),
        ],
        AppMode::EditWorkspace => vec![
            (
                cfg.get_binding("new_workspace", "switch_field"),
                "switch field",
            ),
            ("enter".to_string(), "save"),
            ("esc".to_string(), "cancel"),
        ],
        AppMode::NewTab => {
            let menu = match app.active_dialog {
                Some(crate::dialog_state::DialogState::NewTab { ref menu }) => menu.clone(),
                _ => crate::dialog_state::NewTabMenu::Main,
            };
            match menu {
                crate::dialog_state::NewTabMenu::Main => vec![
                    ("1-3".to_string(), "select"),
                    (cfg.get_binding("new_tab", "exit"), "cancel"),
                ],
                crate::dialog_state::NewTabMenu::Agents { .. } => vec![
                    ("j/k".to_string(), "navigate"),
                    ("enter/1-9".to_string(), "select"),
                    (cfg.get_binding("new_tab", "exit"), "back"),
                ],
                crate::dialog_state::NewTabMenu::Tools => vec![
                    ("1-3".to_string(), "select"),
                    (cfg.get_binding("new_tab", "exit"), "back"),
                ],
            }
        }
        AppMode::SubmitReview => vec![
            ("Tab".to_string(), "cycle verdict"),
            ("enter".to_string(), "submit"),
            ("esc".to_string(), "close"),
            (cfg.format_binding("ctrl-d"), "discard"),
        ],
        AppMode::Diff => vec![
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("diff", "up"),
                    cfg.get_binding("diff", "down")
                ),
                "scroll",
            ),
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("diff", "page_up"),
                    cfg.get_binding("diff", "page_down")
                ),
                "page",
            ),
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("diff", "scroll_top"),
                    cfg.get_binding("diff", "scroll_bottom")
                ),
                "top/bottom",
            ),
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("diff", "next_file"),
                    cfg.get_binding("diff", "prev_file")
                ),
                "next/prev file",
            ),
            (cfg.get_binding("diff", "exit"), "close"),
        ],
        AppMode::Logs => vec![
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("logs", "up"),
                    cfg.get_binding("logs", "down")
                ),
                "select",
            ),
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("logs", "left"),
                    cfg.get_binding("logs", "right")
                ),
                "hscroll",
            ),
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("logs", "page_up"),
                    cfg.get_binding("logs", "page_down")
                ),
                "page",
            ),
            (cfg.get_binding("logs", "copy"), "copy"),
            ("0-5".to_string(), "filter"),
            (cfg.get_binding("logs", "exit"), "close"),
        ],
        AppMode::Dashboard => vec![
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("dashboard", "up"),
                    cfg.get_binding("dashboard", "down")
                ),
                "select",
            ),
            (cfg.get_binding("dashboard", "select"), "switch"),
            (cfg.get_binding("dashboard", "exit"), "close"),
        ],
        AppMode::GitStash => vec![
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("git_stash", "up"),
                    cfg.get_binding("git_stash", "down")
                ),
                "select",
            ),
            (cfg.get_binding("git_stash", "save"), "save"),
            (cfg.get_binding("git_stash", "pop"), "pop"),
            (cfg.get_binding("git_stash", "apply"), "apply"),
            (cfg.get_binding("git_stash", "drop"), "drop"),
            (cfg.get_binding("git_stash", "show"), "show diff"),
            (cfg.get_binding("git_stash", "exit"), "close"),
        ],
        AppMode::GitLog => vec![
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("git_log", "up"),
                    cfg.get_binding("git_log", "down")
                ),
                "navigate",
            ),
            (cfg.get_binding("git_log", "select"), "view diff"),
            (cfg.get_binding("git_log", "exit"), "close"),
        ],
        AppMode::ConflictResolution => vec![
            (
                format!(
                    "{}/{}",
                    cfg.get_binding("conflict_resolution", "up"),
                    cfg.get_binding("conflict_resolution", "down")
                ),
                "select",
            ),
            (cfg.get_binding("conflict_resolution", "ours"), "ours"),
            (cfg.get_binding("conflict_resolution", "theirs"), "theirs"),
            (
                cfg.get_binding("conflict_resolution", "mark_resolved"),
                "mark resolved",
            ),
            (cfg.get_binding("conflict_resolution", "edit"), "edit"),
            (cfg.get_binding("conflict_resolution", "abort"), "abort"),
            (cfg.get_binding("conflict_resolution", "exit"), "close"),
        ],
        AppMode::ConfirmCloseTab => vec![("Y".to_string(), "close"), ("N".to_string(), "cancel")],
        AppMode::ConfirmQuit => vec![("Y".to_string(), "quit"), ("N".to_string(), "cancel")],
        AppMode::DispatchCardMove => vec![
            ("↑/↓".to_string(), "select"),
            ("enter".to_string(), "confirm"),
            ("esc".to_string(), "cancel"),
        ],
        _ if app.interacting => {
            if app.active_pane == ActivePane::GitStatus {
                let is_project = app
                    .current_workspace()
                    .is_some_and(|ws| ws.info.workspace_type == piki_core::WorkspaceType::Project);
                if is_project {
                    vec![
                        (
                            format!(
                                "{}/{}",
                                cfg.get_binding("file_list", "up"),
                                cfg.get_binding("file_list", "down")
                            ),
                            "select",
                        ),
                        ("enter".to_string(), "open as workspace"),
                        (cfg.get_binding("interaction", "exit_interaction"), "back"),
                    ]
                } else {
                    let has_sel = app.selection_count() > 0;
                    let stage_label: &'static str = if has_sel { "stage sel" } else { "stage" };
                    let unstage_label: &'static str =
                        if has_sel { "unstage sel" } else { "unstage" };
                    vec![
                        (
                            format!(
                                "{}/{}",
                                cfg.get_binding("file_list", "up"),
                                cfg.get_binding("file_list", "down")
                            ),
                            "navigate",
                        ),
                        (
                            cfg.get_binding("file_list", "toggle_select"),
                            "select",
                        ),
                        (cfg.get_binding("file_list", "diff"), "diff"),
                        (cfg.get_binding("file_list", "stage"), stage_label),
                        (cfg.get_binding("file_list", "unstage"), unstage_label),
                        (cfg.get_binding("file_list", "select_all"), "sel all"),
                        (cfg.get_binding("file_list", "edit_external"), "editor"),
                        (cfg.get_binding("interaction", "exit_interaction"), "back"),
                    ]
                }
            } else if app
                .current_workspace()
                .and_then(|ws| ws.current_tab())
                .is_some_and(|tab| tab.api_state.is_some())
            {
                let has_search = app
                    .current_workspace()
                    .and_then(|ws| ws.current_tab())
                    .and_then(|tab| tab.api_state.as_ref())
                    .is_some_and(|api| api.search.is_some());
                if has_search {
                    vec![
                        ("enter".to_string(), "next match"),
                        ("shift-enter".to_string(), "prev match"),
                        ("esc".to_string(), "close search"),
                    ]
                } else {
                    {
                        let p = if cfg.platform.is_macos() { "⌘" } else { "^" };
                        vec![
                            (format!("{}S", p), "send"),
                            (format!("{}J/{}K", p, p), "scroll"),
                            (format!("{}F", p), "search"),
                            (format!("{}C", p), "copy response"),
                            (cfg.get_binding("interaction", "exit_interaction"), "back"),
                        ]
                    }
                }
            } else if app
                .current_workspace()
                .and_then(|ws| ws.current_tab())
                .is_some_and(|tab| tab.markdown_content.is_some())
            {
                vec![
                    (
                        format!(
                            "{}/{}",
                            cfg.get_binding("markdown", "up"),
                            cfg.get_binding("markdown", "down")
                        ),
                        "scroll",
                    ),
                    (
                        format!(
                            "{}/{}",
                            cfg.get_binding("markdown", "page_up"),
                            cfg.get_binding("markdown", "page_down")
                        ),
                        "page",
                    ),
                    (
                        format!(
                            "{}/{}",
                            cfg.get_binding("markdown", "scroll_top"),
                            cfg.get_binding("markdown", "scroll_bottom")
                        ),
                        "top/bottom",
                    ),
                    (cfg.get_binding("interaction", "exit_interaction"), "back"),
                ]
            } else if app.term_search.is_some() {
                vec![
                    ("enter".to_string(), "next match"),
                    ("shift-enter".to_string(), "prev match"),
                    ("esc".to_string(), "close search"),
                ]
            } else {
                vec![
                    (cfg.get_binding("interaction", "search"), "search"),
                    (
                        cfg.get_binding("interaction", "exit_interaction"),
                        "navigation mode",
                    ),
                ]
            }
        }
        _ => {
            let nav = format!(
                "{}{}{}{}",
                cfg.get_binding("navigation", "up"),
                cfg.get_binding("navigation", "down"),
                cfg.get_binding("navigation", "left"),
                cfg.get_binding("navigation", "right")
            );
            let mut keys = vec![
                (nav, "navigate"),
                (cfg.get_binding("navigation", "enter_pane"), "interact"),
            ];
            match app.active_pane {
                ActivePane::WorkspaceList => {
                    keys.push((cfg.get_binding("navigation", "new_workspace"), "new ws"));
                    keys.push((cfg.get_binding("navigation", "clone_workspace"), "clone ws"));
                    keys.push((cfg.get_binding("navigation", "edit_workspace"), "edit ws"));
                    keys.push((
                        cfg.get_binding("navigation", "delete_workspace"),
                        "delete ws",
                    ));
                    keys.push((cfg.get_binding("navigation", "next_workspace"), "switch ws"));
                }
                ActivePane::GitStatus => {
                    let is_project = app.current_workspace().is_some_and(|ws| {
                        ws.info.workspace_type == piki_core::WorkspaceType::Project
                    });
                    keys.push((cfg.get_binding("navigation", "fuzzy_search"), "search"));
                    if !is_project {
                        keys.push((cfg.get_binding("navigation", "commit"), "commit"));
                        keys.push((cfg.get_binding("navigation", "push"), "push"));
                        keys.push((cfg.get_binding("navigation", "merge"), "merge"));
                        keys.push((cfg.get_binding("navigation", "undo"), "undo"));
                    }
                }
                ActivePane::MainPanel => {
                    keys.push((cfg.get_binding("navigation", "new_tab"), "new tab"));
                    keys.push((cfg.get_binding("navigation", "close_tab"), "close tab"));
                    keys.push((
                        format!(
                            "{}/{}",
                            cfg.get_binding("navigation", "next_tab"),
                            cfg.get_binding("navigation", "prev_tab")
                        ),
                        "next/prev tab",
                    ));
                }
            }
            keys.push((
                format!(
                    "{}/{}",
                    cfg.get_binding("navigation", "sidebar_shrink"),
                    cfg.get_binding("navigation", "sidebar_grow")
                ),
                "resize",
            ));
            keys.push((cfg.get_binding("navigation", "command_palette"), "commands"));
            keys.push((
                cfg.get_binding("navigation", "workspace_switcher"),
                "switch ws",
            ));
            keys.push((cfg.get_binding("navigation", "help"), "help"));
            keys.push((cfg.get_binding("navigation", "quit"), "quit"));
            keys
        }
    }
}

pub(crate) fn render_footer_from_keys(
    frame: &mut Frame,
    area: Rect,
    keys: &[(String, &str)],
    theme: &crate::theme::Theme,
) {
    let make_spans = |items: &[(String, &str)]| -> Vec<Span<'static>> {
        items
            .iter()
            .flat_map(|(key, desc)| {
                vec![
                    Span::styled(
                        format!(" [{}] ", key),
                        Style::default().fg(theme.footer.key),
                    ),
                    Span::styled(
                        format!("{} ", desc),
                        Style::default().fg(theme.footer.description),
                    ),
                ]
            })
            .collect()
    };

    // Calculate total width of all spans
    let total_width: usize = keys
        .iter()
        .map(|(key, desc)| key.len() + desc.len() + 5)
        .sum();

    if total_width as u16 <= area.width || area.height < 2 {
        // Single line
        let spans = make_spans(keys);
        let footer = Paragraph::new(Line::from(spans));
        frame.render_widget(footer, area);
    } else {
        // Split into two lines: find the split point closest to half
        let mut acc = 0usize;
        let half = total_width / 2;
        let mut split_at = keys.len();
        for (i, (key, desc)) in keys.iter().enumerate() {
            acc += key.len() + desc.len() + 5;
            if acc >= half {
                split_at = i + 1;
                break;
            }
        }
        let line1 = Line::from(make_spans(&keys[..split_at]));
        let line2 = Line::from(make_spans(&keys[split_at..]));
        let footer = Paragraph::new(vec![line1, line2]);
        frame.render_widget(footer, area);
    }
}
