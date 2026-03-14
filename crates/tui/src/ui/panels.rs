use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph};

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

            if provider == crate::app::AIProvider::Kanban {
                if let Some(ws) = app.workspaces.get(app.active_workspace)
                    && let Some(kanban_app) = &ws.kanban_app
                {
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    let inner_area = block.inner(area);
                    frame.render_widget(block, area);
                    flow::ui::render(frame, kanban_app, Some(inner_area));
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
