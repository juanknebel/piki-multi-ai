use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::dialog_state::DialogState;

pub(crate) fn render_dispatch_card_move_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::DispatchCardMove {
        ref columns,
        selected,
        ..
    }) = app.active_dialog
    else {
        return;
    };

    let theme = &app.theme.dialog;
    // Height: 1 blank + 1 title line + 1 blank + columns + 1 blank + 2 border = columns.len() + 5
    let height = (columns.len() as u16) + 5;
    let popup = super::clear_popup(frame, area, 50, height);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Move card to which column?",
            Style::default()
                .fg(theme.delete_text)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (i, (_id, title)) in columns.iter().enumerate() {
        let marker = if i == selected { "▸ " } else { "  " };
        let style = if i == selected {
            Style::default()
                .fg(theme.delete_name)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.delete_text)
        };
        lines.push(Line::from(Span::styled(format!("  {marker}{title}"), style)));
    }

    let text =
        Paragraph::new(lines).block(super::popup_block("Card Destination", theme.delete_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_dispatch_agent_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::DispatchAgent {
        ref card_title,
        ref card_description,
        agent_idx,
        ref agents,
        ref additional_prompt,
        additional_prompt_cursor,
        step,
        use_current_ws,
        ..
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = (area.width * 60 / 100).max(50);
    let popup_height = 14;
    let popup = super::clear_popup(frame, area, popup_width, popup_height);
    let active_c = app.theme.dialog.new_ws_active;
    let inactive_c = app.theme.dialog.new_ws_inactive;
    let fmax = popup.width.saturating_sub(14) as usize;

    // Truncate card title/desc for display
    let title_display = if card_title.len() > fmax {
        format!("{}…", &card_title[..fmax.saturating_sub(1)])
    } else {
        card_title.clone()
    };
    let desc_display = if card_description.len() > fmax {
        format!("{}…", &card_description[..fmax.saturating_sub(1)])
    } else {
        card_description.clone()
    };

    // Selector shows agents followed by raw dispatchable providers.
    // Indices [0..agents.len()) = agents, [agents.len()..) = providers.
    let providers = app.dispatchable_provider_list();
    let mut selector_items: Vec<String> = agents
        .iter()
        .enumerate()
        .map(|(i, (name, _, _))| {
            if i == agent_idx {
                format!("[{}]", name)
            } else {
                format!(" {} ", name)
            }
        })
        .collect();
    if !agents.is_empty() && !providers.is_empty() {
        selector_items.push("│".to_string());
    }
    for (i, p) in providers.iter().enumerate() {
        let combined_idx = agents.len() + i;
        if combined_idx == agent_idx {
            selector_items.push(format!("[{}]", p.label()));
        } else {
            selector_items.push(format!(" {} ", p.label()));
        }
    }
    let selector_text = selector_items.join(" ");

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Card:     ", Style::default().fg(inactive_c)),
            Span::styled(title_display, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Desc:     ", Style::default().fg(inactive_c)),
            Span::styled(desc_display, Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Agent:    ", Style::default().fg(active_c)),
            Span::styled(selector_text, Style::default().fg(active_c)),
        ]),
        Line::from(Span::styled(
            "              ◄/► to change",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];

    if step == 1 {
        // Step 2: workspace destination selector
        let ws_selector = if use_current_ws {
            " New   [Current]"
        } else {
            "[New]   Current "
        };
        lines.push(Line::from(vec![
            Span::styled("  Workspace:", Style::default().fg(active_c)),
            Span::styled(
                format!(" {}", ws_selector),
                Style::default().fg(active_c),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            "              ◄/► to change",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "  [Enter] Dispatch    ",
                Style::default().fg(active_c).add_modifier(Modifier::BOLD),
            ),
            Span::styled("[Esc] Back", Style::default().fg(inactive_c)),
        ]));
    } else {
        // Step 0: agent selection + prompt
        lines.push(super::render_text_field(
            "  Prompt:   ",
            additional_prompt,
            true,
            additional_prompt_cursor,
            fmax,
            super::field_style(true, active_c, inactive_c),
        ));
        lines.push(Line::from(""));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "  [Enter] Dispatch    ",
                Style::default().fg(active_c).add_modifier(Modifier::BOLD),
            ),
            Span::styled("[Esc] Cancel", Style::default().fg(inactive_c)),
        ]));
    }

    let text = Paragraph::new(lines).block(super::popup_block("Dispatch Agent", Color::Yellow));
    frame.render_widget(text, popup);
}

pub(crate) fn render_manage_agents_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::ManageAgents { selected, .. }) = app.active_dialog else {
        return;
    };

    let popup_width = (area.width * 60 / 100).max(50);
    let popup_height = (app.agent_profiles.len() as u16 + 6).min(area.height - 4).max(8);
    let popup = super::clear_popup(frame, area, popup_width, popup_height);
    let active_c = app.theme.dialog.new_ws_active;
    let inactive_c = app.theme.dialog.new_ws_inactive;

    let mut lines = vec![Line::from("")];

    if app.agent_profiles.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No agents configured. Press [n] to create one.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, agent) in app.agent_profiles.iter().enumerate() {
            let marker = if i == selected { "  ▸ " } else { "    " };
            let style = if i == selected {
                Style::default().fg(active_c).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let sync_indicator = if agent.last_synced_at.is_some() {
                Span::styled(
                    format!(" v{} ✓", agent.version),
                    Style::default().fg(Color::Green),
                )
            } else {
                Span::styled(
                    format!(" v{} ✗", agent.version),
                    Style::default().fg(Color::DarkGray),
                )
            };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{:<18}", agent.name), style),
                Span::styled(
                    format!("{:<13}", agent.provider),
                    Style::default().fg(inactive_c),
                ),
                sync_indicator,
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  [n] New  ", Style::default().fg(active_c)),
        Span::styled("[e] Edit  ", Style::default().fg(active_c)),
        Span::styled("[d] Delete  ", Style::default().fg(active_c)),
        Span::styled("[p] Sync to repo  ", Style::default().fg(active_c)),
        Span::styled("[i] Import from repo  ", Style::default().fg(active_c)),
        Span::styled("[Esc] Close", Style::default().fg(inactive_c)),
    ]));

    let project_name = app
        .current_workspace()
        .map(|ws| {
            ws.source_repo
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default();
    let title = format!("Manage Agents ({})", project_name);

    let text = Paragraph::new(lines).block(super::popup_block(&title, Color::Cyan));
    frame.render_widget(text, popup);
}

pub(crate) fn render_edit_agent_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::EditAgent {
        editing_id,
        ref name,
        name_cursor,
        provider_idx,
        active_field,
        ..
    }) = app.active_dialog
    else {
        return;
    };

    let providers = app.new_tab_agent_list();
    let popup_width = (area.width * 50 / 100).max(45);
    let popup_height = 11;
    let popup = super::clear_popup(frame, area, popup_width, popup_height);
    let active_c = app.theme.dialog.new_ws_active;
    let inactive_c = app.theme.dialog.new_ws_inactive;
    let fmax = popup.width.saturating_sub(14) as usize;

    use crate::dialog_state::EditAgentField;

    let name_active = active_field == EditAgentField::Name;
    let prov_active = active_field == EditAgentField::Provider;

    let provider_text: String = providers
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == provider_idx {
                format!("[{}]", p.label())
            } else {
                format!(" {} ", p.label())
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let prov_color = if prov_active { active_c } else { inactive_c };

    let title = if editing_id.is_some() {
        "Edit Agent (1/2)"
    } else {
        "New Agent (1/2)"
    };

    let lines = vec![
        Line::from(""),
        super::render_text_field(
            "  Name:     ",
            name,
            name_active,
            name_cursor,
            fmax,
            super::field_style(name_active, active_c, inactive_c),
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Provider: ",
                Style::default().fg(if prov_active { active_c } else { inactive_c }),
            ),
            Span::styled(provider_text, Style::default().fg(prov_color)),
        ]),
        Line::from(Span::styled(
            "              Tab switch  ◄/► provider",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [Enter] Next (edit role)    ",
                Style::default().fg(active_c).add_modifier(Modifier::BOLD),
            ),
            Span::styled("[Esc] Cancel", Style::default().fg(inactive_c)),
        ]),
    ];

    let text = Paragraph::new(lines).block(super::popup_block(title, Color::Cyan));
    frame.render_widget(text, popup);
}

pub(crate) fn render_edit_agent_role_dialog(frame: &mut Frame, area: Rect, app: &mut App) {
    let providers = app.new_tab_agent_list();

    let Some(DialogState::EditAgentRole {
        ref name,
        provider_idx,
        ref role,
        role_cursor,
        ref mut scroll,
        ..
    }) = app.active_dialog
    else {
        return;
    };
    let provider_label = providers
        .get(provider_idx)
        .map(|p| p.label())
        .unwrap_or("?");

    // Large floating editor — 80% of screen
    let popup_width = (area.width * 80 / 100).max(60);
    let popup_height = (area.height * 70 / 100).max(15);
    let popup = super::clear_popup(frame, area, popup_width, popup_height);
    let active_c = app.theme.dialog.new_ws_active;
    let inactive_c = app.theme.dialog.new_ws_inactive;

    let title = format!("Agent Role — {} ({})", name, provider_label);
    let inner_width = popup.width.saturating_sub(4) as usize;
    let inner_height = popup.height.saturating_sub(5) as usize;

    // Render role text with word wrapping and cursor
    let role_lines: Vec<&str> = role.split('\n').collect();

    // Compute cursor position in wrapped text
    let mut display_lines: Vec<String> = Vec::new();
    let mut cursor_line = 0;
    let mut cursor_col = 0;
    let mut char_count = 0;

    for line in &role_lines {
        if line.is_empty() {
            if char_count == role_cursor {
                cursor_line = display_lines.len();
                cursor_col = 0;
            }
            display_lines.push(String::new());
            char_count += 1; // count the \n
        } else {
            let chars: Vec<char> = line.chars().collect();
            for chunk in chars.chunks(inner_width.max(1)) {
                let s: String = chunk.iter().collect();
                for (i, _) in chunk.iter().enumerate() {
                    if char_count + i == role_cursor {
                        cursor_line = display_lines.len();
                        cursor_col = i;
                    }
                }
                char_count += chunk.len();
                display_lines.push(s);
            }
            char_count += 1; // count the \n
        }
    }
    // Handle cursor at end
    if role_cursor >= role.len() {
        cursor_line = display_lines.len().saturating_sub(1);
        cursor_col = display_lines.last().map(|l| l.len()).unwrap_or(0);
    }

    // Auto-scroll to keep cursor visible
    let s = *scroll;
    if cursor_line >= s + inner_height {
        *scroll = cursor_line - inner_height + 1;
    } else if cursor_line < s {
        *scroll = cursor_line;
    }
    let s = *scroll;

    let mut lines = vec![Line::from(Span::styled(
        format!("  {}/{} lines", cursor_line + 1, display_lines.len()),
        Style::default().fg(Color::DarkGray),
    ))];

    let visible_range = s..display_lines.len().min(s + inner_height);
    for (i, line_idx) in visible_range.enumerate() {
        let text = display_lines.get(line_idx).map(|s| s.as_str()).unwrap_or("");
        if line_idx == cursor_line {
            // Show cursor
            let before: String = text.chars().take(cursor_col).collect();
            let after: String = text.chars().skip(cursor_col).collect();
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(before, Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(active_c)),
                Span::styled(after, Style::default().fg(Color::White)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(text.to_string(), Style::default().fg(Color::White)),
            ]));
        }
        let _ = i;
    }

    // Pad remaining lines
    while lines.len() < inner_height + 1 {
        lines.push(Line::from(""));
    }

    // Footer
    lines.push(Line::from(""));
    let save_label = format!("  [{}] Save    ", app.config.format_binding("ctrl-s"));
    let clear_label = format!("[{}] Clear all    ", app.config.format_binding("ctrl-d"));
    lines.push(Line::from(vec![
        Span::styled(
            save_label,
            Style::default().fg(active_c).add_modifier(Modifier::BOLD),
        ),
        Span::styled(clear_label, Style::default().fg(inactive_c)),
        Span::styled("[Esc] Back", Style::default().fg(inactive_c)),
    ]));

    let text = Paragraph::new(lines).block(super::popup_block(&title, Color::Cyan));
    frame.render_widget(text, popup);
}

pub(crate) fn render_import_agents_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::ImportAgents {
        ref discovered,
        ref selected,
        cursor,
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = (area.width * 60 / 100).max(50);
    let popup_height = (discovered.len() as u16 + 6).min(area.height - 4).max(8);
    let popup = super::clear_popup(frame, area, popup_width, popup_height);
    let active_c = app.theme.dialog.new_ws_active;
    let inactive_c = app.theme.dialog.new_ws_inactive;

    let mut lines = vec![Line::from("")];

    if discovered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No agent files found in repo.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, (name, provider, _role, exists)) in discovered.iter().enumerate() {
            let checkbox = if selected.get(i).copied().unwrap_or(false) {
                "[x] "
            } else {
                "[ ] "
            };
            let marker = if i == cursor { "▸ " } else { "  " };
            let style = if i == cursor {
                Style::default().fg(active_c).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let status = if *exists {
                Span::styled(" (exists)", Style::default().fg(Color::DarkGray))
            } else {
                Span::styled(" (new)", Style::default().fg(Color::Green))
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {}{}", marker, checkbox), style),
                Span::styled(format!("{:<18}", name), style),
                Span::styled(
                    format!("{:<13}", provider),
                    Style::default().fg(inactive_c),
                ),
                status,
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  [Space] Toggle  ", Style::default().fg(active_c)),
        Span::styled("[a] All  ", Style::default().fg(active_c)),
        Span::styled("[Enter] Import  ", Style::default().fg(active_c)),
        Span::styled("[Esc] Cancel", Style::default().fg(inactive_c)),
    ]));

    let text =
        Paragraph::new(lines).block(super::popup_block("Import Agents from Repo", Color::Cyan));
    frame.render_widget(text, popup);
}
