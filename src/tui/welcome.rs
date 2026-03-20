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

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(8),  // Logo
            Constraint::Length(3),  // Subtitle
            Constraint::Min(8),    // Dependency checks
            Constraint::Length(3), // Navigation help
        ])
        .split(area);

    // Logo
    let logo = Paragraph::new(LOGO)
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center);
    frame.render_widget(logo, chunks[0]);

    // Subtitle
    let subtitle = Paragraph::new("Cross-Machine Claude Code Bridge")
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center)
        .block(Block::default());
    frame.render_widget(subtitle, chunks[1]);

    // Dependency checks
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
                    if v.len() > 50 {
                        format!(" ({}...)", &v[..47])
                    } else {
                        format!(" ({})", v)
                    }
                })
                .unwrap_or_default();

            dep_lines.push(Line::from(vec![
                Span::styled(format!("  {} ", icon), style),
                Span::styled(&dep.name, style.bold()),
                Span::styled(version_text, Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    let deps_block = Paragraph::new(dep_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Preflight Check "),
    );
    frame.render_widget(deps_block, chunks[2]);

    // Navigation
    let nav = Paragraph::new(Line::from(vec![
        Span::styled(" [Enter] ", Style::default().fg(Color::Cyan).bold()),
        Span::raw("Continue to Setup  "),
        Span::styled(" [r] ", Style::default().fg(Color::Yellow).bold()),
        Span::raw("Re-check  "),
        Span::styled(" [Esc] ", Style::default().fg(Color::Red).bold()),
        Span::raw("Quit"),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(nav, chunks[3]);
}
