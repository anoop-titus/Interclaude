mod bridge;
mod setup;
pub mod status_bar;
mod welcome;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers, MouseEventKind, EnableMouseCapture, DisableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::app::{App, Page, SetupField};
use crate::bridge::connection;
use crate::bridge::engine::{BridgeEngine, BridgeEvent};
use crate::transport::TransportKind;

pub async fn run(app: &mut App) -> Result<()> {
    // Load saved config
    app.settings = crate::config::Settings::load();

    // Check dependencies on startup
    check_dependencies(app).await;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
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
    let mut _tunnel_handles: Vec<tokio::process::Child> = Vec::new();

    while app.running {
        // Increment frame counter (used for spinner animation, etc.)
        app.frame_count = app.frame_count.wrapping_add(1);

        // Process bridge events if engine is running
        if let Some(rx) = &mut event_rx {
            while let Ok(evt) = rx.try_recv() {
                process_bridge_event(app, evt);
            }
        }

        // Auto-advance timer on Welcome page
        if app.page == Page::Welcome && app.dep_check_complete && app.all_required_met() {
            match &mut app.auto_advance_ticks {
                Some(ticks) => {
                    if *ticks == 0 {
                        app.auto_advance_ticks = None;
                        app.next_page();
                    } else {
                        *ticks -= 1;
                    }
                }
                None => {
                    // Start 2-second countdown (20 ticks at 100ms each)
                    app.auto_advance_ticks = Some(20);
                }
            }
        }

        terminal.draw(|frame| draw(frame, app))?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    let action = handle_input(app, key.code, key.modifiers).await;
                    let result = execute_action(app, action, &mut bridge_engine, &mut event_rx, &mut _tunnel_handles).await;
                    // If activate returned a StartBridge, execute that too
                    if let Some(follow_up) = result {
                        let _ = execute_action(app, follow_up, &mut bridge_engine, &mut event_rx, &mut _tunnel_handles).await;
                    }
                }
                Event::Mouse(mouse) => {
                    handle_mouse(app, mouse);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Execute an InputAction, return an optional follow-up action
async fn execute_action(
    app: &mut App,
    action: InputAction,
    bridge_engine: &mut Option<Arc<BridgeEngine>>,
    event_rx: &mut Option<mpsc::Receiver<BridgeEvent>>,
    tunnel_handles: &mut Vec<tokio::process::Child>,
) -> Option<InputAction> {
    match action {
        InputAction::None => None,
        InputAction::StartBridge => {
            if bridge_engine.is_none() {
                let (tx, rx) = mpsc::channel(256);
                match BridgeEngine::new(app.settings.clone(), tx) {
                    Ok(engine) => {
                        let engine = Arc::new(engine);

                        let handles = engine.start_tunnels().await;
                        *tunnel_handles = handles;

                        engine.start_health_monitor();
                        engine.start_receive_loop();
                        engine.start_heartbeat();
                        engine.start_redis_subscriber_if_active();

                        {
                            let eng = engine.clone();
                            tokio::spawn(async move {
                                if let Err(e) = eng.send_handshake().await {
                                    // Can't eprintln during TUI — logged via engine event
                                    let _ = e;
                                }
                            });
                        }

                        *bridge_engine = Some(engine);
                        *event_rx = Some(rx);
                        app.connection_status = "Connecting...".to_string();
                        app.push_bridge_log("Bridge engine started".to_string());
                    }
                    Err(e) => {
                        app.push_bridge_log(format!("Engine init failed: {e}"));
                    }
                }
            }
            None
        }
        InputAction::SwitchTransport(kind) => {
            if let Some(engine) = bridge_engine {
                let engine = engine.clone();
                tokio::spawn(async move {
                    let _ = engine.switch_transport(kind).await;
                });
            }
            None
        }
        InputAction::SendCommand(task) => {
            if let Some(engine) = bridge_engine {
                let engine = engine.clone();
                tokio::spawn(async move {
                    let _ = engine.send_command(task).await;
                });
            }
            None
        }
        InputAction::LaunchSlave => {
            if let Some(engine) = bridge_engine {
                let engine = engine.clone();
                tokio::spawn(async move {
                    let _ = engine.launch_slave().await;
                });
            }
            None
        }
        InputAction::PushInstall => {
            app.push_setup_log("Pushing Interclaude to remote...".to_string());
            let settings = app.settings.clone();
            match connection::push_install_slave(&settings).await {
                Ok(msg) => {
                    app.push_setup_log(msg);
                    app.push_setup_log("Push OK! Press Ctrl+D to go to Bridge, then Ctrl+L to start slave.".to_string());
                }
                Err(e) => {
                    app.push_setup_log(format!("Push install failed: {e}"));
                }
            }
            None
        }
        InputAction::Activate => {
            // Full auto-sequence: Test → Save → Push Install → Deploy to Bridge
            app.push_setup_log("=== Activating full sequence ===".to_string());

            // Step 1: Test connection
            app.ssh_test_running = true;
            app.push_setup_log(format!(
                "Testing {} connection to {}...",
                app.settings.connection.label(),
                app.settings.remote_host
            ));
            let result = connection::test_connection(&app.settings).await;
            app.ssh_test_running = false;

            if !result.success {
                app.push_setup_log(format!("FAIL: {}", result.output));
                app.push_setup_log("Activation aborted — connection test failed.".to_string());
                return None;
            }
            app.ssh_test_passed = true;
            app.connection_status = "Connected".to_string();
            app.push_setup_log(format!("OK: {}", result.output));

            // Step 2: Save config
            match app.settings.save() {
                Ok(()) => app.push_setup_log("Config saved.".to_string()),
                Err(e) => {
                    app.push_setup_log(format!("Save failed: {e}"));
                    return None;
                }
            }

            // Step 3: Push install
            app.push_setup_log("Pushing binary to remote...".to_string());
            let settings = app.settings.clone();
            match connection::push_install_slave(&settings).await {
                Ok(msg) => app.push_setup_log(msg),
                Err(e) => {
                    app.push_setup_log(format!("Push install failed: {e} (continuing)"));
                }
            }

            // Step 4: Create dirs + go to Bridge
            app.push_setup_log("Creating directories...".to_string());
            match connection::setup_local_dirs(&app.settings) {
                Ok(msg) => app.push_setup_log(msg),
                Err(e) => app.push_setup_log(format!("Local dirs: {e}")),
            }
            match connection::setup_remote_dirs(&app.settings).await {
                Ok(_) => app.push_setup_log("Remote dirs OK.".to_string()),
                Err(e) => app.push_setup_log(format!("Remote dirs: {e} (continuing)")),
            }

            app.active_transport = app.settings.active_transport;
            app.push_setup_log("=== Deploying Bridge ===".to_string());
            app.next_page();
            Some(InputAction::StartBridge)
        }
    }
}

fn handle_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            match app.page {
                Page::Setup => {
                    app.setup_log_scroll = app.setup_log_scroll.saturating_sub(1);
                }
                Page::Bridge => {
                    app.bridge_log_scroll = app.bridge_log_scroll.saturating_sub(1);
                }
                _ => {}
            }
        }
        MouseEventKind::ScrollDown => {
            match app.page {
                Page::Setup => {
                    app.setup_log_scroll = app.setup_log_scroll.saturating_add(1);
                }
                Page::Bridge => {
                    app.bridge_log_scroll = app.bridge_log_scroll.saturating_add(1);
                }
                _ => {}
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            let row = mouse.row;
            let col = mouse.column;

            // Tab bar clicks (top 3 rows, accounting for margin)
            if row <= 3 {
                if let Some(page) = status_bar::handle_tab_click(app, col) {
                    app.page = page;
                }
                return;
            }

            // Page-specific click handling
            match app.page {
                Page::Welcome => {
                    // Click anywhere on welcome → same as Enter
                }
                Page::Setup => {
                    // Form with section headers — layout:
                    // row+0: "── Connection ──" header
                    // row+1..6: 6 connection fields
                    // row+7: spacer
                    // row+8: "── Transport ──" header
                    // row+9: Transport field
                    // row+10: spacer (if redis)
                    // row+11: "── Redis ──" header (if redis)
                    // row+12..14: Redis fields (if redis)
                    let form_start_row = 8_u16; // after status bar + margin + title + border
                    let mut field_rows: Vec<(u16, SetupField)> = Vec::new();
                    let mut r = form_start_row;
                    r += 1; // Connection header
                    for field in [
                        SetupField::RemoteHost, SetupField::Connection,
                        SetupField::SshUser, SetupField::SshPort,
                        SetupField::KeyPath, SetupField::RemoteDir,
                    ] {
                        field_rows.push((r, field));
                        r += 1;
                    }
                    r += 1; // spacer
                    r += 1; // Transport header
                    field_rows.push((r, SetupField::Transport));
                    r += 1;
                    if app.show_redis_config() {
                        r += 1; // spacer
                        r += 1; // Redis header
                        for field in [
                            SetupField::RedisHost, SetupField::RedisPort, SetupField::RedisPassword,
                        ] {
                            field_rows.push((r, field));
                            r += 1;
                        }
                    }

                    for (field_row, field) in &field_rows {
                        if row == *field_row {
                            app.setup_field = *field;
                            break;
                        }
                    }
                }
                Page::Bridge => {
                    // Click on transport selector (row ~4-6, the header area)
                    // Transport labels are in the header at row ~4
                    if row >= 4 && row <= 6 {
                        // Rough column ranges for transport labels
                        // "[1]rsync" starts around col 14, "[2]MCP" around col 26, "[3]Redis" around col 36
                        if col >= 10 && col < 24 {
                            app.set_transport(TransportKind::Rsync);
                        } else if col >= 24 && col < 34 {
                            app.set_transport(TransportKind::Mcp);
                        } else if col >= 34 && col < 46 {
                            app.set_transport(TransportKind::Redis);
                        }
                    }

                    // Click on messages in outbox/inbox (row >= 7)
                    // Messages start after header, each is 1 row
                    // This is approximate — exact positioning depends on layout
                    if row >= 7 && !app.messages.is_empty() {
                        let msg_row = (row - 7) as usize;
                        if msg_row < app.messages.len() {
                            app.selected_message = Some(msg_row);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Actions that input handling can request
enum InputAction {
    None,
    StartBridge,
    SwitchTransport(TransportKind),
    SendCommand(String),
    LaunchSlave,
    PushInstall,
    Activate,
}

fn process_bridge_event(app: &mut App, event: BridgeEvent) {
    match event {
        BridgeEvent::MessageSent(entry) => {
            app.messages.push(entry);
        }
        BridgeEvent::MessageReceived(entry) => {
            app.messages.push(entry);
        }
        BridgeEvent::CommandReceived(_msg) => {}
        BridgeEvent::HealthUpdate(kind, healthy) => {
            let idx = match kind {
                TransportKind::Rsync => 0,
                TransportKind::Mcp => 1,
                TransportKind::Redis => 2,
            };
            app.transport_health[idx] = healthy;
        }
        BridgeEvent::ConnectionStatus(status) => {
            if status.starts_with("Connected") {
                app.connection_status = status;
            } else {
                app.connection_status = status;
                if app.session_status.starts_with("Active") {
                    app.session_status = "Inactive".to_string();
                }
            }
        }
        BridgeEvent::StatusUpdate(msg_id, status) => {
            for msg in &mut app.messages {
                if msg.msg_id == msg_id {
                    msg.status = status;
                }
            }
        }
        BridgeEvent::Log(msg) => {
            app.push_bridge_log(msg);
        }
        BridgeEvent::TransportSwitched(kind) => {
            app.active_transport = kind;
        }
        BridgeEvent::RoleConfirmed(role) => {
            app.push_bridge_log(format!("Role confirmed: {:?}", role));
            app.connection_status = format!("Connected ({:?})", role);
            app.session_status = format!("Active ({:?})", role);
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
    // On Bridge page, typing goes directly to the input bar
    if app.page == Page::Bridge && !modifiers.contains(KeyModifiers::CONTROL) {
        match key {
            KeyCode::Enter => {
                if !app.compose_input.is_empty() {
                    let task = app.compose_input.clone();
                    app.compose_input.clear();
                    return InputAction::SendCommand(task);
                }
                return InputAction::None;
            }
            KeyCode::Char(c) => {
                // Only capture if not a transport switch key or if input has content
                if !app.compose_input.is_empty() || !matches!(c, '1' | '2' | '3') {
                    app.compose_input.push(c);
                    return InputAction::None;
                }
                // Fall through for 1/2/3 transport switch when input is empty
            }
            KeyCode::Backspace => {
                app.compose_input.pop();
                return InputAction::None;
            }
            KeyCode::Esc => {
                if !app.compose_input.is_empty() {
                    app.compose_input.clear();
                    return InputAction::None;
                }
                // Fall through to page navigation
            }
            _ => {} // Fall through for Up/Down/etc
        }
    }

    // Global Ctrl keybindings
    if modifiers.contains(KeyModifiers::CONTROL) {
        match key {
            KeyCode::Char('q') => {
                app.running = false;
                return InputAction::None;
            }
            KeyCode::Char('s') => {
                if app.page == Page::Setup {
                    match app.settings.save() {
                        Ok(()) => app.push_setup_log("Config saved.".to_string()),
                        Err(e) => app.push_setup_log(format!("Save failed: {e}")),
                    }
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
    // Handle Ctrl combinations first (before Enter/char matching)
    if modifiers.contains(KeyModifiers::CONTROL) {
        match key {
            KeyCode::Char('d') => {
                // Ctrl+D = save, setup dirs, proceed to bridge (works from ANY field)
                app.push_setup_log("Saving configuration...".to_string());
                match app.settings.save() {
                    Ok(()) => app.push_setup_log("Config saved.".to_string()),
                    Err(e) => {
                        app.push_setup_log(format!("Save failed: {e}"));
                        return InputAction::None;
                    }
                }

                app.push_setup_log("Creating local directories...".to_string());
                match connection::setup_local_dirs(&app.settings) {
                    Ok(msg) => app.push_setup_log(msg),
                    Err(e) => {
                        app.push_setup_log(format!("Local setup failed: {e}"));
                        return InputAction::None;
                    }
                }

                app.push_setup_log("Creating remote directories...".to_string());
                match connection::setup_remote_dirs(&app.settings).await {
                    Ok(_msg) => {
                        app.push_setup_log("Remote dirs OK.".to_string());
                    }
                    Err(e) => {
                        app.push_setup_log(format!("Remote setup failed: {e}"));
                        app.push_setup_log("Proceeding anyway (set up remote manually).".to_string());
                    }
                }

                app.active_transport = app.settings.active_transport;
                app.next_page();
                return InputAction::StartBridge;
            }
            KeyCode::Char('t') => {
                // Ctrl+T = Test connection
                app.ssh_test_running = true;
                app.push_setup_log(format!(
                    "Testing {} connection to {}...",
                    app.settings.connection.label(),
                    app.settings.remote_host
                ));

                let result = connection::test_connection(&app.settings).await;
                app.ssh_test_running = false;

                if result.success {
                    app.ssh_test_passed = true;
                    app.connection_status = "Connected".to_string();
                    app.push_setup_log(format!("OK: {}", result.output));
                } else {
                    app.push_setup_log(format!("FAIL: {}", result.output));
                }
                return InputAction::None;
            }
            KeyCode::Char('g') => {
                // Ctrl+G = Generate SSH key
                app.push_setup_log("Generating SSH key...".to_string());
                let key_path = crate::config::Settings::expand_path(&app.settings.key_path);
                if std::path::Path::new(&key_path).exists() {
                    app.push_setup_log(format!("Key already exists at {key_path}"));
                } else {
                    match tokio::process::Command::new("ssh-keygen")
                        .args(["-t", "ed25519", "-f", &key_path, "-N", "", "-C", "interclaude"])
                        .output()
                        .await
                    {
                        Ok(output) => {
                            if output.status.success() {
                                app.push_setup_log(format!("Key generated at {key_path}"));
                                app.push_setup_log("Copy public key to remote: ssh-copy-id".to_string());
                            } else {
                                let err = String::from_utf8_lossy(&output.stderr);
                                app.push_setup_log(format!("Key gen failed: {err}"));
                            }
                        }
                        Err(e) => app.push_setup_log(format!("ssh-keygen error: {e}")),
                    }
                }
                return InputAction::None;
            }
            KeyCode::Char('p') => {
                // Ctrl+P = Push install to remote
                return InputAction::PushInstall;
            }
            KeyCode::Char('a') => {
                // Ctrl+A = Activate (full auto-sequence)
                if app.has_remote_config() {
                    return InputAction::Activate;
                } else {
                    app.push_setup_log("Fill Remote Host and User first.".to_string());
                    return InputAction::None;
                }
            }
            _ => {}
        }
    }

    match key {
        KeyCode::Tab | KeyCode::Down => {
            let show_redis = app.show_redis_config();
            app.setup_field = app.setup_field.next_visible(show_redis);
        }
        KeyCode::BackTab | KeyCode::Up => {
            let show_redis = app.show_redis_config();
            app.setup_field = app.setup_field.prev_visible(show_redis);
        }
        KeyCode::Enter => {
            // Plain Enter — cycle selector fields
            if app.setup_field == SetupField::Connection {
                app.settings.connection = app.settings.connection.cycle();
            } else if app.setup_field == SetupField::Transport {
                app.settings.active_transport = match app.settings.active_transport {
                    TransportKind::Rsync => TransportKind::Mcp,
                    TransportKind::Mcp => TransportKind::Redis,
                    TransportKind::Redis => TransportKind::Rsync,
                };
                // Snap focus away from Redis fields if they're now hidden
                if app.setup_field.is_redis_field() && !app.show_redis_config() {
                    app.setup_field = SetupField::Transport;
                }
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
        _ => {}
    }

    InputAction::None
}

fn handle_bridge_input(app: &mut App, key: KeyCode, modifiers: KeyModifiers) -> InputAction {
    if modifiers.contains(KeyModifiers::CONTROL) {
        match key {
            KeyCode::Char('l') => {
                return InputAction::LaunchSlave;
            }
            _ => {}
        }
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
    // (name, check_cmd, install_hint, required)
    let deps: Vec<(&str, &str, &str, bool)> = vec![
        ("ssh",       "ssh -V",              "(built-in)",                        true),
        ("rsync",     "rsync --version",     "brew install rsync",                true),
        ("claude",    "claude --version",    "npm i -g @anthropic-ai/claude-code", true),
        ("mosh",      "mosh --version",      "brew install mosh",                 false),
        ("autossh",   "autossh -V",          "brew install autossh",              false),
        ("fswatch",   "fswatch --version",   "brew install fswatch",              false),
        ("redis-cli", "redis-cli --version", "brew install redis",                false),
    ];

    app.dep_checks.clear();
    app.dep_check_complete = false;

    for (name, cmd, hint, required) in deps {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let result = tokio::process::Command::new(parts[0])
            .args(&parts[1..])
            .output()
            .await;

        let (available, version) = match result {
            Ok(output) => {
                let ver = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .find(|l| !l.trim().is_empty()
                        && !l.starts_with("Usage:")
                        && !l.starts_with("Warning:"))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let ver = if ver.is_empty() {
                    String::from_utf8_lossy(&output.stderr)
                        .lines()
                        .find(|l| !l.trim().is_empty()
                            && !l.starts_with("Usage:")
                            && !l.starts_with("Warning:"))
                        .unwrap_or("")
                        .trim()
                        .to_string()
                } else {
                    ver
                };
                // Truncate noisy version strings
                let ver = if ver.len() > 40 {
                    format!("{}...", &ver[..37])
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
            install_hint: hint.to_string(),
            required,
        });
    }

    app.dep_check_complete = true;
}
