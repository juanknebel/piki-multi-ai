use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::dialog_state::{DialogState, EditProviderField};

pub(crate) fn render_manage_providers_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::ManageProviders { selected, .. }) = app.active_dialog else {
        return;
    };

    let providers = app.provider_manager.all();
    let popup_width = (area.width * 60 / 100).max(55);
    let popup_height = (providers.len() as u16 + 6).min(area.height - 4).max(8);
    let popup = super::clear_popup(frame, area, popup_width, popup_height);
    let active_c = app.theme.dialog.new_ws_active;
    let inactive_c = app.theme.dialog.new_ws_inactive;

    let mut lines = vec![Line::from("")];

    if providers.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No providers configured. Press [n] to create one.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, config) in providers.iter().enumerate() {
            let marker = if i == selected { "  ▸ " } else { "    " };
            let style = if i == selected {
                Style::default().fg(active_c).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let dispatch_marker = if config.dispatchable { " ✓" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{:<20}", config.name), style),
                Span::styled(
                    format!("{:<16}", config.command),
                    Style::default().fg(inactive_c),
                ),
                Span::styled(
                    dispatch_marker,
                    Style::default().fg(Color::Green),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  [n] New  ", Style::default().fg(active_c)),
        Span::styled("[e] Edit  ", Style::default().fg(active_c)),
        Span::styled("[d] Delete  ", Style::default().fg(active_c)),
        Span::styled("[Esc] Close", Style::default().fg(inactive_c)),
    ]));

    let text = Paragraph::new(lines).block(super::popup_block("Manage Providers", Color::Cyan));
    frame.render_widget(text, popup);
}

pub(crate) fn render_edit_provider_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::EditProvider {
        ref original_name,
        ref name,
        name_cursor,
        ref description,
        desc_cursor,
        ref command,
        command_cursor,
        ref default_args,
        args_cursor,
        prompt_format_idx,
        ref prompt_flag,
        flag_cursor,
        dispatchable,
        ref agent_dir,
        agent_dir_cursor,
        active_field,
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = (area.width * 60 / 100).max(55);
    let popup = super::clear_popup(frame, area, popup_width, 18);
    let active_c = app.theme.dialog.new_ws_active;
    let inactive_c = app.theme.dialog.new_ws_inactive;

    let title = if original_name.is_some() {
        "Edit Provider"
    } else {
        "New Provider"
    };

    let prompt_labels = ["Positional", "Flag", "None"];
    let fw = popup_width.saturating_sub(20) as usize; // field width for visible_field

    let text_field = |label: &str, value: &str, cursor: usize, field_id: EditProviderField| -> Line<'_> {
        let is_active = active_field == field_id;
        let label_style = Style::default().fg(if is_active { active_c } else { inactive_c });
        let display = super::visible_field(value, is_active, cursor, fw);
        Line::from(vec![
            Span::styled(format!("  {label:<15}"), label_style),
            Span::styled(display, Style::default().fg(Color::White)),
        ])
    };

    let selector = |label: &str, display: &str, field_id: EditProviderField| -> Line<'_> {
        let is_active = active_field == field_id;
        let label_style = Style::default().fg(if is_active { active_c } else { inactive_c });
        let val_style = if is_active {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        Line::from(vec![
            Span::styled(format!("  {label:<15}"), label_style),
            Span::styled(format!("◄ {display} ►"), val_style),
        ])
    };

    let lines = vec![
        Line::from(""),
        text_field("Name:", name, name_cursor, EditProviderField::Name),
        text_field("Description:", description, desc_cursor, EditProviderField::Description),
        text_field("Command:", command, command_cursor, EditProviderField::Command),
        text_field("Default Args:", default_args, args_cursor, EditProviderField::DefaultArgs),
        selector("Prompt Format:", prompt_labels[prompt_format_idx], EditProviderField::PromptFormat),
        text_field("Flag (if Flag):", prompt_flag, flag_cursor, EditProviderField::PromptFlag),
        selector("Dispatchable:", if dispatchable { "Yes" } else { "No" }, EditProviderField::Dispatchable),
        text_field("Agent Dir:", agent_dir, agent_dir_cursor, EditProviderField::AgentDir),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [Ctrl+S] Save  ", Style::default().fg(active_c)),
            Span::styled("[Tab] Next field  ", Style::default().fg(inactive_c)),
            Span::styled("[Esc] Cancel", Style::default().fg(inactive_c)),
        ]),
    ];

    let text = Paragraph::new(lines).block(super::popup_block(title, Color::Cyan));
    frame.render_widget(text, popup);
}
