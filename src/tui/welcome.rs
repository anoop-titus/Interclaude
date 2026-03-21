use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::App;

const LOGO: &str = r#"
  ___       _                 _                 _
 |_ _|_ __ | |_ ___ _ __ ___| | __ _ _   _  __| | ___
  | || '_ \| __/ _ \ '__/ __| |/ _` | | | |/ _` |/ _ \
  | || | | | ||  __/ | | (__| | (_| | |_| | (_| |  __/
 |___|_| |_|\__\___|_|  \___|_|\__,_|\__,_|\__,_|\___|
"#;

const LOGO_SMALL: &str = "[ Interclaude ]";

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let width = area.width;
    let height = area.height;

    // Adapt logo size to terminal width
    let use_full_logo = width >= 60;
    let logo_height = if use_full_logo { 8 } else { 3 };

    // Adapt margins to terminal size
    let margin = if width >= 80 { 2 } else { 1 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(margin)
        .constraints([
            Constraint::Length(logo_height),
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);

    // Logo — use small version on narrow terminals
    let logo_text = if use_full_logo { LOGO } else { LOGO_SMALL };
    let logo = Paragraph::new(logo_text)
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(logo, chunks[0]);

    // Subtitle
    let subtitle = Paragraph::new("Cross-Machine Claude Code Bridge")
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(Block::default());
    frame.render_widget(subtitle, chunks[1]);

    // Dependency checks — truncate version text to available width
    let inner_width = chunks[2].width.saturating_sub(4) as usize; // borders + padding
    let version_max = inner_width.saturating_sub(20); // space for icon + name

    let mut dep_lines: Vec<Line> = vec![
        Line::from(Span::styled(
            " System Dependencies",
            Style::default().fg(Color::Yellow).bold(),
        )),
        Line::from(""),
    ];

    if app.dep_checks.is_empty() {
        dep_lines.push(Line::from(Span::styled(
            "  Checking dependencies...",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for dep in &app.dep_checks {
            let (icon, style) = if dep.available {
                ("[+]", Style::default().fg(Color::Green))
            } else {
                ("[-]", Style::default().fg(Color::Red))
            };

            let version_text = dep
                .version
                .as_ref()
                .map(|v| {
                    let truncated = if v.len() > version_max && version_max > 3 {
                        format!("{}...", &v[..version_max.saturating_sub(3)])
                    } else {
                        v.clone()
                    };
                    format!(" ({})", truncated)
                })
                .unwrap_or_default();

            dep_lines.push(Line::from(vec![
                Span::styled(format!("  {} ", icon), style),
                Span::styled(&dep.name, style.bold()),
                Span::styled(version_text, Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    let deps_block = Paragraph::new(dep_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Preflight Check "),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(deps_block, chunks[2]);

    // Navigation — adapt to width
    let nav_line = if width >= 50 {
        Line::from(vec![
            Span::styled(" [Enter] ", Style::default().fg(Color::Cyan).bold()),
            Span::raw("Continue  "),
            Span::styled(" [r] ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Re-check  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Red).bold()),
            Span::raw("Quit"),
        ])
    } else {
        Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(Color::Cyan).bold()),
            Span::raw(" "),
            Span::styled("[r]", Style::default().fg(Color::Yellow).bold()),
            Span::raw(" "),
            Span::styled("[Esc]", Style::default().fg(Color::Red).bold()),
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
    frame.render_widget(nav, chunks[3]);
}
