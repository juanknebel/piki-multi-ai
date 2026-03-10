use std::time::SystemTime;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{PomodoroState, Workspace};

pub fn render(frame: &mut Frame, area: Rect, ws: &Workspace, state: &PomodoroState, border_style: Style) {
    let mut bg_style = Style::default();
    if state.alert {
        // Flash every 500ms
        let millis = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        if (millis / 500) % 2 == 0 {
            bg_style = bg_style.bg(Color::Red);
        } else {
            bg_style = bg_style.bg(Color::Yellow);
        }
    }

    let block = Block::default()
        .title(" Pomodoro Timer ")
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(bg_style);

    let inner_area = block.inner(area);
    frame.render_widget(Clear, area); // Clear with bg
    frame.render_widget(block, area);

    let chunks: [Rect; 5] = Layout::vertical([
        Constraint::Length(3), // Phase
        Constraint::Length(5), // Timer
        Constraint::Length(3), // Info
        Constraint::Min(0),    // Help/Buttons
        Constraint::Length(6), // Stats
    ])
    .areas(inner_area);

    // Phase
    let phase_label = if state.alert {
        format!("FINISHED: {}", state.phase.label())
    } else {
        state.phase.label().to_string()
    };
    
    let mut phase_style = match state.phase {
        crate::app::PomodoroPhase::Work => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        crate::app::PomodoroPhase::ShortBreak => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        crate::app::PomodoroPhase::LongBreak => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
    };

    if state.alert {
        phase_style = Style::default().fg(Color::Black).add_modifier(Modifier::BOLD);
    }

    let phase_para = Paragraph::new(Line::from(vec![
        Span::raw("Phase: "),
        Span::styled(phase_label, phase_style),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(phase_para, chunks[0]);

    // Timer
    let minutes = state.remaining_seconds / 60;
    let seconds = state.remaining_seconds % 60;
    let time_str = format!("{:02}:{:02}", minutes, seconds);
    
    // Simple big-ish text for timer
    let timer_style = if state.alert {
        Style::default().fg(Color::Black).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };

    let timer_para = Paragraph::new(time_str)
        .style(timer_style)
        .alignment(Alignment::Center);
    frame.render_widget(timer_para, chunks[1]);

    // Info
    let info_line = Line::from(vec![
        Span::raw(format!("Cycle: {}/{}", state.current_cycle, state.cycles_until_long)),
    ]);
    let info_para = Paragraph::new(info_line).alignment(Alignment::Center);
    frame.render_widget(info_para, chunks[2]);

    // Help
    let status_str = if state.is_running { "Running" } else { "Paused" };
    let help_lines = vec![
        Line::from(format!("Status: {}", status_str)),
        Line::from(""),
        Line::from("[s] Start/Pause  [r] Reset"),
    ];
    let help_para = Paragraph::new(help_lines)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));
    frame.render_widget(help_para, chunks[3]);

    // Stats
    let stats = &ws.pomodoro_stats;
    let stats_lines = vec![
        Line::from(vec![
            Span::styled("--- STATISTICS (This Workspace) ---", Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(format!("Total Work Sessions: {}", stats.work_sessions)),
        Line::from(format!("Total Work Minutes:  {}", stats.total_work_minutes)),
        Line::from(format!("Short Breaks:        {}", stats.short_breaks)),
        Line::from(format!("Long Breaks:         {}", stats.long_breaks)),
    ];
    let stats_para = Paragraph::new(stats_lines)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(stats_para, chunks[4]);
}
