use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{ActivePane, App, AppMode};

use super::layout::{pane_border_style, pane_title_style};

pub(super) fn render_subtabs(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(ws) = app.current_workspace() {
        super::subtabs::render(frame, area, ws, &app.theme);
    } else {
        let block = Block::default().borders(Borders::BOTTOM);
        frame.render_widget(block, area);
    }
}

pub(super) fn render_main_content(frame: &mut Frame, area: Rect, app: &mut App) {
    if app.mode == AppMode::InlineEdit {
        super::editor::render(frame, area, app);
        return;
    }

    let border_style = pane_border_style(app, ActivePane::MainPanel);
    let title_style = pane_title_style(app, ActivePane::MainPanel);
    app.api_response_inner_area = None;

    let selection = app.selection.take();
    let selection_style = Style::default()
        .bg(app.theme.selection.bg)
        .fg(app.theme.selection.fg);
    if let Some(ws) = app.current_workspace() {
        if let Some(tab) = ws.current_tab() {
            // Markdown tab — use cached parsed text when available
            if tab.markdown_content.is_some() {
                if let Some(ref rendered) = tab.markdown_rendered {
                    let label = tab.markdown_label.as_deref().unwrap_or("markdown");
                    let scroll = tab.markdown_scroll;
                    super::markdown::render_cached(
                        frame,
                        area,
                        rendered,
                        label,
                        scroll,
                        border_style,
                        title_style,
                        app.theme.general.scrollbar_thumb,
                    );
                } else if let (Some(content), Some(label)) =
                    (&tab.markdown_content, &tab.markdown_label)
                {
                    let content = content.clone();
                    let label = label.clone();
                    let scroll = tab.markdown_scroll;
                    super::markdown::render(
                        frame,
                        area,
                        &content,
                        &label,
                        scroll,
                        border_style,
                        title_style,
                        Some(&app.syntax),
                        app.theme.general.scrollbar_thumb,
                    );
                }
                return;
            }

            let provider = &tab.provider;

            if *provider == crate::app::AIProvider::CodeReview {
                // Code review has its own full-screen layout; show a placeholder here
                let block = Block::default()
                    .title(" Code Review ")
                    .title_style(title_style)
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(border_style);
                let text = Paragraph::new("  Code Review renders in full-screen mode")
                    .style(Style::default().fg(app.theme.general.muted_text))
                    .block(block);
                frame.render_widget(text, area);
                return;
            }

            if *provider == crate::app::AIProvider::Api {
                if let Some(ref api) = tab.api_state {
                    let response_inner = super::api::render(
                        frame,
                        area,
                        api,
                        &app.theme,
                        border_style,
                        title_style,
                        selection.as_ref(),
                        selection_style,
                    );
                    app.api_response_inner_area = response_inner;
                    // Override terminal_inner_area so mouse selection coordinates work
                    if let Some(inner) = response_inner {
                        app.terminal_inner_area = Some(inner);
                    }
                }
                app.selection = selection;
                return;
            }

            if let Some(ref parser) = tab.pty_parser {
                super::terminal::render(
                    frame,
                    area,
                    parser,
                    border_style,
                    title_style,
                    tab.display_label(),
                    tab.term_scroll,
                    selection.as_ref(),
                    selection_style,
                    app.term_search.as_ref(),
                    app.theme.general.scrollbar_thumb,
                    &app.theme.palette,
                );
            } else {
                // Provider CLI not found — show fun ASCII art
                let block = Block::default()
                    .title(format!(" {} ", tab.display_label()))
                    .title_style(title_style)
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(border_style);
                let cmd = provider.command();
                let new_tab = app.config.get_binding("app", "new_tab");
                let ascii_art = format!(
                    r#"
        ___________________
       /                   \
      |   Command not found |
      |   `{cmd}` is not    |
      |   installed :-(     |
       \___________________/
              \
               \    _(o o)_
                \  / \.-./ \
                  ##  | |  ##
                     _| |_
                    (_____)

    Install `{cmd}` and add it to your PATH
    then press [{new_tab}] to open a new tab."#
                );
                let text = Paragraph::new(ascii_art)
                    .style(Style::default().fg(app.theme.general.muted_text))
                    .block(block);
                frame.render_widget(text, area);
            }
        } else {
            // No tabs yet — centered hints
            let block = Block::default()
                .title(" Terminal ")
                .title_style(title_style)
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(border_style);
            let key_style = Style::default().fg(app.theme.footer.key);
            let desc_style = Style::default().fg(app.theme.general.muted_text);
            let content_lines: Vec<Line> = vec![
                Line::from(Span::styled("No tabs open", desc_style)),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        format!("[{}]", app.config.get_binding("app", "new_tab")),
                        key_style,
                    ),
                    Span::styled(" Open a new tab", desc_style),
                ]),
                Line::from(vec![
                    Span::styled(
                        format!("[{}]", app.config.get_binding("app", "command_palette")),
                        key_style,
                    ),
                    Span::styled(" Search commands", desc_style),
                ]),
            ];
            let content_height = content_lines.len() as u16;
            let inner_height = area.height.saturating_sub(2);
            let pad = inner_height.saturating_sub(content_height) / 2;
            let mut lines: Vec<Line> = vec![Line::from(""); pad as usize];
            lines.extend(content_lines);
            let text = Paragraph::new(lines)
                .alignment(ratatui::layout::Alignment::Center)
                .block(block);
            frame.render_widget(text, area);
        }
    } else {
        let block = Block::default()
            .title(" piki-multi-ai ")
            .title_style(title_style)
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(border_style);
        let key_style = Style::default().fg(app.theme.footer.key);
        let desc_style = Style::default().fg(app.theme.general.welcome_text);
        let title_style = Style::default()
            .fg(app.theme.palette.fg0)
            .add_modifier(Modifier::BOLD);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("  Welcome to piki-multi-ai", title_style)),
            Line::from(""),
            Line::from(Span::styled("  Quick Start:", desc_style)),
            Line::from(""),
            Line::from(vec![
                Span::styled("  1. ", desc_style),
                Span::styled(
                    format!("[{}]", app.config.get_binding("app", "new_workspace")),
                    key_style,
                ),
                Span::styled(" Add your first repository", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  2. ", desc_style),
                Span::styled(
                    format!("[{}]", app.config.get_binding("app", "workspace_switcher")),
                    key_style,
                ),
                Span::styled(" Switch between checkouts", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  3. ", desc_style),
                Span::styled("[Enter]", key_style),
                Span::styled(" Interact with the terminal", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  4. ", desc_style),
                Span::styled(
                    format!("[{}]", app.config.get_binding("app", "help")),
                    key_style,
                ),
                Span::styled(" Full help", desc_style),
            ]),
        ];
        let text = Paragraph::new(lines).block(block);
        frame.render_widget(text, area);
    }
    app.selection = selection;
}
