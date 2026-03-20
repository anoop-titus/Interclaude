mod app;
mod bridge;
mod config;
mod transport;
mod tui;

use anyhow::Result;
use clap::Parser;

use crate::bridge::engine::BridgeEngine;
use crate::config::Role;
use crate::transport::TransportKind;

#[derive(Parser, Debug)]
#[command(name = "interclaude", version, about = "Cross-machine Claude Code bridge")]
struct Cli {
    /// Path to config file
    #[arg(short, long)]
    config: Option<String>,

    /// Run in slave/watcher mode (headless, no TUI)
    #[arg(long)]
    slave: bool,

    /// Specify role explicitly
    #[arg(long, value_parser = ["master", "slave"])]
    role: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.slave {
        run_slave().await?;
        return Ok(());
    }

    let mut app = app::App::new()?;
    tui::run(&mut app).await?;

    Ok(())
}

/// Run in headless slave mode — monitors inbox and processes commands
async fn run_slave() -> Result<()> {
    eprintln!("Interclaude slave mode starting...");

    let mut settings = config::Settings::load();
    settings.role = Role::Slave;

    let base = settings.local_interclaude_dir();
    eprintln!("Interclaude dir: {}", base.display());

    // Ensure directories exist
    bridge::connection::setup_local_dirs(&settings)?;

    // Create bridge engine with a channel for events (we'll just log them)
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(256);
    let engine = BridgeEngine::new(settings.clone(), event_tx)?;
    let engine = std::sync::Arc::new(engine);

    // Start background tasks
    engine.start_health_monitor();
    engine.start_receive_loop();
    engine.start_heartbeat();

    eprintln!("Slave watcher running. Press Ctrl+C to stop.");

    // Log events from the engine
    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match event {
                    BridgeEvent::MessageReceived(entry) => {
                        eprintln!("[RECV] {} - {}", entry.timestamp, entry.content_preview);
                    }
                    BridgeEvent::MessageSent(entry) => {
                        eprintln!("[SENT] {} - {}", entry.timestamp, entry.content_preview);
                    }
                    BridgeEvent::HealthUpdate(kind, healthy) => {
                        eprintln!("[HEALTH] {}: {}", kind.label(), if healthy { "UP" } else { "DOWN" });
                    }
                    BridgeEvent::ConnectionStatus(status) => {
                        eprintln!("[STATUS] {}", status);
                    }
                    BridgeEvent::Log(msg) => {
                        eprintln!("[LOG] {}", msg);
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nShutting down slave...");
                break;
            }
        }
    }

    Ok(())
}

use bridge::engine::BridgeEvent;
