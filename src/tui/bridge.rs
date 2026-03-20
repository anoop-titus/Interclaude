use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, DeliveryStatus, MessageDirection};
use crate::transport::TransportKind;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // If composing, show compose overlay
    if app.composing {
        draw_with_compose(frame, app, area);
    } else {
        draw_normal(frame, app, area);
    }
}

fn draw_normal(frame: &mut Frame, app: &App, area: Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Header with transport selector
            Constraint::Min(10),   // Message panels
            Constraint::Length(5), // Status bar
            Constraint::Length(3), // Navigation
        ])
        .split(area);

    draw_header(frame, app, main_chunks[0]);
    draw_messages(frame, app, main_chunks[1]);
    draw_status_bar(frame, app, main_chunks[2]);
    draw_nav(frame, main_chunks[3]);
}

fn draw_with_compose(frame: &mut Frame, app: &App, area: Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(8),    // Message panels (smaller)
            Constraint::Length(5), // Status bar
            Constraint::Length(3), // Compose input
            Constraint::Length(3), // Navigation
        ])
        .split(area);

    draw_header(frame, app, main_chunks[0]);
    draw_messages(frame, app, main_chunks[1]);
    draw_status_bar(frame, app, main_chunks[2]);
    draw_compose(frame, app, main_chunks[3]);
    draw_nav_compose(frame, main_chunks[4]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let transports = [
        (TransportKind::Rsync, "[1] rsync", 0),
        (TransportKind::Mcp, "[2] MCP", 1),
        (TransportKind::Redis, "[3] Redis", 2),
    ];

    let mut spans = vec![
        Span::styled(" Transport: ", Style::default().fg(Color::White).bold()),
    ];

    for (kind, label, idx) in &transports {
        let is_active = app.active_transport == *kind;
        let is_healthy = app.transport_health[*idx];

        let health_dot = if is_healthy { "* " } else { "x " };
        let health_color = if is_healthy { Color::Green } else { Color::Red };

        let style = if is_active {
            Style::default().fg(Color::Black).bg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };

        spans.push(Span::styled(health_dot, Style::default().fg(health_color)));
        spans.push(Span::styled(format!("{} ", label), style));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(
        format!("   | {} ", app.connection_status),
        Style::default().fg(match app.connection_status.as_str() {
            "Connected" => Color::Green,
            "Connecting..." | "Launching slave..." => Color::Yellow,
            _ => Color::Red,
        }),
    ));

    let header = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Interclaude Bridge ")
            .title_style(Style::default().fg(Color::Cyan).bold()),
    );
    frame.render_widget(header, area);
}

fn draw_messages(frame: &mut Frame, app: &App, area: Rect) {
    let msg_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Outbox (sent)
    let outbox_msgs: Vec<Line> = app
        .messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.direction == MessageDirection::Outbound)
        .map(|(i, m)| {
            let selected = app.selected_message == Some(i);
            let status_style = status_color(&m.status);
            let prefix = if selected { ">> " } else { "   " };

            Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("[{}] ", m.status.symbol()),
                    status_style,
                ),
                Span::styled(&m.timestamp, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(&m.content_preview, Style::default().fg(Color::White)),
            ])
        })
        .collect();

    let outbox_content = if outbox_msgs.is_empty() {
        vec![Line::from(Span::styled(
            "   No messages sent yet. Press Ctrl+N to compose.",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        outbox_msgs
    };

    let outbox = Paragraph::new(outbox_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Outbox (Sent) ")
                .title_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(outbox, msg_chunks[0]);

    // Inbox (received)
    let inbox_msgs: Vec<Line> = app
        .messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.direction == MessageDirection::Inbound)
        .map(|(i, m)| {
            let selected = app.selected_message == Some(i);
            let status_style = status_color(&m.status);
            let prefix = if selected { ">> " } else { "   " };

            Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("[{}] ", m.status.symbol()),
                    status_style,
                ),
                Span::styled(&m.timestamp, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(&m.content_preview, Style::default().fg(Color::Green)),
            ])
        })
        .collect();

    let inbox_content = if inbox_msgs.is_empty() {
        vec![Line::from(Span::styled(
            "   Waiting for responses...",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        inbox_msgs
    };

    let inbox = Paragraph::new(inbox_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Inbox (Received) ")
                .title_style(Style::default().fg(Color::Green)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(inbox, msg_chunks[1]);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Transport health overview + log
    let mut health_lines = vec![
        Line::from(vec![
            Span::styled(" rsync: ", Style::default().fg(Color::White)),
            transport_status_span(app.transport_health[0]),
            Span::raw("  "),
            Span::styled("MCP: ", Style::default().fg(Color::White)),
            transport_status_span(app.transport_health[1]),
            Span::raw("  "),
            Span::styled("Redis: ", Style::default().fg(Color::White)),
            transport_status_span(app.transport_health[2]),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" Active: {}", app.active_transport.label()),
                Style::default().fg(Color::Cyan).bold(),
            ),
        ]),
    ];

    // Show latest bridge log entry
    if let Some(last_log) = app.bridge_log.last() {
        health_lines.push(Line::from(Span::styled(
            format!(" {}", last_log),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let health = Paragraph::new(health_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Transport Status "),
    );
    frame.render_widget(health, status_chunks[0]);

    // Selected message delivery pipeline
    let pipeline_lines = if let Some(idx) = app.selected_message {
        if let Some(msg) = app.messages.get(idx) {
            render_delivery_pipeline(&msg.status)
        } else {
            vec![Line::from(Span::styled(
                " No message selected",
                Style::default().fg(Color::DarkGray),
            ))]
        }
    } else {
        vec![Line::from(Span::styled(
            " Select a message (Up/Down) to see delivery status",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let pipeline = Paragraph::new(pipeline_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Delivery Pipeline "),
    );
    frame.render_widget(pipeline, status_chunks[1]);
}

fn draw_compose(frame: &mut Frame, app: &App, area: Rect) {
    let cursor_char = if (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        / 500)
        % 2
        == 0
    {
        "_"
    } else {
        " "
    };

    let input_text = format!(" {}{}", app.compose_input, cursor_char);

    let compose = Paragraph::new(Line::from(vec![
        Span::styled(&input_text, Style::default().fg(Color::White)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Compose Command (Enter to send, Esc to cancel) ")
            .title_style(Style::default().fg(Color::Cyan).bold()),
    );
    frame.render_widget(compose, area);
}

fn draw_nav(frame: &mut Frame, area: Rect) {
    let nav = Paragraph::new(Line::from(vec![
        Span::styled(" [1/2/3] ", Style::default().fg(Color::Cyan).bold()),
        Span::raw("Transport  "),
        Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).bold()),
        Span::raw("Select  "),
        Span::styled(" [Ctrl+N] ", Style::default().fg(Color::Green).bold()),
        Span::raw("New cmd  "),
        Span::styled(" [Ctrl+L] ", Style::default().fg(Color::Magenta).bold()),
        Span::raw("Launch slave  "),
        Span::styled(" [Ctrl+Q] ", Style::default().fg(Color::Red).bold()),
        Span::raw("Quit"),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(nav, area);
}

fn draw_nav_compose(frame: &mut Frame, area: Rect) {
    let nav = Paragraph::new(Line::from(vec![
        Span::styled(" [Enter] ", Style::default().fg(Color::Green).bold()),
        Span::raw("Send  "),
        Span::styled(" [Esc] ", Style::default().fg(Color::Red).bold()),
        Span::raw("Cancel  "),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(nav, area);
}

fn transport_status_span(healthy: bool) -> Span<'static> {
    if healthy {
        Span::styled("READY", Style::default().fg(Color::Green).bold())
    } else {
        Span::styled("DOWN", Style::default().fg(Color::Red))
    }
}

fn status_color(status: &DeliveryStatus) -> Style {
    match status {
        DeliveryStatus::Delivered => Style::default().fg(Color::Yellow),
        DeliveryStatus::Read => Style::default().fg(Color::Blue),
        DeliveryStatus::Executing => Style::default().fg(Color::Magenta),
        DeliveryStatus::Executed => Style::default().fg(Color::Cyan),
        DeliveryStatus::Replying => Style::default().fg(Color::Blue),
        DeliveryStatus::ReceivingReply => Style::default().fg(Color::Yellow),
        DeliveryStatus::ReceivedReply => Style::default().fg(Color::Green),
        DeliveryStatus::DeliveryFailed => Style::default().fg(Color::Red),
        DeliveryStatus::ExecutionError => Style::default().fg(Color::Red),
        DeliveryStatus::Timeout => Style::default().fg(Color::Red),
    }
}

fn render_delivery_pipeline(current: &DeliveryStatus) -> Vec<Line<'static>> {
    let stages = [
        (DeliveryStatus::Delivered, ">>"),
        (DeliveryStatus::Read, "()"),
        (DeliveryStatus::Executing, ".."),
        (DeliveryStatus::Executed, "OK"),
        (DeliveryStatus::Replying, "<<"),
        (DeliveryStatus::ReceivingReply, "<-"),
        (DeliveryStatus::ReceivedReply, "++"),
    ];

    let mut spans = Vec::new();
    let mut reached_current = false;

    for (i, (stage, sym)) in stages.iter().enumerate() {
        let is_current = stage == current;
        if is_current {
            reached_current = true;
        }

        let style = if is_current {
            Style::default().fg(Color::Cyan).bold()
        } else if !reached_current {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        spans.push(Span::styled(format!("[{}]{}", sym, stage.label()), style));
        if i < stages.len() - 1 {
            spans.push(Span::styled("->", Style::default().fg(Color::DarkGray)));
        }
    }

    vec![
        Line::from(Span::styled(
            format!(" Status: {}", current.label()),
            status_color(current).bold(),
        )),
        Line::from(spans),
    ]
}
