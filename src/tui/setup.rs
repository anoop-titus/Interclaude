use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, SetupField};
use super::status_bar;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let width = area.width;

    if width < 80 {
        draw_narrow(frame, app, area);
    } else {
        draw_wide(frame, app, area);
    }
}

fn draw_wide(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width;
    let left_pct = if width >= 120 { 55 } else { 60 };
    let right_pct = 100 - left_pct;

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2),  // Global status bar + tabs
            Constraint::Min(10),
        ])
        .split(area);

    status_bar::draw_global_status(frame, app, outer[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(outer[1]);

    draw_left_panel(frame, app, main_chunks[0]);
    draw_tutorial(frame, app, main_chunks[1]);
}

fn draw_narrow(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height;
    let show_tutorial = height >= 30;

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2),  // Global status bar + tabs
            Constraint::Min(10),
        ])
        .split(area);

    status_bar::draw_global_status(frame, app, outer[0]);

    let constraints = if show_tutorial {
        vec![Constraint::Percentage(65), Constraint::Percentage(35)]
    } else {
        vec![Constraint::Percentage(100)]
    };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(outer[1]);

    draw_left_panel(frame, app, main_chunks[0]);

    if show_tutorial && main_chunks.len() > 1 {
        draw_tutorial(frame, app, main_chunks[1]);
    }
}

fn draw_left_panel(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height;
    let log_height = if height >= 25 { 8 } else if height >= 18 { 5 } else { 3 };
    let nav_height = 3;

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(log_height),
            Constraint::Length(nav_height),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(" Setup Connection")
        .style(Style::default().fg(Color::Yellow).bold())
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::Cyan)),
        );
    frame.render_widget(title, left_chunks[0]);

    // Form fields
    draw_form(frame, app, left_chunks[1]);

    // Scrollable log panel
    draw_scrollable_log(frame, app, left_chunks[2]);

    // Navigation
    draw_setup_nav(frame, app, left_chunks[3]);
}

fn draw_scrollable_log(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let total_lines = app.setup_log.len();

    let max_scroll = total_lines.saturating_sub(inner_height);
    let scroll = if app.setup_log_scroll == u16::MAX {
        max_scroll
    } else {
        (app.setup_log_scroll as usize).min(max_scroll)
    };

    let log_lines: Vec<Line> = app
        .setup_log
        .iter()
        .skip(scroll)
        .take(inner_height)
        .map(|line| {
            let color = if line.starts_with("OK:") || line.starts_with("Push OK") {
                Color::Green
            } else if line.starts_with("FAIL:") || line.contains("failed") {
                Color::Red
            } else if line.starts_with("===") {
                Color::Yellow
            } else {
                Color::DarkGray
            };
            Line::from(Span::styled(line.as_str(), Style::default().fg(color)))
        })
        .collect();

    let scroll_info = if total_lines > inner_height {
        format!(" Log [{}/{}] ", scroll + inner_height.min(total_lines), total_lines)
    } else {
        " Log ".to_string()
    };

    let log = Paragraph::new(log_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(scroll_info)
                .title_style(Style::default().fg(Color::Yellow)),
        );
    frame.render_widget(log, area);
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    // Render outer block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Configuration ")
        .title_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let show_redis = app.show_redis_config();

    // Build all lines dynamically
    let mut lines: Vec<Line> = Vec::new();

    // ── Connection ──────────
    lines.push(section_header("Connection", Color::Cyan, inner.width));

    let conn_fields = [
        (SetupField::RemoteHost, "Remote Host", "e.g., 192.168.1.100"),
        (SetupField::Connection, "Connection", "[Enter] cycle: MOSH/SSH"),
        (SetupField::SshUser, "User", "e.g., root"),
        (SetupField::SshPort, "Port", "default: 22"),
        (SetupField::KeyPath, "Key Path", "~/.ssh/id_ed25519"),
        (SetupField::RemoteDir, "Remote Dir", "~/Interclaude"),
    ];

    let value_max = inner.width.saturating_sub(24) as usize;

    for (field, label, placeholder) in &conn_fields {
        lines.push(render_field(app, *field, label, placeholder, value_max));
    }

    // ── Transport ──────────
    lines.push(Line::from("")); // spacer
    lines.push(section_header("Transport", Color::Magenta, inner.width));

    lines.push(render_field(
        app,
        SetupField::Transport,
        "Transport",
        "[Enter] to cycle",
        value_max,
    ));

    // ── Redis Configuration ────────── (conditional)
    if show_redis {
        lines.push(Line::from("")); // spacer
        lines.push(section_header("Redis Configuration", Color::Yellow, inner.width));

        let redis_fields = [
            (SetupField::RedisHost, "Redis Host", "127.0.0.1"),
            (SetupField::RedisPort, "Redis Port", "6379"),
            (SetupField::RedisPassword, "Redis Pass", "(optional)"),
        ];

        for (field, label, placeholder) in &redis_fields {
            lines.push(render_field(app, *field, label, placeholder, value_max));
        }
    }

    // Spacer + Activate button (centered, prominent)
    lines.push(Line::from("")); // spacer
    let has_config = app.has_remote_config();
    let btn_style = if has_config {
        Style::default().fg(Color::Black).bg(Color::Green).bold()
    } else {
        Style::default().fg(Color::DarkGray).bg(Color::DarkGray)
    };
    let btn_label = if has_config {
        "  [ Ctrl+A  Activate ]  "
    } else {
        "  [ Activate — fill host+user first ]  "
    };
    lines.push(Line::from(Span::styled(btn_label, btn_style)).alignment(Alignment::Center));

    let form = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(form, inner);
}

/// Render a "── Label ──────" section header
fn section_header<'a>(label: &str, color: Color, width: u16) -> Line<'a> {
    let prefix = format!("── {} ", label);
    let remaining = (width as usize).saturating_sub(prefix.len() + 1);
    let rule = "─".repeat(remaining);
    Line::from(Span::styled(
        format!("{}{}", prefix, rule),
        Style::default().fg(color).bold(),
    ))
}

/// Render a single form field line with cursor, label, value/placeholder, and validation indicator
fn render_field<'a>(
    app: &App,
    field: SetupField,
    label: &str,
    placeholder: &str,
    value_max: usize,
) -> Line<'a> {
    let is_focused = app.setup_field == field;
    let is_selector = field.is_selector();
    let value = app.settings.get_field(&field);

    // Build display text
    let display_text = if value.is_empty() {
        let ph = if placeholder.len() > value_max && value_max > 3 {
            format!("{}...", &placeholder[..value_max.saturating_sub(3)])
        } else {
            placeholder.to_string()
        };
        Span::styled(ph, Style::default().fg(Color::DarkGray))
    } else {
        let v = if value.len() > value_max && value_max > 3 {
            format!("{}...", &value[..value_max.saturating_sub(3)])
        } else {
            value.clone()
        };
        let color = if is_selector { Color::Magenta } else { Color::White };
        Span::styled(v, Style::default().fg(color))
    };

    let label_style = if is_focused {
        Style::default().fg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::Gray)
    };

    let cursor = if is_focused { "> " } else { "  " };
    let cursor_color = if is_focused { Color::Yellow } else { Color::DarkGray };

    // Validation indicator
    let validation = validate_field(app, field);

    let mut spans = vec![
        Span::styled(cursor.to_string(), Style::default().fg(cursor_color)),
        Span::styled(format!("{:<14}", label), label_style),
        display_text,
    ];

    // Append validation indicator if non-empty field
    match validation {
        Some(true) => {
            spans.push(Span::styled(" ok", Style::default().fg(Color::Green)));
        }
        Some(false) => {
            spans.push(Span::styled(" ?", Style::default().fg(Color::Yellow).bold()));
        }
        None => {}
    }

    Line::from(spans)
}

/// Validate a field value. Returns None for empty/non-validated fields,
/// Some(true) for valid, Some(false) for invalid.
fn validate_field(app: &App, field: SetupField) -> Option<bool> {
    match field {
        SetupField::RemoteHost => {
            let v = &app.settings.remote_host;
            if v.is_empty() { return None; }
            Some(!v.contains(' ') && !v.starts_with('/') && !v.contains('\t'))
        }
        SetupField::SshPort => {
            let v = app.settings.ssh_port;
            // Port is parsed as u16 already, so it's always 0-65535
            // But check that it's >= 1
            Some(v >= 1)
        }
        SetupField::KeyPath => {
            let v = &app.settings.key_path;
            if v.is_empty() { return None; }
            let expanded = crate::config::Settings::expand_path(v);
            Some(std::path::Path::new(&expanded).exists())
        }
        SetupField::RedisHost => {
            let v = &app.settings.redis.host;
            if v.is_empty() { return None; }
            Some(!v.contains(' ') && !v.starts_with('/'))
        }
        SetupField::RedisPort => {
            let v = app.settings.redis.port;
            Some(v >= 1)
        }
        _ => None,
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

fn draw_setup_nav(frame: &mut Frame, app: &App, area: Rect) {
    let has_config = app.has_remote_config();

    let mut spans: Vec<Span> = Vec::new();

    // Simplified: only 4 essential keys
    spans.push(Span::styled("[Tab] ", Style::default().fg(Color::Cyan).bold()));
    spans.push(Span::raw("Navigate  "));

    if has_config {
        spans.push(Span::styled("[C-A] ", Style::default().fg(Color::Green).bold()));
        spans.push(Span::raw("Activate  "));
    } else {
        spans.push(Span::styled("[C-A] ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled("Activate  ", Style::default().fg(Color::DarkGray)));
    }

    spans.push(Span::styled("[C-S] ", Style::default().fg(Color::Blue).bold()));
    spans.push(Span::raw("Save  "));

    spans.push(Span::styled("[Esc] ", Style::default().fg(Color::Red).bold()));
    spans.push(Span::raw("Back"));

    let nav = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(nav, area);
}
