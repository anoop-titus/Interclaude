use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, Page};

/// Draw the combined top bar: page tabs (left) + connection/session status (right)
/// This appears at the top of every page, takes 2 rows
pub fn draw_global_status(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width;

    // Build tab spans
    let mut spans = Vec::new();
    spans.push(Span::styled(" ", Style::default()));

    for (label, page, enabled) in app.page_tabs() {
        let is_active = app.page == page;

        let style = if is_active {
            Style::default().fg(Color::Black).bg(Color::Cyan).bold()
        } else if enabled {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::styled(" ", Style::default()));
    }

    // Separator
    spans.push(Span::styled("  ", Style::default()));

    // Connection status with Unicode indicators
    let conn_color = if app.connection_status.starts_with("Connected") {
        Color::Green
    } else if app.connection_status.contains("onnecting") || app.connection_status.contains("econnecting") {
        Color::Yellow
    } else if app.connection_status == "Disconnected" {
        Color::Red
    } else {
        Color::DarkGray
    };

    let conn_dot = if app.connection_status.starts_with("Connected") {
        Span::styled("● ", Style::default().fg(Color::Green))
    } else if app.connection_status.contains("onnecting") || app.connection_status.contains("econnecting") {
        // Pulsing reconnection indicator
        let pulse = if (app.frame_count / 5) % 2 == 0 { "◌ " } else { "● " };
        Span::styled(pulse, Style::default().fg(Color::Yellow))
    } else {
        Span::styled("○ ", Style::default().fg(Color::Red))
    };

    let sess_color = match app.session_status.as_str() {
        "Active" => Color::Green,
        "Inactive" => Color::DarkGray,
        _ if app.session_status.starts_with("Active") => Color::Green,
        _ => Color::DarkGray,
    };

    let sess_dot = if app.session_status.starts_with("Active") {
        Span::styled("● ", Style::default().fg(Color::Green))
    } else {
        Span::styled("○ ", Style::default().fg(Color::DarkGray))
    };

    if width >= 60 {
        spans.push(Span::styled("Conn:", Style::default().fg(Color::White)));
        spans.push(conn_dot);
        spans.push(Span::styled(&app.connection_status, Style::default().fg(conn_color)));

        // Session duration
        if let Some(duration) = app.session_duration() {
            spans.push(Span::styled(
                format!(" [{}]", duration),
                Style::default().fg(Color::DarkGray),
            ));
        }

        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled("Sess:", Style::default().fg(Color::White)));
        spans.push(sess_dot);
        spans.push(Span::styled(&app.session_status, Style::default().fg(sess_color)));
    } else {
        // Narrow: Unicode dots instead of text
        spans.push(conn_dot);
        spans.push(Span::styled(
            if app.connection_status.starts_with("Connected") { "●" } else { "○" },
            Style::default().fg(conn_color),
        ));
        spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
        spans.push(sess_dot);
        spans.push(Span::styled(
            if app.session_status.starts_with("Active") { "●" } else { "○" },
            Style::default().fg(sess_color),
        ));
    }

    let bar = Paragraph::new(Line::from(spans))
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(bar, area);
}

/// Handle mouse click on tab bar — returns Some(page) if a tab was clicked
pub fn handle_tab_click(app: &App, col: u16) -> Option<Page> {
    let mut pos: u16 = 1;
    for (label, page, enabled) in app.page_tabs() {
        let tab_width = label.len() as u16 + 2;
        if col >= pos && col < pos + tab_width {
            if enabled {
                return Some(page);
            } else {
                return None;
            }
        }
        pos += tab_width + 1;
    }
    None
}
