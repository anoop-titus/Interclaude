use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, Page};

/// Draw the combined top bar: page tabs (left) + status indicators (right)
/// This appears at the top of every page, takes 2 rows
pub fn draw_global_status(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width as usize;

    // Build tab spans
    let mut spans = Vec::new();
    spans.push(Span::styled(" ", Style::default()));

    let mut used_width: usize = 1;

    for (label, page, enabled) in app.page_tabs() {
        let is_active = app.page == page;

        let style = if is_active {
            Style::default().fg(Color::Black).bg(Color::Cyan).bold()
        } else if enabled {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let tab_text = format!(" {label} ");
        used_width += tab_text.len() + 1; // +1 for separator space
        spans.push(Span::styled(tab_text, style));
        spans.push(Span::styled(" ", Style::default()));
    }

    // Separator
    spans.push(Span::styled("  ", Style::default()));
    used_width += 2;

    // ERE status indicator (always shown)
    let error_count = app.error_store.all_recent().len();
    let has_overlay = app.active_error_overlay.is_some();

    let ere_span = if has_overlay {
        // Pulsing indicator when overlay is active
        let pulse = if (app.frame_count / 5) % 2 == 0 { "⚡" } else { "◆" };
        Span::styled(
            format!("ERE:{pulse}analyzing "),
            Style::default().fg(Color::Yellow),
        )
    } else if error_count > 0 {
        Span::styled(
            format!("ERE:{}err ", error_count),
            Style::default().fg(Color::Red),
        )
    } else {
        Span::styled("ERE:ok ", Style::default().fg(Color::Green))
    };
    used_width += ere_span.width();
    spans.push(ere_span);

    spans.push(Span::styled("| ", Style::default().fg(Color::DarkGray)));
    used_width += 2;

    // Connection + session status (truncated to fit)
    let conn_dot = if app.connection_status.starts_with("Connected") {
        Span::styled("●", Style::default().fg(Color::Green))
    } else if app.connection_status.contains("onnecting") || app.connection_status.contains("econnecting") {
        let pulse = if (app.frame_count / 5) % 2 == 0 { "◌" } else { "●" };
        Span::styled(pulse, Style::default().fg(Color::Yellow))
    } else {
        Span::styled("○", Style::default().fg(Color::Red))
    };

    let sess_dot = if app.session_status.starts_with("Active") {
        Span::styled("●", Style::default().fg(Color::Green))
    } else {
        Span::styled("○", Style::default().fg(Color::DarkGray))
    };

    if width >= 60 {
        let conn_color = if app.connection_status.starts_with("Connected") {
            Color::Green
        } else if app.connection_status.contains("onnecting") || app.connection_status.contains("econnecting") {
            Color::Yellow
        } else if app.connection_status == "Disconnected" {
            Color::Red
        } else {
            Color::DarkGray
        };

        // Calculate remaining space for connection status text
        // Reserve space for " | Sess:● status"
        let sess_suffix_len = 3 + 5 + 1 + app.session_status.len(); // " | Sess:● status"
        let conn_prefix_len = 5 + 1; // "Conn:●"
        let remaining = width.saturating_sub(used_width + conn_prefix_len + sess_suffix_len);

        spans.push(Span::styled("Conn:", Style::default().fg(Color::White)));
        spans.push(conn_dot);

        // Truncate connection status to fit
        let conn_text = if app.connection_status.len() > remaining {
            if remaining > 3 {
                format!("{}...", &app.connection_status[..remaining - 3])
            } else {
                String::new()
            }
        } else {
            app.connection_status.clone()
        };
        spans.push(Span::styled(conn_text, Style::default().fg(conn_color)));

        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled("Sess:", Style::default().fg(Color::White)));
        spans.push(sess_dot);

        let sess_color = match app.session_status.as_str() {
            "Active" => Color::Green,
            _ if app.session_status.starts_with("Active") => Color::Green,
            _ => Color::DarkGray,
        };
        spans.push(Span::styled(&app.session_status, Style::default().fg(sess_color)));
    } else {
        // Narrow: just dots
        spans.push(conn_dot);
        spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
        spans.push(sess_dot);
    }

    let bar = Paragraph::new(Line::from(spans))
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
