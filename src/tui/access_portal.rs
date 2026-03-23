use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{AccessPortalField, App};
use super::status_bar;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let margin = if area.width >= 80 { 2 } else { 1 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(margin)
        .constraints([
            Constraint::Length(2),  // Global status bar
            Constraint::Length(3),  // Title/description
            Constraint::Min(8),    // Form
            Constraint::Length(3), // Validation status
            Constraint::Length(3),  // Navigation
        ])
        .split(area);

    status_bar::draw_global_status(frame, app, chunks[0]);
    draw_title(frame, chunks[1]);
    draw_form(frame, app, chunks[2]);
    draw_validation_status(frame, app, chunks[3]);
    draw_nav(frame, app, chunks[4]);
}

fn draw_title(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled(
            "Configure API access for Error Resolution Engine",
            Style::default().fg(Color::White).bold(),
        )),
        Line::from(Span::styled(
            "Credentials are encrypted and stored locally",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let title = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(title, area);
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let show_api_key = app.show_api_key_field();

    let mut lines: Vec<Line> = Vec::new();

    // ── Authentication ── header
    let auth_header = centered_section_header("Authentication", area.width.saturating_sub(4) as usize);
    lines.push(Line::from(Span::styled(auth_header, Style::default().fg(Color::DarkGray))));

    // Access Mode field
    let mode_focused = app.access_portal_field == AccessPortalField::AccessMode;
    let mode_indicator = if mode_focused { "▸ " } else { "  " };
    let mode_style = if mode_focused {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::White)
    };
    lines.push(Line::from(vec![
        Span::styled(mode_indicator, Style::default().fg(Color::Cyan)),
        Span::styled("Access Mode: ", Style::default().fg(Color::White)),
        Span::styled(format!("< {} >", app.access_mode.label()), mode_style),
    ]));

    // API Key field (only when ApiKey mode)
    if show_api_key {
        let key_focused = app.access_portal_field == AccessPortalField::ApiKey;
        let key_indicator = if key_focused { "▸ " } else { "  " };
        let key_style = if key_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::White)
        };

        // Mask the API key (show first 7 chars "sk-ant-" + bullets)
        let masked = if app.api_key_input.is_empty() {
            "(enter API key)".to_string()
        } else if app.api_key_input.len() <= 7 {
            app.api_key_input.clone()
        } else {
            let visible = &app.api_key_input[..7];
            let bullets = "•".repeat(app.api_key_input.len() - 7);
            format!("{}{}", visible, bullets)
        };

        lines.push(Line::from(vec![
            Span::styled(key_indicator, Style::default().fg(Color::Cyan)),
            Span::styled("API Key:     ", Style::default().fg(Color::White)),
            Span::styled(masked, key_style),
        ]));
    }

    lines.push(Line::from("")); // spacer

    // ── Model ── header
    let model_header = centered_section_header("Model", area.width.saturating_sub(4) as usize);
    lines.push(Line::from(Span::styled(model_header, Style::default().fg(Color::DarkGray))));

    // Model selector
    let model_focused = app.access_portal_field == AccessPortalField::Model;
    let model_indicator = if model_focused { "▸ " } else { "  " };
    let model_style = if model_focused {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::White)
    };
    lines.push(Line::from(vec![
        Span::styled(model_indicator, Style::default().fg(Color::Cyan)),
        Span::styled("Model:       ", Style::default().fg(Color::White)),
        Span::styled(format!("< {} >", app.model_selection.label()), model_style),
    ]));

    // Model ID hint
    lines.push(Line::from(vec![
        Span::raw("               "),
        Span::styled(
            format!("ID: {}", app.model_selection.model_id()),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    let form = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Access Portal ")
                .title_style(Style::default().fg(Color::Magenta).bold()),
        );
    frame.render_widget(form, area);
}

fn draw_validation_status(frame: &mut Frame, app: &App, area: Rect) {
    let (text, color) = match &app.api_validation_status {
        None => {
            if app.credentials_saved {
                ("  ✓ Credentials saved (press Ctrl+V to validate)".to_string(), Color::Green)
            } else if !app.api_key_input.is_empty() {
                ("  Press Ctrl+V to validate API key".to_string(), Color::Yellow)
            } else {
                ("  Enter credentials to continue".to_string(), Color::DarkGray)
            }
        }
        Some(Ok(msg)) => (format!("  ✓ {}", msg), Color::Green),
        Some(Err(msg)) => (format!("  ✗ {}", msg), Color::Red),
    };

    let status = Paragraph::new(Line::from(Span::styled(text, Style::default().fg(color).bold())))
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(status, area);
}

fn draw_nav(frame: &mut Frame, _app: &App, area: Rect) {
    let width = area.width;

    let nav_line = if width >= 60 {
        Line::from(vec![
            Span::styled(" [Tab] ", Style::default().fg(Color::Cyan).bold()),
            Span::raw("Next field  "),
            Span::styled(" [Enter] ", Style::default().fg(Color::Green).bold()),
            Span::raw("Cycle option  "),
            Span::styled(" [C-V] ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Validate  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Red).bold()),
            Span::raw("Back"),
        ])
    } else {
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(Color::Cyan).bold()),
            Span::styled(":Next ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Green).bold()),
            Span::styled(":Cycle ", Style::default().fg(Color::DarkGray)),
            Span::styled("C-V", Style::default().fg(Color::Yellow).bold()),
            Span::styled(":Check ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Red).bold()),
            Span::styled(":Back", Style::default().fg(Color::DarkGray)),
        ])
    };

    let nav = Paragraph::new(nav_line)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(nav, area);
}

fn centered_section_header(title: &str, width: usize) -> String {
    let dashes = width.saturating_sub(title.len() + 4) / 2;
    let left = "─".repeat(dashes);
    let right = "─".repeat(width.saturating_sub(dashes + title.len() + 4));
    format!("{} {} {}", left, title, right)
}
