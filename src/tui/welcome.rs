use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::App;
use super::status_bar;

const LOGO: &str = r#"
  ___       _                 _                 _
 |_ _|_ __ | |_ ___ _ __ ___| | __ _ _   _  __| | ___
  | || '_ \| __/ _ \ '__/ __| |/ _` | | | |/ _` |/ _ \
  | || | | | ||  __/ | | (__| | (_| | |_| | (_| |  __/
 |___|_| |_|\__\___|_|  \___|_|\__,_|\__,_|\__,_|\___|
"#;

const LOGO_SMALL: &str = "[ Interclaude ]";

const SPINNER: [char; 4] = ['|', '/', '-', '\\'];

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let width = area.width;

    let use_full_logo = width >= 60;
    let logo_height = if use_full_logo { 8 } else { 3 };
    let margin = if width >= 80 { 2 } else { 1 };

    // Banner height: 2 lines for status banner (ready/missing)
    let banner_height = if app.dep_check_complete { 3 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(margin)
        .constraints([
            Constraint::Length(2),           // Global status bar
            Constraint::Length(logo_height), // Logo
            Constraint::Length(4),           // Description
            Constraint::Min(6),             // Dependency checks
            Constraint::Length(banner_height), // Status banner
            Constraint::Length(3),           // Navigation
        ])
        .split(area);

    // Global status bar
    status_bar::draw_global_status(frame, app, chunks[0]);

    // Logo
    let logo_text = if use_full_logo { LOGO } else { LOGO_SMALL };
    let logo = Paragraph::new(logo_text)
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(logo, chunks[1]);

    // Description block
    let desc_lines = vec![
        Line::from(Span::styled(
            "Cross-Machine Claude Code Bridge",
            Style::default().fg(Color::White).bold(),
        )),
        Line::from(Span::styled(
            "Send tasks between two Claude Code sessions over SSH/MOSH",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "Transports: rsync | MCP | Redis Pub/Sub",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let desc = Paragraph::new(desc_lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(desc, chunks[2]);

    // Dependency checks — split into Required and Optional
    draw_dep_checks(frame, app, chunks[3]);

    // Status banner (only after checks complete)
    if app.dep_check_complete {
        draw_status_banner(frame, app, chunks[4]);
    }

    // Navigation
    draw_nav(frame, app, chunks[5]);
}

fn draw_dep_checks(frame: &mut Frame, app: &App, area: Rect) {
    let inner_width = area.width.saturating_sub(4) as usize;
    let version_max = inner_width.saturating_sub(24);

    let mut dep_lines: Vec<Line> = Vec::new();

    if !app.dep_check_complete {
        // Loading spinner
        let spinner_char = SPINNER[(app.frame_count as usize / 3) % 4];
        dep_lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", spinner_char),
                Style::default().fg(Color::Yellow).bold(),
            ),
            Span::styled(
                "Checking system dependencies...",
                Style::default().fg(Color::Yellow),
            ),
        ]));
    } else {
        // Required section
        dep_lines.push(Line::from(Span::styled(
            " Required",
            Style::default().fg(Color::White).bold(),
        )));

        let required: Vec<_> = app.dep_checks.iter().filter(|d| d.required).collect();
        for dep in &required {
            dep_lines.push(render_dep_line(dep, version_max, true));
        }

        dep_lines.push(Line::from(""));

        // Optional section
        dep_lines.push(Line::from(Span::styled(
            " Optional",
            Style::default().fg(Color::DarkGray).bold(),
        )));

        let optional: Vec<_> = app.dep_checks.iter().filter(|d| !d.required).collect();
        for dep in &optional {
            dep_lines.push(render_dep_line(dep, version_max, false));
        }
    }

    let deps_block = Paragraph::new(dep_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Preflight Check ")
                .title_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(deps_block, area);
}

fn render_dep_line<'a>(dep: &crate::app::DepCheck, version_max: usize, is_required: bool) -> Line<'a> {
    if dep.available {
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

        Line::from(vec![
            Span::styled("  [+] ", Style::default().fg(Color::Green)),
            Span::styled(dep.name.clone(), Style::default().fg(Color::Green).bold()),
            Span::styled(version_text, Style::default().fg(Color::DarkGray)),
        ])
    } else {
        // Required missing = red, optional missing = yellow
        let color = if is_required { Color::Red } else { Color::Yellow };
        let indicator = if is_required { "[-]" } else { "[~]" };

        Line::from(vec![
            Span::styled(format!("  {} ", indicator), Style::default().fg(color)),
            Span::styled(dep.name.clone(), Style::default().fg(color).bold()),
            Span::styled(
                format!("  {}", dep.install_hint),
                Style::default().fg(Color::Yellow),
            ),
        ])
    }
}

fn draw_status_banner(frame: &mut Frame, app: &App, area: Rect) {
    if app.all_required_met() {
        let banner_text = if let Some(ticks) = app.auto_advance_ticks {
            let secs_left = (ticks + 9) / 10; // round up
            format!("  All required dependencies found! Continuing in {}s...", secs_left)
        } else {
            "  All required dependencies found! Press Enter to continue...".to_string()
        };

        let banner = Paragraph::new(Line::from(Span::styled(
            banner_text,
            Style::default().fg(Color::Green).bold(),
        )))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        );
        frame.render_widget(banner, area);
    } else {
        let missing = app.missing_required_deps();
        let missing_names: Vec<&str> = missing.iter().map(|d| d.name.as_str()).collect();
        let banner_text = format!(
            "  Missing required: {}. Install before proceeding.",
            missing_names.join(", ")
        );

        let banner = Paragraph::new(Line::from(Span::styled(
            banner_text,
            Style::default().fg(Color::Red).bold(),
        )))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        );
        frame.render_widget(banner, area);
    }
}

fn draw_nav(frame: &mut Frame, _app: &App, area: Rect) {
    let width = area.width;

    let nav_line = if width >= 50 {
        Line::from(vec![
            Span::styled(" [Enter] ", Style::default().fg(Color::Cyan).bold()),
            Span::raw("Continue  "),
            Span::styled(" [r] ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Re-check  "),
            Span::styled(" [Ctrl+Q] ", Style::default().fg(Color::Red).bold()),
            Span::raw("Quit"),
        ])
    } else {
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Cyan).bold()),
            Span::styled(":Go ", Style::default().fg(Color::DarkGray)),
            Span::styled("r", Style::default().fg(Color::Yellow).bold()),
            Span::styled(":Check ", Style::default().fg(Color::DarkGray)),
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
