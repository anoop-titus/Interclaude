use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, BridgeFocus, DeliveryStatus, MessageDirection};
use crate::transport::TransportKind;
use super::status_bar;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Help overlay takes priority
    if app.show_help_overlay {
        draw_layout(frame, app, area);
        draw_help_overlay(frame, area);
        return;
    }

    draw_layout(frame, app, area);
}

fn draw_layout(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height;
    let small_terminal = height < 20;

    // Auto-collapse panels on small terminals
    let show_status = if small_terminal { false } else { app.show_status_panel };
    let _show_pipeline = if small_terminal { false } else { app.show_pipeline_panel };

    let status_height = if show_status { 5 } else { 0 };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2),  // Global status bar
            Constraint::Length(3),  // Transport header
            Constraint::Min(6),    // Messages (outbox + inbox)
            Constraint::Length(status_height), // Status + pipeline (collapsible)
            Constraint::Length(3),  // Input bar
            Constraint::Length(3),  // Nav legend
        ])
        .split(area);

    status_bar::draw_global_status(frame, app, main_chunks[0]);
    draw_header(frame, app, main_chunks[1]);
    draw_messages(frame, app, main_chunks[2]);

    if show_status {
        draw_status_bar(frame, app, main_chunks[3]);
    }

    draw_input_bar(frame, app, main_chunks[4]);
    draw_nav(frame, app, main_chunks[5]);
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

        let health_dot = if is_healthy { "● " } else { "○ " };
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

    // Transport recommendation hint
    if let Some((rec_kind, ref reason)) = app.transport_recommendation {
        if rec_kind != app.active_transport {
            let hint = format!(" Suggested: {} ({})", rec_kind.label(), reason);
            let hint = if hint.len() > (width as usize).saturating_sub(50) {
                format!(" Try: {}", rec_kind.label())
            } else {
                hint
            };
            spans.push(Span::styled(hint, Style::default().fg(Color::Yellow)));
        }
    }

    // Connection status
    let status_text = &app.connection_status;
    let remaining = width.saturating_sub(spans.iter().map(|s| s.width() as u16).sum::<u16>() + 6);
    if remaining > 5 {
        let truncated = if status_text.len() > remaining as usize {
            format!("{}...", &status_text[..remaining.saturating_sub(3) as usize])
        } else {
            status_text.clone()
        };
        let conn_color = if app.connection_status.starts_with("Connected") {
            Color::Green
        } else if app.connection_status.contains("onnecting") || app.connection_status.contains("econnecting") {
            Color::Yellow
        } else {
            Color::Red
        };
        spans.push(Span::styled(
            format!("| {} ", truncated),
            Style::default().fg(conn_color),
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

    let outbox_focused = app.bridge_focus == BridgeFocus::Outbox;
    let inbox_focused = app.bridge_focus == BridgeFocus::Inbox;

    // Available content width (subtract borders + padding)
    let outbox_content_width = outbox_area.width.saturating_sub(2) as usize;
    let inbox_content_width = inbox_area.width.saturating_sub(2) as usize;

    // Outbox (sent)
    let outbox_msgs: Vec<&crate::app::MessageEntry> = app.messages.iter()
        .filter(|m| m.direction == MessageDirection::Outbound)
        .collect();

    let outbox_items: Vec<ListItem> = if outbox_msgs.is_empty() {
        vec![ListItem::new(
            wrap_message_text(
                " No tasks sent yet. Type below to send your first task.",
                outbox_content_width,
                Style::default().fg(Color::DarkGray),
            )
        )]
    } else {
        outbox_msgs.iter().map(|m| {
            let prefix = format!("You → [{}] {} ", m.status.symbol(), m.timestamp);
            let prefix_len = prefix.len();
            let content = &m.content_preview;
            let full_text = format!("{}{}", prefix, content);

            let lines = wrap_to_lines(&full_text, outbox_content_width);
            let mut styled_lines: Vec<Line> = Vec::new();

            for (i, line_text) in lines.iter().enumerate() {
                if i == 0 {
                    // First line: styled prefix + content start
                    let mut spans = vec![
                        Span::styled("You → ", Style::default().fg(Color::Cyan).bold()),
                        Span::styled(format!("[{}] ", m.status.symbol()), status_color(&m.status)),
                        Span::styled(&m.timestamp, Style::default().fg(Color::DarkGray)),
                        Span::raw(" "),
                    ];
                    // Content portion of first line
                    let content_start = if line_text.len() > prefix_len {
                        &line_text[prefix_len..]
                    } else {
                        ""
                    };
                    if !content_start.is_empty() {
                        spans.push(Span::styled(content_start.to_string(), Style::default().fg(Color::White)));
                    }
                    styled_lines.push(Line::from(spans));
                } else {
                    // Continuation lines: indented content
                    styled_lines.push(Line::from(Span::styled(
                        format!("       {}", line_text),
                        Style::default().fg(Color::White),
                    )));
                }
            }

            ListItem::new(Text::from(styled_lines))
        }).collect()
    };

    let outbox_title = format!(" Outbox ({}) ", outbox_msgs.len());
    let outbox_border = if outbox_focused { Color::Cyan } else { Color::DarkGray };
    let outbox_list = List::new(outbox_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(outbox_border))
                .title(outbox_title)
                .title_style(Style::default().fg(Color::Yellow)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    let mut outbox_state = ListState::default();
    if outbox_focused && !outbox_msgs.is_empty() {
        outbox_state.select(Some(app.outbox_scroll.min(outbox_msgs.len().saturating_sub(1))));
    }
    frame.render_stateful_widget(outbox_list, outbox_area, &mut outbox_state);

    // Inbox (received)
    let inbox_msgs: Vec<&crate::app::MessageEntry> = app.messages.iter()
        .filter(|m| m.direction == MessageDirection::Inbound)
        .collect();

    let inbox_items: Vec<ListItem> = if inbox_msgs.is_empty() {
        vec![ListItem::new(
            wrap_message_text(
                " Waiting for remote Claude to respond...",
                inbox_content_width,
                Style::default().fg(Color::DarkGray),
            )
        )]
    } else {
        inbox_msgs.iter().map(|m| {
            let prefix = format!("Remote → [{}] {} ", m.status.symbol(), m.timestamp);
            let prefix_len = prefix.len();
            let content = &m.content_preview;
            let full_text = format!("{}{}", prefix, content);

            let lines = wrap_to_lines(&full_text, inbox_content_width);
            let mut styled_lines: Vec<Line> = Vec::new();

            for (i, line_text) in lines.iter().enumerate() {
                if i == 0 {
                    let mut spans = vec![
                        Span::styled("Remote → ", Style::default().fg(Color::Green).bold()),
                        Span::styled(format!("[{}] ", m.status.symbol()), status_color(&m.status)),
                        Span::styled(&m.timestamp, Style::default().fg(Color::DarkGray)),
                        Span::raw(" "),
                    ];
                    let content_start = if line_text.len() > prefix_len {
                        &line_text[prefix_len..]
                    } else {
                        ""
                    };
                    if !content_start.is_empty() {
                        spans.push(Span::styled(content_start.to_string(), Style::default().fg(Color::Green)));
                    }
                    styled_lines.push(Line::from(spans));
                } else {
                    styled_lines.push(Line::from(Span::styled(
                        format!("          {}", line_text),
                        Style::default().fg(Color::Green),
                    )));
                }
            }

            ListItem::new(Text::from(styled_lines))
        }).collect()
    };

    let inbox_title = format!(" Inbox ({}) ", inbox_msgs.len());
    let inbox_border = if inbox_focused { Color::Cyan } else { Color::DarkGray };
    let inbox_list = List::new(inbox_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(inbox_border))
                .title(inbox_title)
                .title_style(Style::default().fg(Color::Green)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    let mut inbox_state = ListState::default();
    if inbox_focused && !inbox_msgs.is_empty() {
        inbox_state.select(Some(app.inbox_scroll.min(inbox_msgs.len().saturating_sub(1))));
    }
    frame.render_stateful_widget(inbox_list, inbox_area, &mut inbox_state);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width;

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
    let mut health_lines = Vec::new();

    health_lines.push(Line::from(vec![
        Span::styled(" rsync:", Style::default().fg(Color::White)),
        transport_status_span(app.transport_health[0]),
        Span::raw(" "),
        Span::styled("MCP:", Style::default().fg(Color::White)),
        transport_status_span(app.transport_health[1]),
        Span::raw(" "),
        Span::styled("Redis:", Style::default().fg(Color::White)),
        transport_status_span(app.transport_health[2]),
    ]));

    let active_text = truncate_str(&format!(" Active: {}", app.active_transport.label()), health_inner);
    health_lines.push(Line::from(Span::styled(
        active_text,
        Style::default().fg(Color::Cyan).bold(),
    )));

    if let Some(last_log) = app.bridge_log.last() {
        let log_text = truncate_str(last_log, health_inner.saturating_sub(1));
        health_lines.push(Line::from(Span::styled(
            format!(" {}", log_text),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let health = Paragraph::new(health_lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Status "),
        );
    frame.render_widget(health, health_area);

    // Delivery pipeline
    let pipeline_lines = if let Some(idx) = app.selected_message {
        if let Some(msg) = app.messages.get(idx) {
            render_delivery_pipeline(&msg.status, app.frame_count)
        } else {
            vec![Line::from(Span::styled(
                " No message selected",
                Style::default().fg(Color::DarkGray),
            ))]
        }
    } else {
        vec![Line::from(Span::styled(
            " Select msg (Up/Down) for delivery status",
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

fn draw_input_bar(frame: &mut Frame, app: &App, area: Rect) {
    let input_focused = app.bridge_focus == BridgeFocus::Input;
    let border_color = if input_focused { Color::Cyan } else { Color::DarkGray };

    let max_visible = area.width.saturating_sub(4) as usize;
    let input = &app.compose_input;

    let display_text = if input.is_empty() {
        Line::from(Span::styled(
            " Type a task for the remote Claude session...",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        let visible = if input.len() > max_visible && max_visible > 0 {
            &input[input.len() - max_visible..]
        } else {
            input.as_str()
        };

        let cursor_char = if (app.frame_count / 5) % 2 == 0 { "▌" } else { " " };
        Line::from(vec![
            Span::styled(format!(" {}", visible), Style::default().fg(Color::White)),
            Span::styled(cursor_char, Style::default().fg(Color::Cyan)),
        ])
    };

    let input_widget = Paragraph::new(display_text)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(" Send task to remote Claude ")
                .title_style(Style::default().fg(Color::Cyan).bold()),
        );
    frame.render_widget(input_widget, area);
}

fn draw_nav(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width;

    let status_label = if app.show_status_panel { "ON" } else { "OFF" };

    let nav_line = if width >= 75 {
        Line::from(vec![
            Span::styled(" [Enter] ", Style::default().fg(Color::Green).bold()),
            Span::raw("Send  "),
            Span::styled(" [Tab] ", Style::default().fg(Color::Cyan).bold()),
            Span::raw("Focus  "),
            Span::styled(" [1/2/3] ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Transport  "),
            Span::styled(" [F5] ", Style::default().fg(Color::Magenta).bold()),
            Span::raw(format!("Status[{}]  ", status_label)),
            Span::styled(" [C-H] ", Style::default().fg(Color::Blue).bold()),
            Span::raw("Help  "),
            Span::styled(" [C-Q] ", Style::default().fg(Color::Red).bold()),
            Span::raw("Quit"),
        ])
    } else if width >= 50 {
        Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(Color::Green).bold()),
            Span::styled(" Send ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Tab]", Style::default().fg(Color::Cyan).bold()),
            Span::styled(" Focus ", Style::default().fg(Color::DarkGray)),
            Span::styled("[F5]", Style::default().fg(Color::Magenta).bold()),
            Span::styled(format!(" Stat[{}] ", status_label), Style::default().fg(Color::DarkGray)),
            Span::styled("[C-H]", Style::default().fg(Color::Blue).bold()),
            Span::styled(" Help ", Style::default().fg(Color::DarkGray)),
            Span::styled("[C-Q]", Style::default().fg(Color::Red).bold()),
            Span::styled(" Quit", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Green).bold()),
            Span::styled(":Send ", Style::default().fg(Color::DarkGray)),
            Span::styled("Tab", Style::default().fg(Color::Cyan).bold()),
            Span::styled(":Focus ", Style::default().fg(Color::DarkGray)),
            Span::styled("C-H", Style::default().fg(Color::Blue).bold()),
            Span::styled(":Help ", Style::default().fg(Color::DarkGray)),
            Span::styled("C-Q", Style::default().fg(Color::Red).bold()),
            Span::styled(":Quit", Style::default().fg(Color::DarkGray)),
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

fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    let help_lines = vec![
        Line::from(Span::styled("  Keybindings  ", Style::default().fg(Color::Cyan).bold())),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Enter    ", Style::default().fg(Color::Green).bold()),
            Span::raw("Send task to remote Claude"),
        ]),
        Line::from(vec![
            Span::styled("  Tab      ", Style::default().fg(Color::Cyan).bold()),
            Span::raw("Switch focus: Outbox → Inbox → Input"),
        ]),
        Line::from(vec![
            Span::styled("  Up/Down  ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Scroll focused message list"),
        ]),
        Line::from(vec![
            Span::styled("  1/2/3    ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Switch transport: rsync/MCP/Redis"),
        ]),
        Line::from(vec![
            Span::styled("  F5       ", Style::default().fg(Color::Magenta).bold()),
            Span::raw("Toggle status panel"),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+L   ", Style::default().fg(Color::Magenta).bold()),
            Span::raw("Launch slave on remote"),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+H   ", Style::default().fg(Color::Blue).bold()),
            Span::raw("Toggle this help overlay"),
        ]),
        Line::from(vec![
            Span::styled("  Esc      ", Style::default().fg(Color::Red).bold()),
            Span::raw("Back to Setup / dismiss help"),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Q   ", Style::default().fg(Color::Red).bold()),
            Span::raw("Quit application"),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Press Ctrl+H or Esc to close", Style::default().fg(Color::DarkGray))),
    ];

    let overlay_height = help_lines.len() as u16 + 2; // +2 for borders
    let overlay_width = 50.min(area.width.saturating_sub(4));
    let x = (area.width.saturating_sub(overlay_width)) / 2;
    let y = (area.height.saturating_sub(overlay_height)) / 2;

    let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

    // Clear background
    frame.render_widget(Clear, overlay_area);

    let overlay = Paragraph::new(help_lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Help ")
                .title_style(Style::default().fg(Color::Cyan).bold()),
        );
    frame.render_widget(overlay, overlay_area);
}

// ---- Helpers ----

/// Wrap a plain text string into lines that fit within `max_width` characters.
/// Breaks on word boundaries when possible, hard-breaks long words.
fn wrap_to_lines(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    if text.len() <= max_width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_width {
            lines.push(remaining.to_string());
            break;
        }

        // Find last space within max_width for word-boundary break
        let break_at = remaining[..max_width]
            .rfind(' ')
            .unwrap_or(max_width); // hard-break if no space found

        let break_at = if break_at == 0 { max_width } else { break_at };

        lines.push(remaining[..break_at].to_string());
        remaining = remaining[break_at..].trim_start();
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Wrap a single message string into a multi-line Text with consistent style
fn wrap_message_text(text: &str, max_width: usize, style: Style) -> Text<'static> {
    let lines = wrap_to_lines(text, max_width);
    Text::from(
        lines.into_iter()
            .map(|l| Line::from(Span::styled(l, style)))
            .collect::<Vec<_>>()
    )
}

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
        Span::styled("● ", Style::default().fg(Color::Green).bold())
    } else {
        Span::styled("○ ", Style::default().fg(Color::Red))
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

fn render_delivery_pipeline(current: &DeliveryStatus, frame_count: u64) -> Vec<Line<'static>> {
    let stages: &[(DeliveryStatus, &str)] = &[
        (DeliveryStatus::Delivered, "SENT"),
        (DeliveryStatus::Read, "READ"),
        (DeliveryStatus::Executing, "RUNNING"),
        (DeliveryStatus::Executed, "DONE"),
        (DeliveryStatus::Replying, "REPLYING"),
        (DeliveryStatus::ReceivingReply, "RECEIVING"),
        (DeliveryStatus::ReceivedReply, "COMPLETE"),
    ];

    let current_ord = current.ordinal();
    let mut spans = Vec::new();
    spans.push(Span::raw(" "));

    for (i, (stage, label)) in stages.iter().enumerate() {
        let stage_ord = stage.ordinal();

        let style = if stage_ord == current_ord {
            // Current stage: cyan + pulsing indicator
            let pulse = if (frame_count / 5) % 2 == 0 { "● " } else { "○ " };
            spans.push(Span::styled(pulse, Style::default().fg(Color::Cyan).bold()));
            Style::default().fg(Color::Cyan).bold()
        } else if stage_ord < current_ord {
            // Completed: green
            spans.push(Span::styled("✓ ", Style::default().fg(Color::Green)));
            Style::default().fg(Color::Green)
        } else {
            // Future: gray
            spans.push(Span::styled("  ", Style::default()));
            Style::default().fg(Color::DarkGray)
        };

        spans.push(Span::styled(*label, style));

        if i < stages.len() - 1 {
            spans.push(Span::styled(" → ", Style::default().fg(Color::DarkGray)));
        }
    }

    // Error states
    if current_ord == 99 {
        spans.clear();
        spans.push(Span::styled(
            format!(" {} {}", current.symbol(), current.label()),
            Style::default().fg(Color::Red).bold(),
        ));
    }

    vec![
        Line::from(Span::styled(
            format!(" Status: {}", current.label()),
            status_color(current).bold(),
        )),
        Line::from(spans),
    ]
}
