use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, DeliveryStatus, MessageDirection};
use crate::transport::TransportKind;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if app.composing {
        draw_with_compose(frame, app, area);
    } else {
        draw_normal(frame, app, area);
    }
}

fn draw_normal(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height;

    // Adapt status bar height to terminal
    let status_height = if height >= 25 { 5 } else { 4 };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(status_height),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, main_chunks[0]);
    draw_messages(frame, app, main_chunks[1]);
    draw_status_bar(frame, app, main_chunks[2]);
    draw_nav(frame, app, main_chunks[3]);
}

fn draw_with_compose(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height;
    let status_height = if height >= 28 { 5 } else { 4 };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(status_height),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, main_chunks[0]);
    draw_messages(frame, app, main_chunks[1]);
    draw_status_bar(frame, app, main_chunks[2]);
    draw_compose(frame, app, main_chunks[3]);
    draw_nav_compose(frame, main_chunks[4]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width;

    let transports = [
        (TransportKind::Rsync, "[1]rsync", "[1]rs", 0),
        (TransportKind::Mcp, "[2]MCP", "[2]MC", 1),
        (TransportKind::Redis, "[3]Redis", "[3]Rd", 2),
    ];

    let mut spans = Vec::new();

    if width >= 60 {
        spans.push(Span::styled(" Transport: ", Style::default().fg(Color::White).bold()));
    } else {
        spans.push(Span::styled(" ", Style::default()));
    }

    for (kind, label_wide, label_narrow, idx) in &transports {
        let is_active = app.active_transport == *kind;
        let is_healthy = app.transport_health[*idx];

        let health_dot = if is_healthy { "* " } else { "x " };
        let health_color = if is_healthy { Color::Green } else { Color::Red };

        let style = if is_active {
            Style::default().fg(Color::Black).bg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let label = if width >= 70 { *label_wide } else { *label_narrow };

        spans.push(Span::styled(health_dot, Style::default().fg(health_color)));
        spans.push(Span::styled(format!("{} ", label), style));
    }

    // Connection status — truncate to fit
    let status_text = &app.connection_status;
    let remaining = width.saturating_sub(spans.iter().map(|s| s.width() as u16).sum::<u16>() + 6);
    if remaining > 5 {
        let truncated = if status_text.len() > remaining as usize {
            format!("{}...", &status_text[..remaining.saturating_sub(3) as usize])
        } else {
            status_text.clone()
        };
        spans.push(Span::styled(
            format!("| {} ", truncated),
            Style::default().fg(match app.connection_status.as_str() {
                "Connected" => Color::Green,
                "Connecting..." | "Launching slave..." => Color::Yellow,
                _ => Color::Red,
            }),
        ));
    }

    let header = Paragraph::new(Line::from(spans))
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Interclaude Bridge ")
                .title_style(Style::default().fg(Color::Cyan).bold()),
        );
    frame.render_widget(header, area);
}

fn draw_messages(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width;

    // On narrow terminals, stack vertically
    let (outbox_area, inbox_area) = if width >= 60 {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        (chunks[0], chunks[1])
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        (chunks[0], chunks[1])
    };

    // Calculate max content preview width
    let msg_inner_width = outbox_area.width.saturating_sub(4) as usize; // borders
    let preview_max = msg_inner_width.saturating_sub(16); // prefix + status + timestamp

    // Outbox (sent)
    let outbox_msgs: Vec<Line> = app
        .messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.direction == MessageDirection::Outbound)
        .map(|(i, m)| {
            let selected = app.selected_message == Some(i);
            let status_style = status_color(&m.status);
            let prefix = if selected { "> " } else { "  " };

            let preview = truncate_str(&m.content_preview, preview_max);

            Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(format!("[{}]", m.status.symbol()), status_style),
                Span::styled(&m.timestamp, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(preview, Style::default().fg(Color::White)),
            ])
        })
        .collect();

    let outbox_content = if outbox_msgs.is_empty() {
        vec![Line::from(Span::styled(
            " Ctrl+N to compose",
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
    frame.render_widget(outbox, outbox_area);

    // Inbox (received)
    let inbox_inner_width = inbox_area.width.saturating_sub(4) as usize;
    let inbox_preview_max = inbox_inner_width.saturating_sub(16);

    let inbox_msgs: Vec<Line> = app
        .messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.direction == MessageDirection::Inbound)
        .map(|(i, m)| {
            let selected = app.selected_message == Some(i);
            let status_style = status_color(&m.status);
            let prefix = if selected { "> " } else { "  " };

            let preview = truncate_str(&m.content_preview, inbox_preview_max);

            Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(format!("[{}]", m.status.symbol()), status_style),
                Span::styled(&m.timestamp, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(preview, Style::default().fg(Color::Green)),
            ])
        })
        .collect();

    let inbox_content = if inbox_msgs.is_empty() {
        vec![Line::from(Span::styled(
            " Waiting for responses...",
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
    frame.render_widget(inbox, inbox_area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width;

    // On narrow terminals, stack vertically
    let (health_area, pipeline_area) = if width >= 70 {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);
        (chunks[0], chunks[1])
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(2)])
            .split(area);
        (chunks[0], chunks[1])
    };

    // Transport health overview + log
    let health_inner = health_area.width.saturating_sub(4) as usize;

    let mut health_lines = if health_inner >= 35 {
        vec![Line::from(vec![
            Span::styled(" rsync:", Style::default().fg(Color::White)),
            transport_status_span(app.transport_health[0]),
            Span::raw(" "),
            Span::styled("MCP:", Style::default().fg(Color::White)),
            transport_status_span(app.transport_health[1]),
            Span::raw(" "),
            Span::styled("Redis:", Style::default().fg(Color::White)),
            transport_status_span(app.transport_health[2]),
        ])]
    } else {
        vec![Line::from(vec![
            transport_status_char(app.transport_health[0], "R"),
            Span::raw(" "),
            transport_status_char(app.transport_health[1], "M"),
            Span::raw(" "),
            transport_status_char(app.transport_health[2], "D"),
        ])]
    };

    health_lines.push(Line::from(vec![
        Span::styled(
            format!(" Active: {}", app.active_transport.label()),
            Style::default().fg(Color::Cyan).bold(),
        ),
    ]));

    // Show latest bridge log entry, truncated
    if let Some(last_log) = app.bridge_log.last() {
        let log_text = truncate_str(last_log, health_inner.saturating_sub(1));
        health_lines.push(Line::from(Span::styled(
            format!(" {}", log_text),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let health = Paragraph::new(health_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Status "),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(health, health_area);

    // Delivery pipeline
    let pipeline_inner = pipeline_area.width.saturating_sub(4) as usize;
    let pipeline_lines = if let Some(idx) = app.selected_message {
        if let Some(msg) = app.messages.get(idx) {
            render_delivery_pipeline(&msg.status, pipeline_inner)
        } else {
            vec![Line::from(Span::styled(
                " No message selected",
                Style::default().fg(Color::DarkGray),
            ))]
        }
    } else {
        vec![Line::from(Span::styled(
            truncate_str(" Select msg (Up/Down) for delivery status", pipeline_inner),
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let pipeline = Paragraph::new(pipeline_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Pipeline "),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(pipeline, pipeline_area);
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

    let max_visible = area.width.saturating_sub(6) as usize; // borders + padding + cursor
    let input = &app.compose_input;
    let visible = if input.len() > max_visible && max_visible > 0 {
        &input[input.len() - max_visible..]
    } else {
        input.as_str()
    };

    let input_text = format!(" {}{}", visible, cursor_char);

    let compose = Paragraph::new(Line::from(vec![
        Span::styled(input_text, Style::default().fg(Color::White)),
    ]))
    .wrap(Wrap { trim: true })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Compose (Enter=send, Esc=cancel) ")
            .title_style(Style::default().fg(Color::Cyan).bold()),
    );
    frame.render_widget(compose, area);
}

fn draw_nav(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width;

    let nav_line = if width >= 70 {
        Line::from(vec![
            Span::styled(" [1/2/3] ", Style::default().fg(Color::Cyan).bold()),
            Span::raw("Transport  "),
            Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Select  "),
            Span::styled(" [Ctrl+N] ", Style::default().fg(Color::Green).bold()),
            Span::raw("New cmd  "),
            Span::styled(" [Ctrl+L] ", Style::default().fg(Color::Magenta).bold()),
            Span::raw("Slave  "),
            Span::styled(" [Ctrl+Q] ", Style::default().fg(Color::Red).bold()),
            Span::raw("Quit"),
        ])
    } else if width >= 45 {
        Line::from(vec![
            Span::styled("[1/2/3]", Style::default().fg(Color::Cyan).bold()),
            Span::raw(" "),
            Span::styled("[C-N]", Style::default().fg(Color::Green).bold()),
            Span::raw(" "),
            Span::styled("[C-L]", Style::default().fg(Color::Magenta).bold()),
            Span::raw(" "),
            Span::styled("[C-Q]", Style::default().fg(Color::Red).bold()),
        ])
    } else {
        Line::from(vec![
            Span::styled("1/2/3 C-N C-L C-Q", Style::default().fg(Color::DarkGray)),
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

fn draw_nav_compose(frame: &mut Frame, area: Rect) {
    let nav = Paragraph::new(Line::from(vec![
        Span::styled(" [Enter] ", Style::default().fg(Color::Green).bold()),
        Span::raw("Send  "),
        Span::styled(" [Esc] ", Style::default().fg(Color::Red).bold()),
        Span::raw("Cancel"),
    ]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(nav, area);
}

// ---- Helpers ----

fn truncate_str(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.len() > max {
        if max > 3 {
            format!("{}...", &s[..max - 3])
        } else {
            s[..max].to_string()
        }
    } else {
        s.to_string()
    }
}

fn transport_status_span(healthy: bool) -> Span<'static> {
    if healthy {
        Span::styled("OK ", Style::default().fg(Color::Green).bold())
    } else {
        Span::styled("-- ", Style::default().fg(Color::Red))
    }
}

fn transport_status_char(healthy: bool, label: &'static str) -> Span<'static> {
    if healthy {
        Span::styled(label, Style::default().fg(Color::Green).bold())
    } else {
        Span::styled(label, Style::default().fg(Color::Red))
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

fn render_delivery_pipeline(current: &DeliveryStatus, max_width: usize) -> Vec<Line<'static>> {
    let stages = [
        (DeliveryStatus::Delivered, ">>", "SENT"),
        (DeliveryStatus::Read, "()", "READ"),
        (DeliveryStatus::Executing, "..", "EXEC"),
        (DeliveryStatus::Executed, "OK", "DONE"),
        (DeliveryStatus::Replying, "<<", "RPLY"),
        (DeliveryStatus::ReceivingReply, "<-", "RECV"),
        (DeliveryStatus::ReceivedReply, "++", "GOT"),
    ];

    // Choose compact or full format based on width
    let use_compact = max_width < 60;

    let mut spans = Vec::new();
    let mut reached_current = false;

    for (i, (stage, sym, short_label)) in stages.iter().enumerate() {
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

        if use_compact {
            spans.push(Span::styled(format!("[{}]", sym), style));
        } else {
            spans.push(Span::styled(format!("[{}]{}", sym, short_label), style));
        }

        if i < stages.len() - 1 {
            spans.push(Span::styled(
                if use_compact { ">" } else { "->" },
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    vec![
        Line::from(Span::styled(
            format!(" {}", current.label()),
            status_color(current).bold(),
        )),
        Line::from(vec![Span::raw(" ")].into_iter().chain(spans).collect::<Vec<_>>()),
    ]
}
