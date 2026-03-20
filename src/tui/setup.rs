use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, SetupField};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    // Left side: form + log
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(15),   // Form (10 fields + margins)
            Constraint::Length(8), // Log
            Constraint::Length(3), // Nav
        ])
        .split(main_chunks[0]);

    // Title
    let title = Paragraph::new(" Setup Connection")
        .style(Style::default().fg(Color::Cyan).bold())
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(title, left_chunks[0]);

    // Form fields
    let fields = vec![
        (SetupField::RemoteHost, "Remote Host", "e.g., 192.168.1.100"),
        (SetupField::Connection, "Connection", "[Enter] to cycle: MOSH/SSH"),
        (SetupField::SshUser, "User", "e.g., root"),
        (SetupField::SshPort, "Port", "default: 22"),
        (SetupField::KeyPath, "Key Path", "~/.ssh/id_ed25519"),
        (SetupField::RemoteDir, "Remote Dir", "~/Interclaude"),
        (SetupField::Transport, "Transport", "[Enter] to cycle"),
        (SetupField::RedisHost, "Redis Host", "127.0.0.1"),
        (SetupField::RedisPort, "Redis Port", "6379"),
        (SetupField::RedisPassword, "Redis Pass", "(optional)"),
    ];

    let form_constraints: Vec<Constraint> = fields.iter().map(|_| Constraint::Length(1)).collect();

    let form_area = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Configuration ")
        .inner(left_chunks[1]);

    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Configuration "),
        left_chunks[1],
    );

    let field_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(form_constraints)
        .split(form_area);

    for (i, (field_type, label, placeholder)) in fields.iter().enumerate() {
        let is_focused = app.setup_field == *field_type;
        let value = app.settings.get_field(field_type);

        let display = if value.is_empty() {
            Span::styled(*placeholder, Style::default().fg(Color::DarkGray))
        } else {
            Span::styled(&value, Style::default().fg(Color::White))
        };

        let label_style = if is_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::Gray)
        };

        let cursor = if is_focused { "> " } else { "  " };

        if i < field_chunks.len() {
            let line = Line::from(vec![
                Span::styled(cursor, Style::default().fg(Color::Cyan)),
                Span::styled(format!("{:<14}", label), label_style),
                display,
            ]);
            frame.render_widget(Paragraph::new(line), field_chunks[i]);
        }
    }

    // Log panel
    let log_lines: Vec<Line> = app
        .setup_log
        .iter()
        .rev()
        .take(5)
        .rev()
        .map(|line| Line::from(Span::styled(line.as_str(), Style::default().fg(Color::DarkGray))))
        .collect();

    let log = Paragraph::new(log_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Log "),
    );
    frame.render_widget(log, left_chunks[2]);

    // Navigation
    let nav = Paragraph::new(Line::from(vec![
        Span::styled(" [Tab] ", Style::default().fg(Color::Cyan).bold()),
        Span::raw("Next  "),
        Span::styled(" [F2] ", Style::default().fg(Color::Yellow).bold()),
        Span::raw("Test  "),
        Span::styled(" [F3] ", Style::default().fg(Color::Magenta).bold()),
        Span::raw("Gen Key  "),
        Span::styled(" [Ctrl+S] ", Style::default().fg(Color::Blue).bold()),
        Span::raw("Save  "),
        Span::styled(" [Ctrl+Enter] ", Style::default().fg(Color::Green).bold()),
        Span::raw("Connect  "),
        Span::styled(" [Esc] ", Style::default().fg(Color::Red).bold()),
        Span::raw("Back"),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(nav, left_chunks[3]);

    // Right side: tutorial
    let tutorial_lines: Vec<Line> = app
        .tutorial_lines
        .iter()
        .map(|line| Line::from(Span::styled(line.as_str(), Style::default().fg(Color::White))))
        .collect();

    let tutorial = Paragraph::new(tutorial_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Remote Machine Setup Guide ")
            .title_style(Style::default().fg(Color::Yellow)),
    );
    frame.render_widget(tutorial, main_chunks[1]);
}
