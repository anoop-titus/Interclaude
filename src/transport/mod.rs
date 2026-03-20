pub mod dedup;
pub mod mcp_transport;
pub mod redis_transport;
pub mod rsync_transport;
pub mod status;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::bridge::message::Message;

/// The three available transport pathways
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    Rsync,
    Mcp,
    Redis,
}

impl TransportKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rsync => "rsync",
            Self::Mcp => "MCP",
            Self::Redis => "Redis",
        }
    }

    pub fn hotkey(&self) -> &'static str {
        match self {
            Self::Rsync => "1",
            Self::Mcp => "2",
            Self::Redis => "3",
        }
    }

    pub fn from_hotkey(key: char) -> Option<Self> {
        match key {
            '1' => Some(Self::Rsync),
            '2' => Some(Self::Mcp),
            '3' => Some(Self::Redis),
            _ => None,
        }
    }
}

/// Trait that all transport pathways implement
#[allow(async_fn_in_trait)]
pub trait Transport {
    /// Which transport kind this is
    fn kind(&self) -> TransportKind;

    /// Send a message to the remote side
    async fn send(&self, msg: &Message) -> Result<()>;

    /// Check for new messages from the remote side
    async fn receive(&self) -> Result<Vec<Message>>;

    /// Check if this transport is healthy/connected
    async fn health_check(&self) -> Result<bool>;
}

/// Manages all three transports, user can switch at any time
pub struct TransportSelector {
    pub active: TransportKind,
    pub health: [bool; 3], // [rsync, mcp, redis]
}

impl TransportSelector {
    pub fn new(initial: TransportKind) -> Self {
        Self {
            active: initial,
            health: [false, false, false],
        }
    }

    pub fn set_active(&mut self, kind: TransportKind) {
        self.active = kind;
    }

    pub fn update_health(&mut self, kind: TransportKind, healthy: bool) {
        let idx = match kind {
            TransportKind::Rsync => 0,
            TransportKind::Mcp => 1,
            TransportKind::Redis => 2,
        };
        self.health[idx] = healthy;
    }

    pub fn is_healthy(&self, kind: TransportKind) -> bool {
        let idx = match kind {
            TransportKind::Rsync => 0,
            TransportKind::Mcp => 1,
            TransportKind::Redis => 2,
        };
        self.health[idx]
    }
}
