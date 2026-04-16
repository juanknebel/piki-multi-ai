mod agents;
mod dashboard;
mod git;
mod providers;
mod system;
mod workspace;

// Re-export all dialog render functions
pub(crate) use self::agents::{
    render_dispatch_agent_dialog, render_dispatch_card_move_dialog, render_edit_agent_dialog,
    render_edit_agent_role_dialog, render_import_agents_dialog, render_manage_agents_dialog,
};
pub(crate) use self::dashboard::render_dashboard_overlay;
pub(crate) use self::git::{
    render_commit_dialog, render_confirm_merge_dialog, render_conflict_resolution_overlay,
    render_git_log_overlay, render_git_stash_overlay,
};
pub(crate) use self::providers::{render_edit_provider_dialog, render_manage_providers_dialog};
pub(crate) use self::system::{
    render_about_overlay, render_confirm_close_tab_dialog, render_confirm_quit_dialog,
    render_diff_overlay, render_help_overlay, render_logs_overlay, render_new_tab_dialog,
};
pub(crate) use self::workspace::{
    render_confirm_delete_dialog, render_edit_workspace_dialog, render_new_workspace_dialog,
    render_workspace_info_overlay,
};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

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
