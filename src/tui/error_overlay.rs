use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::error::analysis::{AnalysisResult, FixType};

/// Draw the error analysis overlay popup
pub fn draw(frame: &mut Frame, analysis: &AnalysisResult, area: Rect) {
    // Calculate overlay dimensions — smaller than terminal
    let max_width = 60.min(area.width.saturating_sub(6));
    let content_lines = build_content(analysis, max_width as usize);
    let overlay_height = (content_lines.len() as u16 + 4).min(area.height.saturating_sub(4)); // +4 for borders + action bar

    let x = (area.width.saturating_sub(max_width)) / 2;
    let y = (area.height.saturating_sub(overlay_height)) / 2;
    let overlay_area = Rect::new(x, y, max_width, overlay_height);

    // Clear background
    frame.render_widget(Clear, overlay_area);

    // Build inner layout
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3), // Content
            Constraint::Length(2), // Action bar
        ])
        .split(overlay_area.inner(Margin::new(1, 1)));

    // Title color based on fix type
    let (title, title_color) = match analysis.fix_type {
        FixType::InSession => (" Error Analysis (fixable) ", Color::Yellow),
        FixType::OutOfSession => (" Error Analysis (restart needed) ", Color::Red),
    };

    // Outer border
    let border = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(title_color))
        .title(title)
        .title_style(Style::default().fg(title_color).bold());
    frame.render_widget(border, overlay_area);

    // Content
    let content = Paragraph::new(content_lines)
        .wrap(Wrap { trim: true });
    frame.render_widget(content, inner[0]);

    // Action bar
    let actions = match analysis.fix_type {
        FixType::InSession => Line::from(vec![
            Span::styled(" [Y] ", Style::default().fg(Color::Green).bold()),
            Span::raw("Apply Fix  "),
            Span::styled(" [N] ", Style::default().fg(Color::Red).bold()),
            Span::raw("Dismiss  "),
            Span::styled(" [D] ", Style::default().fg(Color::Cyan).bold()),
            Span::raw("Details"),
        ]),
        FixType::OutOfSession => Line::from(vec![
            Span::styled(" [Y] ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Queue for restart  "),
            Span::styled(" [N] ", Style::default().fg(Color::Red).bold()),
            Span::raw("Dismiss"),
        ]),
    };
    let action_bar = Paragraph::new(actions)
        .alignment(Alignment::Center);
    frame.render_widget(action_bar, inner[1]);
}

fn build_content(analysis: &AnalysisResult, max_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Error summary
    lines.push(Line::from(Span::styled(
        analysis.summary.clone(),
        Style::default().fg(Color::White).bold(),
    )));
    lines.push(Line::from(""));

    // Source info
    lines.push(Line::from(vec![
        Span::styled("Source: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} / {}", analysis.original_error.category.label(), analysis.original_error.source),
            Style::default().fg(Color::Yellow),
        ),
    ]));

    // Fix type
    let fix_label = match analysis.fix_type {
        FixType::InSession => "In-session (can fix now)",
        FixType::OutOfSession => "Out-of-session (requires restart)",
    };
    let fix_color = match analysis.fix_type {
        FixType::InSession => Color::Green,
        FixType::OutOfSession => Color::Red,
    };
    lines.push(Line::from(vec![
        Span::styled("Fix: ", Style::default().fg(Color::DarkGray)),
        Span::styled(fix_label.to_string(), Style::default().fg(fix_color).bold()),
    ]));

    // Confidence
    let conf_pct = (analysis.confidence * 100.0) as u8;
    let conf_color = if conf_pct >= 80 {
        Color::Green
    } else if conf_pct >= 50 {
        Color::Yellow
    } else {
        Color::Red
    };
    lines.push(Line::from(vec![
        Span::styled("Confidence: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}%", conf_pct), Style::default().fg(conf_color)),
    ]));

    lines.push(Line::from(""));

    // Suggested action
    lines.push(Line::from(Span::styled(
        "Suggested action:",
        Style::default().fg(Color::Cyan).bold(),
    )));

    // Word-wrap the suggested action text
    let action_text = &analysis.suggested_action;
    let wrap_width = max_width.saturating_sub(2);
    for chunk in wrap_text(action_text, wrap_width) {
        lines.push(Line::from(Span::styled(
            format!("  {}", chunk),
            Style::default().fg(Color::White),
        )));
    }

    lines
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 || text.len() <= max_width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_width {
            lines.push(remaining.to_string());
            break;
        }

        let break_at = remaining[..max_width]
            .rfind(' ')
            .unwrap_or(max_width);
        let break_at = if break_at == 0 { max_width } else { break_at };

        lines.push(remaining[..break_at].to_string());
        remaining = remaining[break_at..].trim_start();
    }

    lines
}
