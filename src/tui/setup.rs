use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, SetupField};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let width = area.width;
    let height = area.height;

    // For narrow terminals, stack vertically instead of side-by-side
    if width < 80 {
        draw_narrow(frame, app, area);
    } else {
        draw_wide(frame, app, area);
    }
}

fn draw_wide(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width;

    // Responsive split: give more to form on smaller screens
    let left_pct = if width >= 120 { 55 } else { 60 };
    let right_pct = 100 - left_pct;

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);

    draw_left_panel(frame, app, main_chunks[0]);

    // Right side: tutorial with wrapping
    draw_tutorial(frame, app, main_chunks[1]);
}

fn draw_narrow(frame: &mut Frame, app: &App, area: Rect) {
    // On narrow terminals, put form on top, tutorial below (or hide tutorial)
    let height = area.height;
    let show_tutorial = height >= 30;

    let constraints = if show_tutorial {
        vec![Constraint::Percentage(65), Constraint::Percentage(35)]
    } else {
        vec![Constraint::Percentage(100)]
    };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(constraints)
        .split(area);

    draw_left_panel(frame, app, main_chunks[0]);

    if show_tutorial && main_chunks.len() > 1 {
        draw_tutorial(frame, app, main_chunks[1]);
    }
}

fn draw_left_panel(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height;

    // Adapt log height to available space
    let log_height = if height >= 25 { 8 } else if height >= 18 { 5 } else { 3 };

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(log_height),
            Constraint::Length(3),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(" Setup Connection")
        .style(Style::default().fg(Color::Cyan).bold())
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(title, left_chunks[0]);

    // Form fields
    draw_form(frame, app, left_chunks[1]);

    // Log panel with wrapping
    let log_take = (log_height as usize).saturating_sub(2); // account for borders
    let log_lines: Vec<Line> = app
        .setup_log
        .iter()
        .rev()
        .take(log_take)
        .rev()
        .map(|line| Line::from(Span::styled(line.as_str(), Style::default().fg(Color::DarkGray))))
        .collect();

    let log = Paragraph::new(log_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Log "),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(log, left_chunks[2]);

    // Navigation — adapt to width
    draw_setup_nav(frame, left_chunks[3]);
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let fields = vec![
        (SetupField::RemoteHost, "Remote Host", "e.g., 192.168.1.100"),
        (SetupField::Connection, "Connection", "[Enter] cycle: MOSH/SSH"),
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
        .inner(area);

    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Configuration "),
        area,
    );

    let field_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(form_constraints)
        .split(form_area);

    // Available width for value text (after cursor + label)
    let value_max_width = form_area.width.saturating_sub(20) as usize; // 2 cursor + 14 label + padding

    for (i, (field_type, label, placeholder)) in fields.iter().enumerate() {
        let is_focused = app.setup_field == *field_type;
        let value = app.settings.get_field(field_type);

        // Truncate display value to fit within bounds
        let display_text = if value.is_empty() {
            let ph = if placeholder.len() > value_max_width && value_max_width > 3 {
                format!("{}...", &placeholder[..value_max_width.saturating_sub(3)])
            } else {
                placeholder.to_string()
            };
            Span::styled(ph, Style::default().fg(Color::DarkGray))
        } else {
            let v = if value.len() > value_max_width && value_max_width > 3 {
                format!("{}...", &value[..value_max_width.saturating_sub(3)])
            } else {
                value.clone()
            };
            Span::styled(v, Style::default().fg(Color::White))
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
                display_text,
            ]);
            frame.render_widget(Paragraph::new(line), field_chunks[i]);
        }
    }
}

fn draw_tutorial(frame: &mut Frame, app: &App, area: Rect) {
    let tutorial_lines: Vec<Line> = app
        .tutorial_lines
        .iter()
        .map(|line| Line::from(Span::styled(line.as_str(), Style::default().fg(Color::White))))
        .collect();

    let tutorial = Paragraph::new(tutorial_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Remote Machine Setup Guide ")
                .title_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(tutorial, area);
}

fn draw_setup_nav(frame: &mut Frame, area: Rect) {
    let width = area.width;

    let nav_line = if width >= 70 {
        Line::from(vec![
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
        ])
    } else if width >= 45 {
        Line::from(vec![
            Span::styled("[Tab]", Style::default().fg(Color::Cyan).bold()),
            Span::raw(" "),
            Span::styled("[F2]", Style::default().fg(Color::Yellow).bold()),
            Span::raw(" "),
            Span::styled("[F3]", Style::default().fg(Color::Magenta).bold()),
            Span::raw(" "),
            Span::styled("[C-S]", Style::default().fg(Color::Blue).bold()),
            Span::raw(" "),
            Span::styled("[C-Enter]", Style::default().fg(Color::Green).bold()),
            Span::raw(" "),
            Span::styled("[Esc]", Style::default().fg(Color::Red).bold()),
        ])
    } else {
        Line::from(vec![
            Span::styled("Tab F2 F3 C-S C-Ent Esc", Style::default().fg(Color::DarkGray)),
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
