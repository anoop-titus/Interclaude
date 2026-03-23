use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::app::{DeliveryStatus, MessageDirection, MessageEntry};
use crate::bridge::handshake::{Handshake, HandshakeResponse};
use crate::bridge::message::{
    CommandPayload, Message, MessagePayload, MessageType, PingPayload, ResponsePayload, StatusPayload,
};
use crate::config::{Role, Settings};
use crate::transport::dedup::DedupLedger;
use crate::transport::mcp_transport::McpTransport;
use crate::transport::redis_transport::RedisTransport;
use crate::transport::rsync_transport::RsyncTransport;
use crate::transport::status::StatusTracker;
use crate::transport::{Transport, TransportKind, TransportSelector};

/// Events from the bridge engine to the TUI
#[derive(Debug, Clone)]
pub enum BridgeEvent {
    /// A new message was sent
    MessageSent(MessageEntry),
    /// A new message was received
    MessageReceived(MessageEntry),
    /// A command was received (slave needs full message to execute)
    CommandReceived(Message),
    /// Transport health updated
    HealthUpdate(TransportKind, bool),
    /// Connection status changed
    ConnectionStatus(String),
    /// Status update for a specific message
    StatusUpdate(String, DeliveryStatus),
    /// Streaming chunk: (msg_id, accumulated_text_so_far)
    StreamChunk(String, String),
    /// Stream complete: (msg_id, final_full_text)
    StreamComplete(String, String),
    /// Ping round-trip time in ms
    PingResult(u64),
    /// Log message
    Log(String),
    /// Transport switched
    TransportSwitched(TransportKind),
    /// Role confirmed via handshake
    RoleConfirmed(Role),
}

/// Persistent transport instances so connections aren't recreated every loop
struct Transports {
    rsync: RsyncTransport,
    mcp: McpTransport,
    redis: RedisTransport,
}

/// The bridge engine manages transports, message flow, and session lifecycle
pub struct BridgeEngine {
    settings: Settings,
    pub event_tx: mpsc::Sender<BridgeEvent>,
    selector: Arc<Mutex<TransportSelector>>,
    ledger: Arc<Mutex<DedupLedger>>,
    status_tracker: Arc<StatusTracker>,
    sequence: Arc<Mutex<u64>>,
    session_id: String,
    transports: Arc<Transports>,
    handshake: Arc<Mutex<Handshake>>,
    pub role: Arc<Mutex<Role>>,
}

impl BridgeEngine {
    pub fn new(settings: Settings, event_tx: mpsc::Sender<BridgeEvent>) -> Result<Self> {
        let base = settings.local_interclaude_dir();
        let ledger = DedupLedger::new(&base)?;
        let status_tracker = StatusTracker::new(&base)?;
        let selector = TransportSelector::new(settings.active_transport);
        let session_id = uuid::Uuid::now_v7().to_string();

        let transports = Transports {
            rsync: RsyncTransport::new(&settings),
            mcp: McpTransport::new(&settings),
            redis: RedisTransport::new(&settings, &session_id),
        };

        let role = settings.role;
        let handshake = Handshake::new(&session_id);

        Ok(Self {
            settings,
            event_tx,
            selector: Arc::new(Mutex::new(selector)),
            ledger: Arc::new(Mutex::new(ledger)),
            status_tracker: Arc::new(status_tracker),
            sequence: Arc::new(Mutex::new(0)),
            session_id,
            transports: Arc::new(transports),
            handshake: Arc::new(Mutex::new(handshake)),
            role: Arc::new(Mutex::new(role)),
        })
    }

    /// Get the next sequence number
    async fn next_seq(&self) -> u64 {
        let mut seq = self.sequence.lock().await;
        *seq += 1;
        *seq
    }

    /// Get current active transport kind
    pub async fn active_transport(&self) -> TransportKind {
        self.selector.lock().await.active
    }

    /// Get current role
    pub async fn current_role(&self) -> Role {
        *self.role.lock().await
    }

    /// Send a command message using the active transport
    pub async fn send_command(&self, task: String) -> Result<String> {
        let seq = self.next_seq().await;
        let role = match *self.role.lock().await {
            Role::Master => "master",
            Role::Slave => "slave",
        };
        let active = self.active_transport().await;
        let msg = Message::new_command(seq, task.clone(), role, active.label());
        let msg_id = msg.msg_id.clone();

        // Send via active transport
        self.send_via_active(&msg).await?;

        // Track delivery status
        self.status_tracker.update(&msg_id, DeliveryStatus::Delivered)?;

        // Emit event to TUI
        let entry = MessageEntry {
            msg_id: msg_id.clone(),
            timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            direction: MessageDirection::Outbound,
            content_preview: msg.preview(),
            status: DeliveryStatus::Delivered,
        };
        let _ = self.event_tx.send(BridgeEvent::MessageSent(entry)).await;

        Ok(msg_id)
    }

    /// Send a response message back (used by slave after executing a command)
    pub async fn send_response(
        &self,
        reply_to: &str,
        stdout: String,
        stderr: String,
        exit_code: i32,
        duration_ms: u64,
    ) -> Result<()> {
        let seq = self.next_seq().await;
        let role = match *self.role.lock().await {
            Role::Master => "master",
            Role::Slave => "slave",
        };
        let active = self.active_transport().await;

        let msg = Message {
            msg_id: uuid::Uuid::now_v7().to_string(),
            msg_type: MessageType::Response,
            timestamp: chrono::Utc::now(),
            sequence: seq,
            sender_role: role.to_string(),
            transport_used: active.label().to_string(),
            payload: MessagePayload::Response(ResponsePayload {
                reply_to: reply_to.to_string(),
                stdout,
                stderr,
                exit_code,
                files_modified: vec![],
                duration_ms,
            }),
        };

        self.send_via_active(&msg).await?;

        let entry = MessageEntry {
            msg_id: msg.msg_id.clone(),
            timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            direction: MessageDirection::Outbound,
            content_preview: msg.preview(),
            status: DeliveryStatus::Replying,
        };
        let _ = self.event_tx.send(BridgeEvent::MessageSent(entry)).await;

        Ok(())
    }

    /// Send a status update message for a given msg_id
    pub async fn send_status_update(&self, ref_msg_id: &str, status: DeliveryStatus) -> Result<()> {
        let seq = self.next_seq().await;
        let role = match *self.role.lock().await {
            Role::Master => "master",
            Role::Slave => "slave",
        };
        let active = self.active_transport().await;

        let msg = Message {
            msg_id: uuid::Uuid::now_v7().to_string(),
            msg_type: MessageType::Status,
            timestamp: chrono::Utc::now(),
            sequence: seq,
            sender_role: role.to_string(),
            transport_used: active.label().to_string(),
            payload: MessagePayload::Status(StatusPayload {
                ref_msg_id: ref_msg_id.to_string(),
                status: status.label().to_string(),
            }),
        };

        // Best-effort — don't fail the whole operation if status send fails
        let _ = self.send_via_active(&msg).await;

        Ok(())
    }

    /// Send a ping message to measure RTT
    pub async fn send_ping(&self) -> Result<()> {
        let seq = self.next_seq().await;
        let role = match *self.role.lock().await {
            Role::Master => "master",
            Role::Slave => "slave",
        };
        let active = self.active_transport().await;
        let ping = Message::new_ping(seq, role, active.label());
        self.send_via_active(&ping).await?;
        Ok(())
    }

    /// Send the handshake proposal on startup
    pub async fn send_handshake(&self) -> Result<()> {
        let desired_role = *self.role.lock().await;
        let proposal = {
            let mut hs = self.handshake.lock().await;
            hs.create_proposal(desired_role)
        };

        let _ = self.event_tx.send(BridgeEvent::Log(
            format!("Sending handshake proposal as {:?}", desired_role)
        )).await;

        self.send_via_active(&proposal).await?;
        Ok(())
    }

    /// Send a message via the currently active transport
    /// Rsync always runs as the file-sync backbone (the slave watcher is file-based).
    /// Redis/MCP are notification overlays on top.
    async fn send_via_active(&self, msg: &Message) -> Result<()> {
        let active = self.active_transport().await;

        // Always write to outbox + attempt rsync push (non-fatal — slave may SSH-to-self which fails)
        if let Err(e) = self.transports.rsync.send(msg).await {
            crate::logging::log(&format!("rsync send failed (non-fatal): {e}"));
        }

        // Additionally send via the overlay transport for faster delivery
        match active {
            TransportKind::Rsync => {} // already attempted above
            TransportKind::Mcp => { let _ = self.transports.mcp.send(msg).await; }
            TransportKind::Redis => { let _ = self.transports.redis.send(msg).await; }
        }

        Ok(())
    }

    /// Start the Redis subscriber if Redis is the active transport
    pub fn start_redis_subscriber_if_active(&self) {
        let transports = self.transports.clone();
        let selector = self.selector.clone();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            let active = selector.lock().await.active;
            if active == TransportKind::Redis {
                let _ = event_tx.send(BridgeEvent::Log(
                    "Starting Redis subscriber...".to_string()
                )).await;
                transports.redis.start_subscriber();
                let _ = event_tx.send(BridgeEvent::Log(
                    "Redis subscriber started".to_string()
                )).await;
            }
        });
    }

    /// Switch to a different transport
    pub async fn switch_transport(&self, new_kind: TransportKind) -> Result<()> {
        let current = self.active_transport().await;
        if current == new_kind {
            return Ok(());
        }

        let _ = self.event_tx.send(BridgeEvent::Log(
            format!("Switching transport: {} -> {}", current.label(), new_kind.label())
        )).await;

        // Send transport_switch announcement on current transport
        let seq = self.next_seq().await;
        let role = match *self.role.lock().await {
            Role::Master => "master",
            Role::Slave => "slave",
        };
        let switch_msg = Message {
            msg_id: uuid::Uuid::now_v7().to_string(),
            msg_type: MessageType::TransportSwitch,
            timestamp: chrono::Utc::now(),
            sequence: seq,
            sender_role: role.to_string(),
            transport_used: current.label().to_string(),
            payload: MessagePayload::TransportSwitch(
                crate::bridge::message::TransportSwitchPayload {
                    from: current.label().to_string(),
                    to: new_kind.label().to_string(),
                },
            ),
        };

        // Best-effort send on current transport
        let _ = self.send_via_active(&switch_msg).await;

        // Activate new transport
        {
            let mut selector = self.selector.lock().await;
            selector.set_active(new_kind);
        }

        // Start the tunnel required by the new transport (rsync needs none)
        let _tunnel_handles = self.start_tunnel_for(new_kind).await;
        // Note: tunnel handles are dropped here — autossh runs independently.
        // In production, these should be tracked for cleanup.

        // If switching TO Redis, start the subscriber
        if new_kind == TransportKind::Redis {
            let _ = self.event_tx.send(BridgeEvent::Log(
                "Starting Redis subscriber for new transport...".to_string()
            )).await;
            self.transports.redis.start_subscriber();
        }

        // Health check on new transport with 10s timeout
        let health_ok = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.check_health(new_kind),
        )
        .await
        .unwrap_or(false);

        if !health_ok {
            // Rollback
            let _ = self.event_tx.send(BridgeEvent::Log(
                format!("Transport {} failed health check, rolling back to {}", new_kind.label(), current.label())
            )).await;
            let mut selector = self.selector.lock().await;
            selector.set_active(current);
            anyhow::bail!("New transport failed health check, rolled back to {}", current.label());
        }

        let _ = self.event_tx.send(BridgeEvent::TransportSwitched(new_kind)).await;
        let _ = self.event_tx.send(BridgeEvent::Log(
            format!("Transport switched to {}", new_kind.label())
        )).await;

        Ok(())
    }

    /// Health check a specific transport
    async fn check_health(&self, kind: TransportKind) -> bool {
        let result = match kind {
            TransportKind::Rsync => self.transports.rsync.health_check().await,
            TransportKind::Mcp => self.transports.mcp.health_check().await,
            TransportKind::Redis => self.transports.redis.health_check().await,
        };

        let healthy = result.unwrap_or(false);
        let mut selector = self.selector.lock().await;
        selector.update_health(kind, healthy);
        let _ = self.event_tx.send(BridgeEvent::HealthUpdate(kind, healthy)).await;
        healthy
    }

    /// Run periodic health checks — only active transport affects connection status
    pub fn start_health_monitor(&self) -> JoinHandle<()> {
        let transports = self.transports.clone();
        let selector = self.selector.clone();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            loop {
                let active = selector.lock().await.active;

                for kind in [TransportKind::Rsync, TransportKind::Mcp, TransportKind::Redis] {
                    // Only health-check transports that have a chance of working:
                    // - Always check the active transport
                    // - Skip inactive Redis/MCP (no tunnel running for them)
                    if kind != active && kind != TransportKind::Rsync {
                        continue;
                    }

                    let healthy = match kind {
                        TransportKind::Rsync => transports.rsync.health_check().await.unwrap_or(false),
                        TransportKind::Mcp => transports.mcp.health_check().await.unwrap_or(false),
                        TransportKind::Redis => transports.redis.health_check().await.unwrap_or(false),
                    };

                    selector.lock().await.update_health(kind, healthy);
                    let _ = event_tx.send(BridgeEvent::HealthUpdate(kind, healthy)).await;

                    // Only the active transport drives connection status
                    if kind == active {
                        if healthy {
                            let _ = event_tx.send(BridgeEvent::ConnectionStatus(
                                "Connected".to_string()
                            )).await;
                        } else {
                            let _ = event_tx.send(BridgeEvent::ConnectionStatus(
                                format!("{} health check failed", kind.label())
                            )).await;
                        }
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            }
        })
    }

    /// Start polling for incoming messages on the active transport
    pub fn start_receive_loop(&self) -> JoinHandle<()> {
        let transports = self.transports.clone();
        let selector = self.selector.clone();
        let ledger = self.ledger.clone();
        let status_tracker = self.status_tracker.clone();
        let event_tx = self.event_tx.clone();
        let sync_interval = self.settings.sync_interval_secs;
        let handshake = self.handshake.clone();
        let role = self.role.clone();
        let sequence = self.sequence.clone();

        tokio::spawn(async move {
            loop {
                let active = selector.lock().await.active;

                // Always rsync pull (slave watcher writes responses to files)
                let mut messages = transports.rsync.receive().await.unwrap_or_default();

                // Also drain overlay transport buffer for faster pickup
                match active {
                    TransportKind::Rsync => {} // already pulled above
                    TransportKind::Mcp => {
                        messages.extend(transports.mcp.receive().await.unwrap_or_default());
                    }
                    TransportKind::Redis => {
                        messages.extend(transports.redis.receive().await.unwrap_or_default());
                    }
                }

                for msg in messages {
                    // Dedup check
                    {
                        let mut ledger = ledger.lock().await;
                        if ledger.is_seen(&msg.msg_id) {
                            continue;
                        }
                        let _ = ledger.mark_seen(&msg.msg_id);
                    }

                    // Process based on message type
                    match msg.msg_type {
                        MessageType::Command => {
                            // First check if this is a handshake message
                            let hs_response = {
                                let mut hs = handshake.lock().await;
                                hs.process_handshake(&msg)
                            };

                            match hs_response {
                                Ok(HandshakeResponse::Accepted(_negotiated_role)) => {
                                    // Keep the configured role — don't let handshake override it
                                    let configured_role = *role.lock().await;
                                    let _ = event_tx.send(BridgeEvent::RoleConfirmed(configured_role)).await;
                                    let _ = event_tx.send(BridgeEvent::Log(
                                        format!("Handshake: confirmed as {:?} (configured)", configured_role)
                                    )).await;
                                }
                                Ok(HandshakeResponse::AlreadyConfirmed(_r)) => {
                                    let configured_role = *role.lock().await;
                                    let _ = event_tx.send(BridgeEvent::Log(
                                        format!("Handshake: already confirmed as {:?}", configured_role)
                                    )).await;
                                }
                                Ok(HandshakeResponse::Error(e)) => {
                                    let _ = event_tx.send(BridgeEvent::Log(
                                        format!("Handshake error: {}", e)
                                    )).await;
                                }
                                Ok(HandshakeResponse::NotHandshake) => {
                                    // Regular command — emit for slave processing
                                    let current_role = *role.lock().await;
                                    if current_role == Role::Slave {
                                        // Slave receives a command to execute
                                        let _ = event_tx.send(BridgeEvent::CommandReceived(msg.clone())).await;
                                    }

                                    let entry = MessageEntry {
                                        msg_id: msg.msg_id.clone(),
                                        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                                        direction: MessageDirection::Inbound,
                                        content_preview: msg.preview(),
                                        status: DeliveryStatus::Read,
                                    };
                                    let _ = event_tx.send(BridgeEvent::MessageReceived(entry)).await;
                                }
                                Err(e) => {
                                    let _ = event_tx.send(BridgeEvent::Log(
                                        format!("Handshake parse error: {}", e)
                                    )).await;
                                }
                            }
                        }
                        MessageType::Response => {
                            // Update status of the original command
                            if let MessagePayload::Response(ref resp) = msg.payload {
                                let _ = status_tracker.update(&resp.reply_to, DeliveryStatus::ReceivedReply);
                                let _ = event_tx.send(BridgeEvent::StatusUpdate(
                                    resp.reply_to.clone(),
                                    DeliveryStatus::ReceivedReply,
                                )).await;
                            }

                            let entry = MessageEntry {
                                msg_id: msg.msg_id.clone(),
                                timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                                direction: MessageDirection::Inbound,
                                content_preview: msg.preview(),
                                status: DeliveryStatus::ReceivedReply,
                            };
                            let _ = event_tx.send(BridgeEvent::MessageReceived(entry)).await;
                        }
                        MessageType::Status => {
                            if let MessagePayload::Status(ref st) = msg.payload {
                                let status = match st.status.as_str() {
                                    "READ" => DeliveryStatus::Read,
                                    "EXECUTING" => DeliveryStatus::Executing,
                                    "EXECUTED" => DeliveryStatus::Executed,
                                    "REPLYING" => DeliveryStatus::Replying,
                                    "RECEIVING" => DeliveryStatus::ReceivingReply,
                                    _ => DeliveryStatus::Delivered,
                                };
                                let _ = status_tracker.update(&st.ref_msg_id, status);
                                let _ = event_tx.send(BridgeEvent::StatusUpdate(
                                    st.ref_msg_id.clone(),
                                    status,
                                )).await;
                            }
                        }
                        MessageType::TransportSwitch => {
                            // Remote side is switching transports — follow suit
                            if let MessagePayload::TransportSwitch(ref sw) = msg.payload {
                                let new_kind = match sw.to.as_str() {
                                    "rsync" => Some(TransportKind::Rsync),
                                    "MCP" => Some(TransportKind::Mcp),
                                    "Redis" => Some(TransportKind::Redis),
                                    _ => None,
                                };
                                if let Some(kind) = new_kind {
                                    let mut sel = selector.lock().await;
                                    sel.set_active(kind);
                                    let _ = event_tx.send(BridgeEvent::TransportSwitched(kind)).await;
                                    let _ = event_tx.send(BridgeEvent::Log(
                                        format!("Followed remote transport switch to {}", kind.label())
                                    )).await;

                                    // Start Redis subscriber if switching to Redis
                                    if kind == TransportKind::Redis {
                                        transports.redis.start_subscriber();
                                    }
                                }
                            } else {
                                let _ = event_tx.send(BridgeEvent::Log(
                                    format!("Remote requested transport switch: {}", msg.preview())
                                )).await;
                            }
                        }
                        MessageType::Heartbeat => {
                            // Update connection status
                            let _ = event_tx.send(BridgeEvent::ConnectionStatus(
                                "Connected".to_string()
                            )).await;
                        }
                        MessageType::Ping => {
                            // Respond with Pong echoing the origin timestamp
                            if let MessagePayload::Ping(ref ping_data) = msg.payload {
                                let mut seq = sequence.lock().await;
                                *seq += 1;
                                let seq_val = *seq;
                                drop(seq);

                                let role_str = match *role.lock().await {
                                    Role::Master => "master",
                                    Role::Slave => "slave",
                                };
                                let active = selector.lock().await.active;
                                let pong = Message::new_pong(seq_val, role_str, active.label(), ping_data);

                                // Send pong via rsync (always available)
                                let _ = transports.rsync.send(&pong).await;
                                match active {
                                    TransportKind::Rsync => {}
                                    TransportKind::Mcp => { let _ = transports.mcp.send(&pong).await; }
                                    TransportKind::Redis => { let _ = transports.redis.send(&pong).await; }
                                }
                            }
                        }
                        MessageType::Pong => {
                            // Calculate RTT from origin timestamp
                            if let MessagePayload::Ping(ref ping_data) = msg.payload {
                                let now_ms = chrono::Utc::now().timestamp_millis();
                                let rtt = (now_ms - ping_data.origin_timestamp_ms).max(0) as u64;
                                let _ = event_tx.send(BridgeEvent::PingResult(rtt)).await;
                                let _ = event_tx.send(BridgeEvent::ConnectionStatus(
                                    "Connected".to_string()
                                )).await;
                            }
                        }
                        _ => {
                            let entry = MessageEntry {
                                msg_id: msg.msg_id.clone(),
                                timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                                direction: MessageDirection::Inbound,
                                content_preview: msg.preview(),
                                status: DeliveryStatus::ReceivedReply,
                            };
                            let _ = event_tx.send(BridgeEvent::MessageReceived(entry)).await;
                        }
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(sync_interval)).await;
            }
        })
    }

    /// Start periodic ping sender (replaces heartbeat — measures RTT + keeps alive)
    pub fn start_heartbeat(&self) -> JoinHandle<()> {
        let transports = self.transports.clone();
        let selector = self.selector.clone();
        let sequence = self.sequence.clone();
        let role = self.role.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;

                let active = selector.lock().await.active;
                let mut seq = sequence.lock().await;
                *seq += 1;
                let seq_val = *seq;
                drop(seq);

                let role_str = match *role.lock().await {
                    Role::Master => "master",
                    Role::Slave => "slave",
                };

                // Send ping instead of heartbeat — doubles as keepalive + RTT measurement
                let ping = Message::new_ping(seq_val, role_str, active.label());

                // Always write to rsync outbox
                let _ = transports.rsync.send(&ping).await;
                // Also send via overlay transport
                match active {
                    TransportKind::Rsync => {}
                    TransportKind::Mcp => { let _ = transports.mcp.send(&ping).await; }
                    TransportKind::Redis => { let _ = transports.redis.send(&ping).await; }
                }
            }
        })
    }

    /// Launch slave watcher on remote (Phase 8)
    pub async fn launch_slave(&self) -> Result<tokio::process::Child> {
        let _ = self.event_tx.send(BridgeEvent::ConnectionStatus(
            "Launching slave...".to_string()
        )).await;

        let child = crate::bridge::session::launch_slave_watcher(&self.settings).await?;

        let _ = self.event_tx.send(BridgeEvent::ConnectionStatus(
            "Connected".to_string()
        )).await;
        let _ = self.event_tx.send(BridgeEvent::Log(
            "Slave watcher launched on remote machine".to_string()
        )).await;

        Ok(child)
    }

    /// Execute a command on the remote machine via SSH + `claude -p` with streaming.
    /// Emits StreamChunk events as Claude generates output, then StreamComplete when done.
    pub async fn execute_remote_command(&self, task: String, msg_id: String) -> Result<()> {
        // Status: EXECUTING
        let _ = self.event_tx.send(BridgeEvent::StatusUpdate(
            msg_id.clone(), DeliveryStatus::Executing,
        )).await;
        let _ = self.event_tx.send(BridgeEvent::Log(
            format!("Executing on remote: {}",
                if task.len() > 50 { format!("{}...", &task[..47]) } else { task.clone() })
        )).await;

        let timeout_secs = self.settings.message_timeout_secs;
        let event_tx = self.event_tx.clone();
        let resp_msg_id = format!("{}-resp", msg_id);
        let settings = self.settings.clone();

        // Create channel for streaming chunks
        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<String>(64);

        // Spawn the streaming exec in a separate task
        let exec_handle = tokio::spawn(async move {
            crate::bridge::session::remote_claude_exec_streaming(&settings, &task, chunk_tx).await
        });

        // Forward chunks as BridgeEvents until the exec finishes
        let chunk_msg_id = resp_msg_id.clone();
        let chunk_event_tx = event_tx.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(accumulated) = chunk_rx.recv().await {
                let _ = chunk_event_tx.send(BridgeEvent::StreamChunk(
                    chunk_msg_id.clone(),
                    accumulated,
                )).await;
            }
        });

        // Wait for completion with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            exec_handle,
        ).await;

        // Ensure forwarder is done
        let _ = forwarder.await;

        match result {
            Ok(Ok(Ok(exec_result))) => {
                let _ = self.event_tx.send(BridgeEvent::StatusUpdate(
                    msg_id.clone(), DeliveryStatus::Executed,
                )).await;
                let _ = self.event_tx.send(BridgeEvent::Log(
                    format!("Command completed (exit={}, {:.1}s)",
                        exec_result.exit_code, exec_result.duration_ms as f64 / 1000.0)
                )).await;

                let response_text = if exec_result.exit_code == 0 {
                    exec_result.stdout.clone()
                } else {
                    format!("[exit {}] {}\n{}", exec_result.exit_code, exec_result.stderr, exec_result.stdout)
                };

                let _ = self.event_tx.send(BridgeEvent::StreamComplete(
                    resp_msg_id, response_text,
                )).await;
                let _ = self.event_tx.send(BridgeEvent::StatusUpdate(
                    msg_id, DeliveryStatus::ReceivedReply,
                )).await;
            }
            Ok(Ok(Err(e))) => {
                let _ = self.event_tx.send(BridgeEvent::Log(
                    format!("Remote execution failed: {e}")
                )).await;

                let entry = MessageEntry {
                    msg_id: format!("{}-err", msg_id),
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    direction: MessageDirection::Inbound,
                    content_preview: format!("[ERROR] {e}"),
                    status: DeliveryStatus::ReceivedReply,
                };
                let _ = self.event_tx.send(BridgeEvent::MessageReceived(entry)).await;
                let _ = self.event_tx.send(BridgeEvent::StatusUpdate(
                    msg_id, DeliveryStatus::ReceivedReply,
                )).await;
            }
            Ok(Err(join_err)) => {
                let _ = self.event_tx.send(BridgeEvent::Log(
                    format!("Remote execution task panicked: {join_err}")
                )).await;
                let entry = MessageEntry {
                    msg_id: format!("{}-err", msg_id),
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    direction: MessageDirection::Inbound,
                    content_preview: format!("[ERROR] Task panicked: {join_err}"),
                    status: DeliveryStatus::ReceivedReply,
                };
                let _ = self.event_tx.send(BridgeEvent::MessageReceived(entry)).await;
                let _ = self.event_tx.send(BridgeEvent::StatusUpdate(
                    msg_id, DeliveryStatus::ReceivedReply,
                )).await;
            }
            Err(_timeout) => {
                let _ = self.event_tx.send(BridgeEvent::Log(
                    format!("Remote execution timed out after {}s", timeout_secs)
                )).await;

                let entry = MessageEntry {
                    msg_id: format!("{}-timeout", msg_id),
                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    direction: MessageDirection::Inbound,
                    content_preview: format!("[TIMEOUT] No response after {}s", timeout_secs),
                    status: DeliveryStatus::ReceivedReply,
                };
                let _ = self.event_tx.send(BridgeEvent::MessageReceived(entry)).await;
                let _ = self.event_tx.send(BridgeEvent::StatusUpdate(
                    msg_id, DeliveryStatus::ReceivedReply,
                )).await;
            }
        }

        Ok(())
    }

    /// Start autossh tunnel only for the active transport (rsync needs no tunnel)
    pub async fn start_tunnels(&self) -> Vec<tokio::process::Child> {
        let active = self.active_transport().await;
        self.start_tunnel_for(active).await
    }

    /// Start the tunnel required for a specific transport
    async fn start_tunnel_for(&self, kind: TransportKind) -> Vec<tokio::process::Child> {
        let mut handles = Vec::new();

        match kind {
            TransportKind::Rsync => {
                let _ = self.event_tx.send(BridgeEvent::Log(
                    "rsync uses SSH directly — no tunnel needed".to_string()
                )).await;
            }
            TransportKind::Redis => {
                let _ = self.event_tx.send(BridgeEvent::Log(
                    format!("Starting Redis tunnel (port {})...", self.settings.redis.port)
                )).await;
                match crate::bridge::connection::start_autossh_tunnel(
                    &self.settings,
                    self.settings.redis.port,
                    self.settings.redis.port,
                ).await {
                    Ok(child) => {
                        handles.push(child);
                        let _ = self.event_tx.send(BridgeEvent::Log(
                            "Redis tunnel started".to_string()
                        )).await;
                    }
                    Err(e) => {
                        let _ = self.event_tx.send(BridgeEvent::Log(
                            format!("Redis tunnel failed: {e}")
                        )).await;
                    }
                }
            }
            TransportKind::Mcp => {
                let _ = self.event_tx.send(BridgeEvent::Log(
                    format!("Starting MCP tunnel (port {})...", self.settings.mcp_port)
                )).await;
                match crate::bridge::connection::start_autossh_tunnel(
                    &self.settings,
                    self.settings.mcp_port,
                    self.settings.mcp_port,
                ).await {
                    Ok(child) => {
                        handles.push(child);
                        let _ = self.event_tx.send(BridgeEvent::Log(
                            "MCP tunnel started".to_string()
                        )).await;
                    }
                    Err(e) => {
                        let _ = self.event_tx.send(BridgeEvent::Log(
                            format!("MCP tunnel failed: {e}")
                        )).await;
                    }
                }
            }
        }

        handles
    }
}
