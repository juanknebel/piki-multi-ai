use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::App;

pub(crate) fn render_chat_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;

    // Centered floating panel
    let width = (area.width * 80 / 100).max(50).min(area.width);
    let height = (area.height * 80 / 100).max(15).min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let popup = Rect::new(x, y, width.min(area.width), height.min(area.height));

    frame.render_widget(Clear, popup);

    // Model indicator in title
    let model_label = if app.chat_panel.config.model.is_empty() {
        "no model".to_string()
    } else {
        app.chat_panel.config.model.clone()
    };
    let title = format!(" AI Chat [{}] ", model_label);

    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(theme.help.border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.help.border));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 4 {
        return;
    }

    // Layout: messages area | input line | footer hints
    let input_height = 1;
    let footer_height = 1;
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(input_height),
        Constraint::Length(footer_height),
    ])
    .split(inner);

    let messages_area = chunks[0];
    let input_area = chunks[1];
    let footer_area = chunks[2];

    // ── Sub-mode content ─────────────────────────
    match app.chat_panel.sub_mode {
        crate::app::ChatSubMode::ModelSelect => {
            render_model_selector(frame, messages_area, app);
        }
        crate::app::ChatSubMode::Settings => {
            render_settings(frame, messages_area, app);
        }
        crate::app::ChatSubMode::Chat => {
            render_messages(frame, messages_area, app);
        }
    }

    // ── Input line ─────────────────────────────
    let input_text = &app.chat_panel.input;
    let cursor = app.chat_panel.input_cursor;
    let max_width = input_area.width.saturating_sub(2) as usize;

    // Simple visible window around cursor
    let visible_start = cursor.saturating_sub(max_width);
    let visible_end = (visible_start + max_width).min(input_text.len());
    let visible = &input_text[visible_start..visible_end];

    let cursor_pos = cursor - visible_start;
    let before = &visible[..cursor_pos.min(visible.len())];
    let cursor_char = visible.get(cursor_pos..cursor_pos + 1).unwrap_or(" ");
    let after_start = (cursor_pos + 1).min(visible.len());
    let after = &visible[after_start..];

    let input_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.help.border)),
        Span::raw(before),
        Span::styled(cursor_char, Style::default().bg(theme.help.border).fg(Color::Black)),
        Span::raw(after),
    ]);
    frame.render_widget(Paragraph::new(input_line), input_area);

    // ── Footer hints ───────────────────────────
    let h = theme.help.border;
    let hints: Vec<Span<'_>> = match app.chat_panel.sub_mode {
        crate::app::ChatSubMode::Settings => vec![
            Span::styled("[Tab]", Style::default().fg(h)),
            Span::raw(" switch field  "),
            Span::styled("[Ctrl+Enter]", Style::default().fg(h)),
            Span::raw(" save  "),
            Span::styled("[Esc]", Style::default().fg(h)),
            Span::raw(" cancel"),
        ],
        crate::app::ChatSubMode::ModelSelect => vec![
            Span::styled("[j/k]", Style::default().fg(h)),
            Span::raw(" navigate  "),
            Span::styled("[Enter]", Style::default().fg(h)),
            Span::raw(" select  "),
            Span::styled("[Esc]", Style::default().fg(h)),
            Span::raw(" cancel"),
        ],
        crate::app::ChatSubMode::Chat if app.chat_panel.streaming => vec![
            Span::styled("streaming", Style::default().fg(h)),
            Span::raw("..."),
        ],
        crate::app::ChatSubMode::Chat => vec![
            Span::styled("[Enter]", Style::default().fg(h)),
            Span::raw(" send  "),
            Span::styled("[Tab]", Style::default().fg(h)),
            Span::raw(" model  "),
            Span::styled("[C-o]", Style::default().fg(h)),
            Span::raw(" settings  "),
            Span::styled("[C-l]", Style::default().fg(h)),
            Span::raw(" clear  "),
            Span::styled("[Esc]", Style::default().fg(h)),
            Span::raw(" hide"),
        ],
    };
    frame.render_widget(Paragraph::new(Line::from(hints)), footer_area);
}

fn render_messages(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }

    let theme = &app.theme;
    let visible_height = area.height as usize;

    // Build display lines from messages + streaming
    let mut lines: Vec<Line<'_>> = Vec::new();

    if app.chat_panel.messages.is_empty() && !app.chat_panel.streaming {
        // Empty state
        let empty_y = visible_height / 2;
        for _ in 0..empty_y.saturating_sub(1) {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(
            Span::styled(
                "Start a conversation with a local AI model",
                Style::default().fg(theme.general.muted_text),
            ),
        ));
        lines.push(Line::from(
            Span::styled(
                "Press Tab to select a model",
                Style::default().fg(theme.general.muted_text),
            ),
        ));
    } else {
        for msg in &app.chat_panel.messages {
            let (role_label, role_color) = match msg.role {
                piki_core::chat::ChatRole::User => ("You", theme.help.border),
                piki_core::chat::ChatRole::Assistant => ("AI", theme.workspace_list.name_active),
                piki_core::chat::ChatRole::System => ("System", theme.workspace_list.detail_normal),
            };

            lines.push(Line::from(Span::styled(
                format!("  {role_label}"),
                Style::default().fg(role_color),
            )));

            // Wrap content manually at area width
            let content_width = area.width.saturating_sub(4) as usize;
            for content_line in msg.content.lines() {
                for chunk in wrap_text(content_line, content_width) {
                    lines.push(Line::from(format!("    {chunk}")));
                }
            }
            lines.push(Line::from(""));
        }

        // Streaming response in progress
        if app.chat_panel.streaming && !app.chat_panel.current_response.is_empty() {
            lines.push(Line::from(Span::styled(
                "  AI",
                Style::default().fg(theme.workspace_list.name_active),
            )));
            let content_width = area.width.saturating_sub(4) as usize;
            for content_line in app.chat_panel.current_response.lines() {
                for chunk in wrap_text(content_line, content_width) {
                    lines.push(Line::from(format!("    {chunk}")));
                }
            }
            // Blinking cursor effect
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("_", Style::default().fg(theme.help.border)),
            ]));
        }
    }

    // Auto-scroll to bottom (scroll=0 means bottom)
    let total = lines.len();
    let scroll = app.chat_panel.scroll;
    let skip = if total > visible_height + scroll {
        total - visible_height - scroll
    } else {
        0
    };

    let visible_lines: Vec<Line<'_>> = lines.into_iter().skip(skip).take(visible_height).collect();
    let para = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn render_model_selector(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let models = &app.chat_panel.models;
    let selected = app.chat_panel.model_selected;
    let visible_height = area.height as usize;

    let mut lines: Vec<Line<'_>> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Select a model:",
        Style::default().fg(theme.help.border),
    )));
    lines.push(Line::from(""));

    for (i, model) in models.iter().enumerate() {
        let is_selected = i == selected;
        let marker = if is_selected { "> " } else { "  " };
        let is_current = *model == app.chat_panel.config.model;
        let suffix = if is_current { " (current)" } else { "" };

        let style = if is_selected {
            Style::default()
                .fg(theme.help.border)
                .bg(theme.workspace_list.selected_bg)
        } else {
            Style::default().fg(Color::Reset)
        };

        lines.push(Line::from(Span::styled(
            format!("{marker}{model}{suffix}"),
            style,
        )));
    }

    let visible: Vec<Line<'_>> = lines.into_iter().take(visible_height).collect();
    frame.render_widget(Paragraph::new(visible), area);
}

fn render_settings(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let h = theme.help.border;
    let active_field = app.chat_panel.settings_field;
    let cursor = app.chat_panel.settings_cursor;
    let max_w = area.width.saturating_sub(4) as usize;

    let mut lines: Vec<Line<'_>> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Chat Settings",
        Style::default().fg(h),
    )));
    lines.push(Line::from(""));

    // Base URL field
    let url_active = active_field == crate::app::ChatSettingsField::BaseUrl;
    let url_label_style = if url_active {
        Style::default().fg(h)
    } else {
        Style::default().fg(theme.general.muted_text)
    };
    lines.push(Line::from(Span::styled("  Base URL:", url_label_style)));
    lines.push(render_text_field(
        &app.chat_panel.settings_url,
        if url_active { Some(cursor) } else { None },
        max_w,
        h,
    ));
    lines.push(Line::from(""));

    // System prompt field
    let prompt_active = active_field == crate::app::ChatSettingsField::SystemPrompt;
    let prompt_label_style = if prompt_active {
        Style::default().fg(h)
    } else {
        Style::default().fg(theme.general.muted_text)
    };
    lines.push(Line::from(Span::styled(
        "  System prompt:",
        prompt_label_style,
    )));
    let prompt_text = &app.chat_panel.settings_prompt;
    if prompt_text.is_empty() && !prompt_active {
        lines.push(Line::from(Span::styled(
            "    (none)",
            Style::default().fg(theme.general.muted_text),
        )));
    } else {
        // Show potentially multi-line prompt, but only the first line with cursor
        for (i, pline) in prompt_text.lines().enumerate() {
            if i == 0 {
                lines.push(render_text_field(
                    pline,
                    if prompt_active { Some(cursor.min(pline.len())) } else { None },
                    max_w,
                    h,
                ));
            } else {
                lines.push(Line::from(format!("    {pline}")));
            }
        }
        if prompt_text.is_empty() && prompt_active {
            lines.push(render_text_field("", Some(0), max_w, h));
        }
    }

    let visible_height = area.height as usize;
    let visible: Vec<Line<'_>> = lines.into_iter().take(visible_height).collect();
    frame.render_widget(Paragraph::new(visible), area);
}

fn render_text_field<'a>(
    text: &'a str,
    cursor_pos: Option<usize>,
    max_width: usize,
    accent: Color,
) -> Line<'a> {
    let prefix = "    ";
    match cursor_pos {
        Some(pos) => {
            let visible_start = pos.saturating_sub(max_width);
            let visible_end = (visible_start + max_width).min(text.len());
            let visible = &text[visible_start..visible_end];
            let cp = pos - visible_start;
            let before = &visible[..cp.min(visible.len())];
            let cursor_char = visible.get(cp..cp + 1).unwrap_or(" ");
            let after_start = (cp + 1).min(visible.len());
            let after = &visible[after_start..];
            Line::from(vec![
                Span::raw(prefix),
                Span::raw(before.to_string()),
                Span::styled(cursor_char.to_string(), Style::default().bg(accent).fg(Color::Black)),
                Span::raw(after.to_string()),
            ])
        }
        None => {
            Line::from(format!("{prefix}{text}"))
        }
    }
}

/// Simple word-wrapping: split text at `width` boundaries.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    let mut remaining = text;
    while remaining.len() > width {
        // Try to find a word break
        let break_at = remaining[..width]
            .rfind(' ')
            .map(|p| p + 1)
            .unwrap_or(width);
        result.push(remaining[..break_at].to_string());
        remaining = &remaining[break_at..];
    }
    result.push(remaining.to_string());
    result
}
