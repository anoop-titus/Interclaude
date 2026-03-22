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
    pub install_hint: String,
    pub required: bool,
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
    pub dep_check_complete: bool,
    pub frame_count: u64,
    pub auto_advance_ticks: Option<u16>,

    // Setup page state
    pub setup_field: SetupField,
    pub setup_log: Vec<String>,
    pub setup_log_scroll: u16,
    pub ssh_test_running: bool,
    pub ssh_test_passed: bool,

    // Bridge page state
    pub active_transport: TransportKind,
    pub transport_health: [bool; 3], // [rsync, mcp, redis]
    pub messages: Vec<MessageEntry>,
    pub connection_status: String,
    pub session_status: String,
    pub selected_message: Option<usize>,
    pub bridge_log: Vec<String>,
    pub bridge_log_scroll: u16,

    // Command input (always visible on Bridge page)
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
            dep_check_complete: false,
            frame_count: 0,
            auto_advance_ticks: None,
            setup_field: SetupField::RemoteHost,
            setup_log: Vec::new(),
            setup_log_scroll: 0,
            ssh_test_running: false,
            ssh_test_passed: false,
            active_transport: TransportKind::Rsync,
            transport_health: [false, false, false],
            messages: Vec::new(),
            connection_status: "Disconnected".to_string(),
            session_status: "Inactive".to_string(),
            selected_message: None,
            bridge_log: Vec::new(),
            bridge_log_scroll: 0,
            compose_input: String::new(),
            tutorial_lines: vec![
                "Pre-flight Installation Guide".to_string(),
                "".to_string(),
                "1. Install Claude Code on remote machine".to_string(),
                "2. Ensure SSH access is configured".to_string(),
                "3. Install mosh (recommended):".to_string(),
                "   Mac: brew install mosh".to_string(),
                "   Linux: apt install mosh".to_string(),
                "4. Install autossh (required for tunnels):".to_string(),
                "   Mac: brew install autossh".to_string(),
                "   Linux: apt install autossh".to_string(),
                "5. Install rsync: apt install rsync".to_string(),
                "".to_string(),
                "=== Transport Setup (3 modes) ===".to_string(),
                "".to_string(),
                "[rsync] File-based sync over SSH".to_string(),
                "  Always active as backbone transport".to_string(),
                "  No extra install needed (uses SSH)".to_string(),
                "".to_string(),
                "[Redis] Pub/Sub for real-time messaging".to_string(),
                "  Remote: apt install redis-server".to_string(),
                "  Config: redis-cli CONFIG SET".to_string(),
                "    requirepass <password>".to_string(),
                "  Master tunnels via autossh to remote".to_string(),
                "".to_string(),
                "[MCP] JSON-RPC over SSH tunnel".to_string(),
                "  Built into interclaude (no extra install)".to_string(),
                "  Slave listens on port 9876 (TCP)".to_string(),
                "  Master connects via autossh tunnel".to_string(),
                "  Enabled automatically when slave starts".to_string(),
                "".to_string(),
                "=== Setup Flow ===".to_string(),
                "1. Fill config fields (host, user, key)".to_string(),
                "2. [C-T] Test SSH connection".to_string(),
                "3. [C-A] Activate (auto: test+push+deploy)".to_string(),
                "4. Bridge page: [C-L] Launch slave".to_string(),
                "5. All 3 transports connect automatically".to_string(),
                "".to_string(),
                "GitHub: github.com/anoop-titus/Interclaude".to_string(),
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

    pub fn set_transport(&mut self, kind: TransportKind) {
        self.active_transport = kind;
    }

    /// Whether minimal required config is filled (host + user)
    pub fn has_remote_config(&self) -> bool {
        !self.settings.remote_host.is_empty() && !self.settings.ssh_user.is_empty()
    }

    /// Whether SSH connection has been tested successfully
    pub fn is_connection_tested(&self) -> bool {
        self.ssh_test_passed
    }

    /// Add to setup log + file log, auto-scroll to bottom
    pub fn push_setup_log(&mut self, msg: String) {
        crate::logging::log(&format!("[SETUP] {msg}"));
        self.setup_log.push(msg);
        self.setup_log_scroll = u16::MAX;
    }

    /// Add to bridge log + file log, auto-scroll to bottom
    pub fn push_bridge_log(&mut self, msg: String) {
        crate::logging::log(&format!("[BRIDGE] {msg}"));
        self.bridge_log.push(msg);
        if self.bridge_log.len() > 100 {
            self.bridge_log.drain(0..50);
        }
        self.bridge_log_scroll = u16::MAX;
    }

    /// Whether all required dependencies are available
    pub fn all_required_met(&self) -> bool {
        self.dep_check_complete
            && self.dep_checks.iter().filter(|d| d.required).all(|d| d.available)
    }

    /// Get missing required deps
    pub fn missing_required_deps(&self) -> Vec<&DepCheck> {
        self.dep_checks.iter().filter(|d| d.required && !d.available).collect()
    }

    /// Page tab info: (label, page, enabled)
    pub fn page_tabs(&self) -> Vec<(&'static str, Page, bool)> {
        vec![
            ("Welcome", Page::Welcome, true),
            ("Setup", Page::Setup, true),
            ("Bridge", Page::Bridge, self.ssh_test_passed),
        ]
    }
}
