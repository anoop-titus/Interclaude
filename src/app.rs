use anyhow::Result;

use crate::config::Settings;
use crate::transport::TransportKind;

/// Which TUI page is active
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Welcome,
    Setup,
    Bridge,
}

/// Delivery status for a single message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStatus {
    Delivered,
    Read,
    Executing,
    Executed,
    Replying,
    ReceivingReply,
    ReceivedReply,
    DeliveryFailed,
    ExecutionError,
    Timeout,
}

impl DeliveryStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Delivered => "DELIVERED",
            Self::Read => "READ",
            Self::Executing => "EXECUTING",
            Self::Executed => "EXECUTED",
            Self::Replying => "REPLYING",
            Self::ReceivingReply => "RECEIVING",
            Self::ReceivedReply => "RECEIVED",
            Self::DeliveryFailed => "FAILED",
            Self::ExecutionError => "ERROR",
            Self::Timeout => "TIMEOUT",
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Delivered => ">>",
            Self::Read => "()",
            Self::Executing => "..",
            Self::Executed => "OK",
            Self::Replying => "<<",
            Self::ReceivingReply => "<-",
            Self::ReceivedReply => "++",
            Self::DeliveryFailed => "XX",
            Self::ExecutionError => "!!",
            Self::Timeout => "??",
        }
    }
}

/// A message in the conversation log
#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub msg_id: String,
    pub timestamp: String,
    pub direction: MessageDirection,
    pub content_preview: String,
    pub status: DeliveryStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
    Outbound,
    Inbound,
}

/// Dependency check result
#[derive(Debug, Clone)]
pub struct DepCheck {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
}

/// Which setup form field is focused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    RemoteHost,
    Connection,
    SshUser,
    SshPort,
    KeyPath,
    RemoteDir,
    Transport,
    RedisHost,
    RedisPort,
    RedisPassword,
}

impl SetupField {
    pub fn next(&self) -> Self {
        match self {
            Self::RemoteHost => Self::Connection,
            Self::Connection => Self::SshUser,
            Self::SshUser => Self::SshPort,
            Self::SshPort => Self::KeyPath,
            Self::KeyPath => Self::RemoteDir,
            Self::RemoteDir => Self::Transport,
            Self::Transport => Self::RedisHost,
            Self::RedisHost => Self::RedisPort,
            Self::RedisPort => Self::RedisPassword,
            Self::RedisPassword => Self::RemoteHost,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::RemoteHost => Self::RedisPassword,
            Self::Connection => Self::RemoteHost,
            Self::SshUser => Self::Connection,
            Self::SshPort => Self::SshUser,
            Self::KeyPath => Self::SshPort,
            Self::RemoteDir => Self::KeyPath,
            Self::Transport => Self::RemoteDir,
            Self::RedisHost => Self::Transport,
            Self::RedisPort => Self::RedisHost,
            Self::RedisPassword => Self::RedisPort,
        }
    }

    /// Whether this field cycles through options on Enter rather than accepting text input
    pub fn is_selector(&self) -> bool {
        matches!(self, Self::Connection | Self::Transport)
    }
}

/// Central application state
pub struct App {
    pub page: Page,
    pub running: bool,
    pub settings: Settings,
    pub dep_checks: Vec<DepCheck>,

    // Setup page state
    pub setup_field: SetupField,
    pub setup_log: Vec<String>,
    pub ssh_test_running: bool,

    // Bridge page state
    pub active_transport: TransportKind,
    pub transport_health: [bool; 3], // [rsync, mcp, redis]
    pub messages: Vec<MessageEntry>,
    pub connection_status: String,
    pub selected_message: Option<usize>,
    pub bridge_log: Vec<String>,

    // Command compose mode
    pub composing: bool,
    pub compose_input: String,

    // Tutorial panel
    pub tutorial_lines: Vec<String>,
}

impl App {
    pub fn new() -> Result<Self> {
        let settings = Settings::default();

        Ok(Self {
            page: Page::Welcome,
            running: true,
            settings,
            dep_checks: Vec::new(),
            setup_field: SetupField::RemoteHost,
            setup_log: Vec::new(),
            ssh_test_running: false,
            active_transport: TransportKind::Rsync,
            transport_health: [false, false, false],
            messages: Vec::new(),
            connection_status: "Disconnected".to_string(),
            selected_message: None,
            bridge_log: Vec::new(),
            composing: false,
            compose_input: String::new(),
            tutorial_lines: vec![
                "1. Install Claude Code on remote machine".to_string(),
                "2. Ensure SSH access is configured".to_string(),
                "3. Install mosh (recommended):".to_string(),
                "   Mac: brew install mosh".to_string(),
                "   Linux: apt install mosh".to_string(),
                "4. Install autossh: apt install autossh".to_string(),
                "5. Install rsync: apt install rsync".to_string(),
                "6. (Optional) Install Redis: apt install redis-server".to_string(),
                "".to_string(),
                "Why MOSH over SSH?".to_string(),
                "  - Survives network roaming (WiFi/cellular)".to_string(),
                "  - Handles intermittent connectivity".to_string(),
                "  - Local echo for instant feedback".to_string(),
                "  - Automatic reconnection (no broken pipes)".to_string(),
                "  - Uses UDP (faster than TCP for interactive)".to_string(),
            ],
        })
    }

    pub fn next_page(&mut self) {
        self.page = match self.page {
            Page::Welcome => Page::Setup,
            Page::Setup => Page::Bridge,
            Page::Bridge => Page::Bridge,
        };
    }

    pub fn prev_page(&mut self) {
        self.page = match self.page {
            Page::Welcome => Page::Welcome,
            Page::Setup => Page::Welcome,
            Page::Bridge => Page::Setup,
        };
    }

    pub fn cycle_transport(&mut self) {
        self.active_transport = match self.active_transport {
            TransportKind::Rsync => TransportKind::Mcp,
            TransportKind::Mcp => TransportKind::Redis,
            TransportKind::Redis => TransportKind::Rsync,
        };
    }

    pub fn set_transport(&mut self, kind: TransportKind) {
        self.active_transport = kind;
    }
}
