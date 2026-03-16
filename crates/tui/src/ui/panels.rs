use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{ActivePane, App, AppMode};

use super::layout::pane_border_style;

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
                    );
                } else if let (Some(content), Some(label)) =
                    (&tab.markdown_content, &tab.markdown_label)
                {
                    let content = content.clone();
                    let label = label.clone();
                    let scroll = tab.markdown_scroll;
                    super::markdown::render(frame, area, &content, &label, scroll, border_style);
                }
                return;
            }

            let provider = tab.provider;

            if provider == crate::app::AIProvider::CodeReview {
                // Code review has its own full-screen layout; show a placeholder here
                let block = Block::default()
                    .title(" Code Review ")
                    .title_style(border_style)
                    .borders(Borders::ALL)
                    .border_style(border_style);
                let text = Paragraph::new("  Code Review renders in full-screen mode")
                    .style(Style::default().fg(app.theme.general.muted_text))
                    .block(block);
                frame.render_widget(text, area);
                return;
            }

            if provider == crate::app::AIProvider::Api {
                if let Some(ref api) = tab.api_state {
                    let response_inner = super::api::render(
                        frame,
                        area,
                        api,
                        border_style,
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


            if provider == crate::app::AIProvider::Kanban {
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                    && let Some(kanban_app) = ws.kanban_app.as_mut()
                {
                    // Take edit_state so flow doesn't render its own overlay
                    let edit_state = kanban_app.edit_state.take();

                    let block = Block::default()
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    let inner_area = block.inner(area);
                    frame.render_widget(block, area);
                    flow::ui::render(frame, kanban_app, Some(inner_area));

                    // Restore and render our own overlay with proper cursor
                    if let Some(edit) = edit_state {
                        kanban_app.edit_state = Some(edit);
                        render_kanban_edit(
                            frame,
                            inner_area,
                            kanban_app.edit_state.as_ref().unwrap(),
                        );
                    }
                }
                return;
            }

            if let Some(ref parser) = tab.pty_parser {
                super::terminal::render(
                    frame,
                    area,
                    parser,
                    border_style,
                    provider.label(),
                    tab.term_scroll,
                    selection.as_ref(),
                    selection_style,
                    app.term_search.as_ref(),
                );
            } else {
                // Provider CLI not found — show fun ASCII art
                let block = Block::default()
                    .title(format!(" {} ", provider.label()))
                    .title_style(border_style)
                    .borders(Borders::ALL)
                    .border_style(border_style);
                let cmd = provider.command();
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
    then press [t] to open a new tab."#
                );
                let text = Paragraph::new(ascii_art)
                    .style(Style::default().fg(app.theme.general.muted_text))
                    .block(block);
                frame.render_widget(text, area);
            }
        } else {
            // No tabs yet
            let block = Block::default()
                .title(" Terminal ")
                .title_style(border_style)
                .borders(Borders::ALL)
                .border_style(border_style);
            let text = Paragraph::new("  Press [t] to open a new tab")
                .style(Style::default().fg(app.theme.general.muted_text))
                .block(block);
            frame.render_widget(text, area);
        }
    } else {
        let block = Block::default()
            .title(" piki-multi-ai ")
            .title_style(border_style)
            .borders(Borders::ALL)
            .border_style(border_style);
        let text = Paragraph::new(
            "  Welcome to piki-multi-ai\n\n  Press [n] to create a new workspace\n  Press [?] for help\n  Press [q] to quit",
        )
        .style(Style::default().fg(app.theme.general.welcome_text))
        .block(block);
        frame.render_widget(text, area);
    }
    app.selection = selection;
}

fn priority_color(p: flow::Priority) -> Color {
    match p {
        flow::Priority::Bug => Color::Red,
        flow::Priority::High => Color::Yellow,
        flow::Priority::Medium => Color::White,
        flow::Priority::Low => Color::DarkGray,
        flow::Priority::Wishlist => Color::Cyan,
    }
}

fn render_kanban_edit(f: &mut Frame, parent: Rect, edit: &flow::app::EditState) {
    use flow::app::EditFocus;

    let area = flow::ui::centered(70, 60, parent);
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("Editing "),
            Span::styled(&edit.card_id, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        chunks[0],
    );

    let title_style = if edit.focus == EditFocus::Title {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    f.render_widget(
        Paragraph::new(edit.title.clone()).block(
            Block::default()
                .title("Title")
                .borders(Borders::ALL)
                .border_style(title_style),
        ),
        chunks[1],
    );

    let priority_focused = edit.focus == EditFocus::Priority;
    let priority_style = if priority_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let priority_spans = vec![
        Span::raw(" "),
        Span::styled(
            format!(" {} ", edit.priority.label()),
            Style::default()
                .fg(priority_color(edit.priority))
                .add_modifier(Modifier::BOLD),
        ),
        if priority_focused {
            Span::styled("  ←/→ to change", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw("")
        },
    ];
    f.render_widget(
        Paragraph::new(Line::from(priority_spans)).block(
            Block::default()
                .title("Priority")
                .borders(Borders::ALL)
                .border_style(priority_style),
        ),
        chunks[2],
    );

    let desc_style = if edit.focus == EditFocus::Description {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let inner_width = chunks[3].width.saturating_sub(2).max(1) as usize;
    let wrapped: Vec<Line> = char_wrap(&edit.description, inner_width)
        .into_iter()
        .map(Line::from)
        .collect();
    f.render_widget(
        Paragraph::new(wrapped).block(
            Block::default()
                .title("Description")
                .borders(Borders::ALL)
                .border_style(desc_style),
        ),
        chunks[3],
    );

    f.render_widget(
        Paragraph::new("Tab: switch field  ←/→: priority  Enter: save  Esc: cancel")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center),
        chunks[4],
    );

    // Position cursor
    match edit.focus {
        EditFocus::Title => {
            let char_pos = edit.title[..edit.cursor_pos.min(edit.title.len())]
                .chars()
                .count();
            f.set_cursor_position((chunks[1].x + 1 + char_pos as u16, chunks[1].y + 1));
        }
        EditFocus::Priority => {
            // No text cursor for priority field
        }
        EditFocus::Description => {
            let char_pos = edit.description[..edit.cursor_pos.min(edit.description.len())]
                .chars()
                .count();
            let (row, col) = cursor_visual_pos(&edit.description, char_pos, inner_width);
            f.set_cursor_position((chunks[3].x + 1 + col, chunks[3].y + 1 + row));
        }
    }

    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title("Edit Card")
            .border_style(Style::default().fg(Color::Cyan)),
        area,
    );
}

/// Wrap text at character boundaries, respecting explicit newlines.
fn char_wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for segment in text.split('\n') {
        if segment.is_empty() {
            lines.push(String::new());
        } else {
            let chars: Vec<char> = segment.chars().collect();
            for chunk in chars.chunks(width) {
                lines.push(chunk.iter().collect());
            }
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Compute the visual (row, col) for a cursor position in char-wrapped text.
fn cursor_visual_pos(text: &str, cursor: usize, width: usize) -> (u16, u16) {
    let mut row: u16 = 0;
    let mut col: usize = 0;
    for (i, ch) in text.chars().enumerate() {
        if ch != '\n' && col >= width {
            row += 1;
            col = 0;
        }
        if i == cursor {
            return (row, col as u16);
        }
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    // Cursor at end of text
    if col >= width {
        row += 1;
        col = 0;
    }
    (row, col as u16)
}
