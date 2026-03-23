mod api;
mod app;
mod bridge;
mod config;
pub mod error;
pub mod logging;
mod transport;
mod tui;

use anyhow::Result;
use clap::Parser;

use crate::bridge::engine::BridgeEngine;
use crate::bridge::engine::BridgeEvent;
use crate::bridge::message::{MessagePayload, MessageType};
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
    logging::init();

    let cli = Cli::parse();

    if cli.slave {
        run_slave().await?;
        return Ok(());
    }

    let mut app = app::App::new()?;
    tui::run(&mut app).await?;

    Ok(())
}

/// Run in headless slave mode — monitors inbox and processes commands via claude -p
async fn run_slave() -> Result<()> {
    logging::log("Interclaude slave mode starting...");

    let mut settings = config::Settings::load();
    settings.role = Role::Slave;

    let base = settings.local_interclaude_dir();
    logging::log(&format!("Interclaude dir: {}", base.display()));

    // Ensure directories exist
    bridge::connection::setup_local_dirs(&settings)?;

    // Create bridge engine with a channel for events
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(256);
    let engine = BridgeEngine::new(settings.clone(), event_tx)?;
    let engine = std::sync::Arc::new(engine);

    // Start autossh tunnels (Redis + MCP) — on slave, these may fail (ports already in use locally) which is fine
    let _tunnel_handles = engine.start_tunnels().await;

    // Start background tasks
    engine.start_health_monitor();
    engine.start_receive_loop();
    engine.start_heartbeat();

    // Start Redis subscriber if active
    engine.start_redis_subscriber_if_active();

    // Start MCP server so master can connect via SSH tunnel
    let mcp_server = transport::mcp_transport::McpServer::new(
        settings.mcp_port,
        &base,
        Role::Slave,
    );
    tokio::spawn(async move {
        logging::log(&format!("MCP server listening on 0.0.0.0:{}", settings.mcp_port));
        if let Err(e) = mcp_server.run().await {
            logging::log(&format!("[ERR] MCP server failed: {e}"));
        }
    });

    // Send handshake proposal
    if let Err(e) = engine.send_handshake().await {
        logging::log(&format!("[WARN] Handshake failed: {e}"));
    }

    logging::log("Slave watcher running. Ctrl+C to stop.");

    // Process events from the engine
    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match event {
                    BridgeEvent::CommandReceived(msg) => {
                        // Extract task from command payload and execute
                        if let MessagePayload::Command(ref cmd) = msg.payload {
                            let task = cmd.task.clone();
                            let msg_id = msg.msg_id.clone();
                            let engine = engine.clone();

                            logging::log(&format!("[EXEC] Processing command: {}", if task.len() > 60 {
                                format!("{}...", &task[..57])
                            } else {
                                task.clone()
                            }));

                            // Execute in background so we don't block the event loop
                            tokio::spawn(async move {
                                // Send READ status
                                let _ = engine.send_status_update(&msg_id, app::DeliveryStatus::Read).await;

                                // Send EXECUTING status
                                let _ = engine.send_status_update(&msg_id, app::DeliveryStatus::Executing).await;

                                // Execute via claude -p
                                let start = std::time::Instant::now();
                                let output = tokio::process::Command::new("claude")
                                    .args(["-p", &task])
                                    .output()
                                    .await;

                                let duration_ms = start.elapsed().as_millis() as u64;

                                match output {
                                    Ok(output) => {
                                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                                        let exit_code = output.status.code().unwrap_or(-1);

                                        logging::log(&format!("[EXEC] Command completed (exit={}, {}ms)", exit_code, duration_ms));

                                        // Send EXECUTED status
                                        let _ = engine.send_status_update(&msg_id, app::DeliveryStatus::Executed).await;

                                        // Send REPLYING status
                                        let _ = engine.send_status_update(&msg_id, app::DeliveryStatus::Replying).await;

                                        // Send response back
                                        if let Err(e) = engine.send_response(
                                            &msg_id, stdout, stderr, exit_code, duration_ms
                                        ).await {
                                            logging::log(&format!("[ERR] Failed to send response: {e}"));
                                        }
                                    }
                                    Err(e) => {
                                        logging::log(&format!("[ERR] claude -p failed: {e}"));
                                        let _ = engine.send_response(
                                            &msg_id,
                                            String::new(),
                                            format!("Failed to execute claude: {e}"),
                                            -1,
                                            duration_ms,
                                        ).await;
                                    }
                                }
                            });
                        }
                    }
                    BridgeEvent::MessageReceived(entry) => {
                        logging::log(&format!("[RECV] {} - {}", entry.timestamp, entry.content_preview));
                    }
                    BridgeEvent::MessageSent(entry) => {
                        logging::log(&format!("[SENT] {} - {}", entry.timestamp, entry.content_preview));
                    }
                    BridgeEvent::HealthUpdate(kind, healthy) => {
                        logging::log(&format!("[HEALTH] {}: {}", kind.label(), if healthy { "UP" } else { "DOWN" }));
                    }
                    BridgeEvent::ConnectionStatus(status) => {
                        logging::log(&format!("[STATUS] {}", status));
                    }
                    BridgeEvent::RoleConfirmed(role) => {
                        logging::log(&format!("[ROLE] Confirmed as {:?}", role));
                    }
                    BridgeEvent::Log(msg) => {
                        logging::log(&format!("[LOG] {}", msg));
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                logging::log(&format!("\nShutting down slave..."));
                break;
            }
        }
    }

    Ok(())
}
