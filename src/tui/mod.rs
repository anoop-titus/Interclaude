mod bridge;
mod setup;
mod welcome;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::app::{App, DeliveryStatus, Page, SetupField};
use crate::bridge::connection;
use crate::bridge::engine::{BridgeEngine, BridgeEvent};
use crate::config::Role;
use crate::transport::TransportKind;

pub async fn run(app: &mut App) -> Result<()> {
    // Load saved config
    app.settings = crate::config::Settings::load();

    // Check dependencies on startup
    check_dependencies(app).await;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let mut bridge_engine: Option<Arc<BridgeEngine>> = None;
    let mut event_rx: Option<mpsc::Receiver<BridgeEvent>> = None;
    // Keep tunnel handle alive so it doesn't get dropped
    let mut _tunnel_handle: Option<tokio::process::Child> = None;

    while app.running {
        // Process bridge events if engine is running
        if let Some(rx) = &mut event_rx {
            while let Ok(event) = rx.try_recv() {
                process_bridge_event(app, event);
            }
        }

        terminal.draw(|frame| draw(frame, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let action = handle_input(app, key.code, key.modifiers).await;

                match action {
                    InputAction::None => {}
                    InputAction::StartBridge => {
                        // Initialize bridge engine when entering bridge page
                        if bridge_engine.is_none() {
                            let (tx, rx) = mpsc::channel(256);
                            match BridgeEngine::new(app.settings.clone(), tx) {
                                Ok(engine) => {
                                    let engine = Arc::new(engine);

                                    // Start autossh tunnel
                                    match engine.start_tunnel().await {
                                        Ok(child) => {
                                            _tunnel_handle = child;
                                        }
                                        Err(e) => {
                                            app.bridge_log.push(format!("Tunnel failed: {e} (continuing)"));
                                        }
                                    }

                                    // Start background tasks
                                    engine.start_health_monitor();
                                    engine.start_receive_loop();
                                    engine.start_heartbeat();

                                    // Start Redis subscriber if needed
                                    engine.start_redis_subscriber_if_active();

                                    // Send handshake
                                    {
                                        let eng = engine.clone();
                                        tokio::spawn(async move {
                                            if let Err(e) = eng.send_handshake().await {
                                                eprintln!("Handshake send failed: {e}");
                                            }
                                        });
                                    }

                                    bridge_engine = Some(engine);
                                    event_rx = Some(rx);
                                    app.connection_status = "Connecting...".to_string();
                                    app.bridge_log.push("Bridge engine started".to_string());
                                }
                                Err(e) => {
                                    app.bridge_log.push(format!("Engine init failed: {e}"));
                                }
                            }
                        }
                    }
                    InputAction::SwitchTransport(kind) => {
                        if let Some(engine) = &bridge_engine {
                            let engine = engine.clone();
                            let kind = kind;
                            tokio::spawn(async move {
                                let _ = engine.switch_transport(kind).await;
                            });
                        }
                    }
                    InputAction::SendCommand(task) => {
                        if let Some(engine) = &bridge_engine {
                            let engine = engine.clone();
                            tokio::spawn(async move {
                                if let Err(e) = engine.send_command(task).await {
                                    eprintln!("Send failed: {e}");
                                }
                            });
                        }
                    }
                    InputAction::LaunchSlave => {
                        if let Some(engine) = &bridge_engine {
                            let engine = engine.clone();
                            tokio::spawn(async move {
                                if let Err(e) = engine.launch_slave().await {
                                    eprintln!("Slave launch failed: {e}");
                                }
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Actions that input handling can request
enum InputAction {
    None,
    StartBridge,
    SwitchTransport(TransportKind),
    SendCommand(String),
    LaunchSlave,
}

fn process_bridge_event(app: &mut App, event: BridgeEvent) {
    match event {
        BridgeEvent::MessageSent(entry) => {
            app.messages.push(entry);
        }
        BridgeEvent::MessageReceived(entry) => {
            app.messages.push(entry);
        }
        BridgeEvent::CommandReceived(_msg) => {
            // In TUI mode (master), we don't execute commands locally
            // This is handled by the slave's event loop in main.rs
        }
        BridgeEvent::HealthUpdate(kind, healthy) => {
            let idx = match kind {
                TransportKind::Rsync => 0,
                TransportKind::Mcp => 1,
                TransportKind::Redis => 2,
            };
            app.transport_health[idx] = healthy;
        }
        BridgeEvent::ConnectionStatus(status) => {
            app.connection_status = status;
        }
        BridgeEvent::StatusUpdate(msg_id, status) => {
            // Update the status of an existing message
            for msg in &mut app.messages {
                if msg.msg_id == msg_id {
                    msg.status = status;
                }
            }
        }
        BridgeEvent::Log(msg) => {
            app.bridge_log.push(msg);
            // Keep log at reasonable size
            if app.bridge_log.len() > 100 {
                app.bridge_log.drain(0..50);
            }
        }
        BridgeEvent::TransportSwitched(kind) => {
            app.active_transport = kind;
        }
        BridgeEvent::RoleConfirmed(role) => {
            app.bridge_log.push(format!("Role confirmed: {:?}", role));
            app.connection_status = format!("Connected ({:?})", role);
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    match app.page {
        Page::Welcome => welcome::draw(frame, app),
        Page::Setup => setup::draw(frame, app),
        Page::Bridge => bridge::draw(frame, app),
    }
}

async fn handle_input(app: &mut App, key: KeyCode, modifiers: KeyModifiers) -> InputAction {
    // If composing a command, handle compose input
    if app.composing {
        return handle_compose_input(app, key, modifiers);
    }

    // Global keybindings
    if modifiers.contains(KeyModifiers::CONTROL) {
        match key {
            KeyCode::Char('q') => {
                app.running = false;
                return InputAction::None;
            }
            KeyCode::Char('s') => {
                match app.settings.save() {
                    Ok(()) => app.setup_log.push("Config saved.".to_string()),
                    Err(e) => app.setup_log.push(format!("Save failed: {e}")),
                }
                return InputAction::None;
            }
            _ => {}
        }
    }

    match key {
        KeyCode::Esc => {
            if app.page != Page::Welcome {
                app.prev_page();
            } else {
                app.running = false;
            }
            InputAction::None
        }
        _ => match app.page {
            Page::Welcome => {
                handle_welcome_input(app, key).await;
                InputAction::None
            }
            Page::Setup => handle_setup_input(app, key, modifiers).await,
            Page::Bridge => handle_bridge_input(app, key, modifiers),
        },
    }
}

fn handle_compose_input(app: &mut App, key: KeyCode, modifiers: KeyModifiers) -> InputAction {
    match key {
        KeyCode::Esc => {
            app.composing = false;
            app.compose_input.clear();
            InputAction::None
        }
        KeyCode::Enter => {
            if !app.compose_input.is_empty() {
                let task = app.compose_input.clone();
                app.compose_input.clear();
                app.composing = false;
                InputAction::SendCommand(task)
            } else {
                InputAction::None
            }
        }
        KeyCode::Char(c) => {
            app.compose_input.push(c);
            InputAction::None
        }
        KeyCode::Backspace => {
            app.compose_input.pop();
            InputAction::None
        }
        _ => InputAction::None,
    }
}

async fn handle_welcome_input(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Enter => app.next_page(),
        KeyCode::Char('r') => {
            check_dependencies(app).await;
        }
        _ => {}
    }
}

async fn handle_setup_input(app: &mut App, key: KeyCode, modifiers: KeyModifiers) -> InputAction {
    match key {
        KeyCode::Tab => app.setup_field = app.setup_field.next(),
        KeyCode::BackTab => app.setup_field = app.setup_field.prev(),
        KeyCode::Enter => {
            if app.setup_field == SetupField::Connection {
                app.settings.connection = app.settings.connection.cycle();
            } else if app.setup_field == SetupField::Transport {
                app.settings.active_transport = match app.settings.active_transport {
                    TransportKind::Rsync => TransportKind::Mcp,
                    TransportKind::Mcp => TransportKind::Redis,
                    TransportKind::Redis => TransportKind::Rsync,
                };
            } else if modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+Enter = save config, setup dirs, proceed to bridge
                app.setup_log.push("Saving configuration...".to_string());
                match app.settings.save() {
                    Ok(()) => app.setup_log.push("Config saved.".to_string()),
                    Err(e) => {
                        app.setup_log.push(format!("Save failed: {e}"));
                        return InputAction::None;
                    }
                }

                app.setup_log.push("Creating local directories...".to_string());
                match connection::setup_local_dirs(&app.settings) {
                    Ok(msg) => app.setup_log.push(msg),
                    Err(e) => {
                        app.setup_log.push(format!("Local setup failed: {e}"));
                        return InputAction::None;
                    }
                }

                app.setup_log.push("Creating remote directories...".to_string());
                match connection::setup_remote_dirs(&app.settings).await {
                    Ok(_msg) => {
                        app.setup_log.push("Remote dirs OK.".to_string());
                    }
                    Err(e) => {
                        app.setup_log
                            .push(format!("Remote setup failed: {e}"));
                        app.setup_log
                            .push("Proceeding anyway (set up remote manually).".to_string());
                    }
                }

                app.active_transport = app.settings.active_transport;
                app.next_page();
                return InputAction::StartBridge;
            }
        }
        KeyCode::Char(c) => {
            if !app.setup_field.is_selector() {
                let mut val = app.settings.get_field(&app.setup_field);
                if app.setup_field == SetupField::RedisPassword {
                    val = app.settings.redis.password.clone();
                }
                val.push(c);
                app.settings.set_field(&app.setup_field, &val);
            }
        }
        KeyCode::Backspace => {
            if !app.setup_field.is_selector() {
                let mut val = app.settings.get_field(&app.setup_field);
                if app.setup_field == SetupField::RedisPassword {
                    val = app.settings.redis.password.clone();
                }
                val.pop();
                app.settings.set_field(&app.setup_field, &val);
            }
        }
        KeyCode::F(2) => {
            app.ssh_test_running = true;
            app.setup_log.push(format!(
                "Testing {} connection to {}...",
                app.settings.connection.label(),
                app.settings.remote_host
            ));

            let result = connection::test_connection(&app.settings).await;
            app.ssh_test_running = false;

            if result.success {
                app.setup_log.push(format!("OK: {}", result.output));
            } else {
                app.setup_log.push(format!("FAIL: {}", result.output));
            }
        }
        KeyCode::F(3) => {
            app.setup_log.push("Generating SSH key...".to_string());
            let key_path = crate::config::Settings::expand_path(&app.settings.key_path);
            if std::path::Path::new(&key_path).exists() {
                app.setup_log
                    .push(format!("Key already exists at {key_path}"));
            } else {
                match tokio::process::Command::new("ssh-keygen")
                    .args([
                        "-t", "ed25519", "-f", &key_path, "-N", "", "-C", "interclaude",
                    ])
                    .output()
                    .await
                {
                    Ok(output) => {
                        if output.status.success() {
                            app.setup_log
                                .push(format!("Key generated at {key_path}"));
                            app.setup_log.push(
                                "Copy public key to remote: ssh-copy-id".to_string(),
                            );
                        } else {
                            let err = String::from_utf8_lossy(&output.stderr);
                            app.setup_log.push(format!("Key gen failed: {err}"));
                        }
                    }
                    Err(e) => app.setup_log.push(format!("ssh-keygen error: {e}")),
                }
            }
        }
        _ => {}
    }

    InputAction::None
}

fn handle_bridge_input(app: &mut App, key: KeyCode, modifiers: KeyModifiers) -> InputAction {
    // Ctrl+N = compose new command
    if modifiers.contains(KeyModifiers::CONTROL) && key == KeyCode::Char('n') {
        app.composing = true;
        app.compose_input.clear();
        return InputAction::None;
    }

    // Ctrl+L = launch slave watcher on remote
    if modifiers.contains(KeyModifiers::CONTROL) && key == KeyCode::Char('l') {
        return InputAction::LaunchSlave;
    }

    match key {
        KeyCode::Char('1') => {
            app.set_transport(TransportKind::Rsync);
            InputAction::SwitchTransport(TransportKind::Rsync)
        }
        KeyCode::Char('2') => {
            app.set_transport(TransportKind::Mcp);
            InputAction::SwitchTransport(TransportKind::Mcp)
        }
        KeyCode::Char('3') => {
            app.set_transport(TransportKind::Redis);
            InputAction::SwitchTransport(TransportKind::Redis)
        }
        KeyCode::Up => {
            if let Some(sel) = app.selected_message {
                if sel > 0 {
                    app.selected_message = Some(sel - 1);
                }
            }
            InputAction::None
        }
        KeyCode::Down => {
            if let Some(sel) = app.selected_message {
                if sel < app.messages.len().saturating_sub(1) {
                    app.selected_message = Some(sel + 1);
                }
            } else if !app.messages.is_empty() {
                app.selected_message = Some(0);
            }
            InputAction::None
        }
        _ => InputAction::None,
    }
}

async fn check_dependencies(app: &mut App) {
    let deps = vec![
        ("ssh", "ssh -V"),
        ("mosh", "mosh --version"),
        ("autossh", "autossh -V"),
        ("rsync", "rsync --version"),
        ("fswatch", "fswatch --version"),
        ("redis-cli", "redis-cli --version"),
        ("claude", "claude --version"),
    ];

    app.dep_checks.clear();
    for (name, cmd) in deps {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let result = tokio::process::Command::new(parts[0])
            .args(&parts[1..])
            .output()
            .await;

        let (available, version) = match result {
            Ok(output) => {
                let ver = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let ver = if ver.is_empty() {
                    String::from_utf8_lossy(&output.stderr)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string()
                } else {
                    ver
                };
                (output.status.success() || !ver.is_empty(), Some(ver))
            }
            Err(_) => (false, None),
        };

        app.dep_checks.push(crate::app::DepCheck {
            name: name.to_string(),
            available,
            version,
        });
    }
}
