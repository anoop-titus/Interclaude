use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::app::{DeliveryStatus, MessageDirection, MessageEntry};
use crate::bridge::message::{Message, MessagePayload, MessageType};
use crate::bridge::watcher::{PollingWatcher, WatchEvent};
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
    /// Transport health updated
    HealthUpdate(TransportKind, bool),
    /// Connection status changed
    ConnectionStatus(String),
    /// Status update for a specific message
    StatusUpdate(String, DeliveryStatus),
    /// Log message
    Log(String),
    /// Transport switched
    TransportSwitched(TransportKind),
}

/// The bridge engine manages transports, message flow, and session lifecycle
pub struct BridgeEngine {
    settings: Settings,
    event_tx: mpsc::Sender<BridgeEvent>,
    selector: Arc<Mutex<TransportSelector>>,
    ledger: Arc<Mutex<DedupLedger>>,
    status_tracker: Arc<StatusTracker>,
    sequence: Arc<Mutex<u64>>,
    session_id: String,
}

impl BridgeEngine {
    pub fn new(settings: Settings, event_tx: mpsc::Sender<BridgeEvent>) -> Result<Self> {
        let base = settings.local_interclaude_dir();
        let ledger = DedupLedger::new(&base)?;
        let status_tracker = StatusTracker::new(&base)?;
        let selector = TransportSelector::new(settings.active_transport);
        let session_id = uuid::Uuid::now_v7().to_string();

        Ok(Self {
            settings,
            event_tx,
            selector: Arc::new(Mutex::new(selector)),
            ledger: Arc::new(Mutex::new(ledger)),
            status_tracker: Arc::new(status_tracker),
            sequence: Arc::new(Mutex::new(0)),
            session_id,
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

    /// Send a command message using the active transport
    pub async fn send_command(&self, task: String) -> Result<String> {
        let seq = self.next_seq().await;
        let role = match self.settings.role {
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

    /// Send a message via the currently active transport
    async fn send_via_active(&self, msg: &Message) -> Result<()> {
        let active = self.active_transport().await;
        match active {
            TransportKind::Rsync => {
                let transport = RsyncTransport::new(&self.settings);
                transport.send(msg).await
            }
            TransportKind::Mcp => {
                let transport = McpTransport::new(&self.settings);
                transport.send(msg).await
            }
            TransportKind::Redis => {
                let transport = RedisTransport::new(&self.settings, &self.session_id);
                transport.send(msg).await
            }
        }
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
        let role = match self.settings.role {
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
            TransportKind::Rsync => {
                let t = RsyncTransport::new(&self.settings);
                t.health_check().await
            }
            TransportKind::Mcp => {
                let t = McpTransport::new(&self.settings);
                t.health_check().await
            }
            TransportKind::Redis => {
                let t = RedisTransport::new(&self.settings, &self.session_id);
                t.health_check().await
            }
        };

        let healthy = result.unwrap_or(false);
        let mut selector = self.selector.lock().await;
        selector.update_health(kind, healthy);
        let _ = self.event_tx.send(BridgeEvent::HealthUpdate(kind, healthy)).await;
        healthy
    }

    /// Run periodic health checks for all transports
    pub fn start_health_monitor(&self) -> JoinHandle<()> {
        let settings = self.settings.clone();
        let selector = self.selector.clone();
        let event_tx = self.event_tx.clone();
        let session_id = self.session_id.clone();

        tokio::spawn(async move {
            loop {
                for kind in [TransportKind::Rsync, TransportKind::Mcp, TransportKind::Redis] {
                    let healthy = match kind {
                        TransportKind::Rsync => {
                            let t = RsyncTransport::new(&settings);
                            t.health_check().await.unwrap_or(false)
                        }
                        TransportKind::Mcp => {
                            let t = McpTransport::new(&settings);
                            t.health_check().await.unwrap_or(false)
                        }
                        TransportKind::Redis => {
                            let t = RedisTransport::new(&settings, &session_id);
                            t.health_check().await.unwrap_or(false)
                        }
                    };

                    selector.lock().await.update_health(kind, healthy);
                    let _ = event_tx.send(BridgeEvent::HealthUpdate(kind, healthy)).await;
                }

                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            }
        })
    }

    /// Start polling for incoming messages on the active transport
    pub fn start_receive_loop(&self) -> JoinHandle<()> {
        let settings = self.settings.clone();
        let selector = self.selector.clone();
        let ledger = self.ledger.clone();
        let status_tracker = self.status_tracker.clone();
        let event_tx = self.event_tx.clone();
        let session_id = self.session_id.clone();
        let sync_interval = self.settings.sync_interval_secs;

        tokio::spawn(async move {
            loop {
                let active = selector.lock().await.active;

                let messages = match active {
                    TransportKind::Rsync => {
                        let t = RsyncTransport::new(&settings);
                        t.receive().await.unwrap_or_default()
                    }
                    TransportKind::Mcp => {
                        let t = McpTransport::new(&settings);
                        t.receive().await.unwrap_or_default()
                    }
                    TransportKind::Redis => {
                        let t = RedisTransport::new(&settings, &session_id);
                        t.receive().await.unwrap_or_default()
                    }
                };

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
                            let _ = event_tx.send(BridgeEvent::Log(
                                format!("Remote requested transport switch: {}", msg.preview())
                            )).await;
                        }
                        MessageType::Heartbeat => {
                            // Update connection status
                            let _ = event_tx.send(BridgeEvent::ConnectionStatus(
                                "Connected".to_string()
                            )).await;
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

    /// Start heartbeat sender
    pub fn start_heartbeat(&self) -> JoinHandle<()> {
        let settings = self.settings.clone();
        let selector = self.selector.clone();
        let session_id = self.session_id.clone();
        let sequence = self.sequence.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;

                let active = selector.lock().await.active;
                let mut seq = sequence.lock().await;
                *seq += 1;
                let seq_val = *seq;
                drop(seq);

                let role = match settings.role {
                    Role::Master => "master",
                    Role::Slave => "slave",
                };

                let heartbeat = Message::new_heartbeat(seq_val, role, active.label());

                let result = match active {
                    TransportKind::Rsync => {
                        let t = RsyncTransport::new(&settings);
                        t.send(&heartbeat).await
                    }
                    TransportKind::Mcp => {
                        let t = McpTransport::new(&settings);
                        t.send(&heartbeat).await
                    }
                    TransportKind::Redis => {
                        let t = RedisTransport::new(&settings, &session_id);
                        t.send(&heartbeat).await
                    }
                };

                if result.is_err() {
                    // Heartbeat failed — transport may be down
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
}
