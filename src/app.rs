use std::sync::Arc;

use anyhow::Result;

use crate::config::Settings;
use crate::error::logging::ErrorStore;
use crate::transport::TransportKind;

/// Which TUI page is active
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Welcome,
    Setup,
    AccessPortal,
    Bridge,
}

/// Authentication mode for Anthropic API
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    OAuth,
    ApiKey,
}

impl AccessMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::OAuth => "OAuth (browser-based)",
            Self::ApiKey => "API Key (direct)",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            Self::OAuth => Self::ApiKey,
            Self::ApiKey => Self::OAuth,
        }
    }
}

/// Model selection for ERE
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelChoice {
    Sonnet46,
    Opus46,
    Haiku45,
}

impl ModelChoice {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Sonnet46 => "Claude Sonnet 4.6 (default)",
            Self::Opus46 => "Claude Opus 4.6",
            Self::Haiku45 => "Claude Haiku 4.5",
        }
    }

    pub fn model_id(&self) -> &'static str {
        match self {
            Self::Sonnet46 => "claude-sonnet-4-6",
            Self::Opus46 => "claude-opus-4-6",
            Self::Haiku45 => "claude-haiku-4-5-20251001",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            Self::Sonnet46 => Self::Opus46,
            Self::Opus46 => Self::Haiku45,
            Self::Haiku45 => Self::Sonnet46,
        }
    }
}

/// Which field is focused on the Access Portal page
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessPortalField {
    AccessMode,
    ApiKey,
    Model,
}

impl AccessPortalField {
    pub fn next(&self, show_api_key: bool) -> Self {
        match self {
            Self::AccessMode => {
                if show_api_key { Self::ApiKey } else { Self::Model }
            }
            Self::ApiKey => Self::Model,
            Self::Model => Self::AccessMode,
        }
    }

    pub fn prev(&self, show_api_key: bool) -> Self {
        match self {
            Self::AccessMode => Self::Model,
            Self::ApiKey => Self::AccessMode,
            Self::Model => {
                if show_api_key { Self::ApiKey } else { Self::AccessMode }
            }
        }
    }

    pub fn is_selector(&self) -> bool {
        matches!(self, Self::AccessMode | Self::Model)
    }
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
            Self::Delivered => "SENT",
            Self::Read => "READ",
            Self::Executing => "RUNNING",
            Self::Executed => "DONE",
            Self::Replying => "REPLYING",
            Self::ReceivingReply => "RECEIVING",
            Self::ReceivedReply => "COMPLETE",
            Self::DeliveryFailed => "FAILED",
            Self::ExecutionError => "ERROR",
            Self::Timeout => "TIMEOUT",
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Delivered => "→",
            Self::Read => "◉",
            Self::Executing => "⟳",
            Self::Executed => "✓",
            Self::Replying => "←",
            Self::ReceivingReply => "⇐",
            Self::ReceivedReply => "✔",
            Self::DeliveryFailed => "✗",
            Self::ExecutionError => "!",
            Self::Timeout => "⏱",
        }
    }

    /// Ordinal position in the pipeline (0-6 for normal flow)
    pub fn ordinal(&self) -> usize {
        match self {
            Self::Delivered => 0,
            Self::Read => 1,
            Self::Executing => 2,
            Self::Executed => 3,
            Self::Replying => 4,
            Self::ReceivingReply => 5,
            Self::ReceivedReply => 6,
            Self::DeliveryFailed | Self::ExecutionError | Self::Timeout => 99,
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

    /// Transport-aware next: skips Redis fields when Redis not selected
    pub fn next_visible(&self, show_redis: bool) -> Self {
        let next = self.next();
        if !show_redis && next.is_redis_field() {
            Self::RemoteHost
        } else {
            next
        }
    }

    /// Transport-aware prev: skips Redis fields when Redis not selected
    pub fn prev_visible(&self, show_redis: bool) -> Self {
        let prev = self.prev();
        if !show_redis && prev.is_redis_field() {
            Self::Transport
        } else {
            prev
        }
    }

    /// Whether this field cycles through options on Enter rather than accepting text input
    pub fn is_selector(&self) -> bool {
        matches!(self, Self::Connection | Self::Transport)
    }

    /// Whether this field belongs to the Redis configuration group
    pub fn is_redis_field(&self) -> bool {
        matches!(self, Self::RedisHost | Self::RedisPort | Self::RedisPassword)
    }
}

/// Which panel has focus on the Bridge page
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeFocus {
    Outbox,
    Inbox,
    Input,
}

impl BridgeFocus {
    pub fn next(&self) -> Self {
        match self {
            Self::Outbox => Self::Inbox,
            Self::Inbox => Self::Input,
            Self::Input => Self::Outbox,
        }
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
    pub bridge_focus: BridgeFocus,
    pub outbox_scroll: usize,
    pub inbox_scroll: usize,

    // Collapsible panels (Plan 11-05)
    pub show_status_panel: bool,
    pub show_pipeline_panel: bool,
    pub transport_recommendation: Option<(TransportKind, String)>,

    // Help overlay (Plan 11-04)
    pub show_help_overlay: bool,
    pub connected_at: Option<std::time::Instant>,

    // Command input (always visible on Bridge page)
    pub compose_input: String,

    // Access Portal state
    pub access_mode: AccessMode,
    pub api_key_input: String,
    pub model_selection: ModelChoice,
    pub access_portal_field: AccessPortalField,
    pub credentials_saved: bool,
    pub api_validation_status: Option<Result<String, String>>, // Some(Ok("valid")) or Some(Err("reason"))
    pub pending_api_validation: Option<(String, String)>, // (api_key, model) — consumed by event loop

    // Error Resolution Engine
    pub error_store: Arc<ErrorStore>,
    pub active_error_overlay: Option<crate::error::analysis::AnalysisResult>,
    pub show_error_details: bool, // toggle detailed view in overlay

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
            bridge_focus: BridgeFocus::Input,
            outbox_scroll: 0,
            inbox_scroll: 0,
            show_status_panel: true,
            show_pipeline_panel: true,
            transport_recommendation: None,
            show_help_overlay: false,
            connected_at: None,
            compose_input: String::new(),
            access_mode: AccessMode::ApiKey,
            api_key_input: String::new(),
            model_selection: ModelChoice::Sonnet46,
            access_portal_field: AccessPortalField::AccessMode,
            credentials_saved: false,
            api_validation_status: None,
            pending_api_validation: None,
            error_store: Arc::new(ErrorStore::new()),
            active_error_overlay: None,
            show_error_details: false,
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
            Page::Setup => Page::AccessPortal,
            Page::AccessPortal => Page::Bridge,
            Page::Bridge => Page::Bridge,
        };
    }

    pub fn prev_page(&mut self) {
        self.page = match self.page {
            Page::Welcome => Page::Welcome,
            Page::Setup => Page::Welcome,
            Page::AccessPortal => Page::Setup,
            Page::Bridge => Page::AccessPortal,
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

    /// Add to setup log + file log, auto-scroll to bottom.
    /// Automatically captures error-like messages to the ErrorStore.
    pub fn push_setup_log(&mut self, msg: String) {
        crate::logging::log(&format!("[SETUP] {msg}"));
        if is_error_message(&msg) {
            self.error_store.log(&crate::error::ErrorEntry::new(
                crate::error::ErrorCategory::Setup,
                classify_severity(&msg),
                "setup",
                &msg,
                "",
            ));
        }
        self.setup_log.push(msg);
        self.setup_log_scroll = u16::MAX;
    }

    /// Add to bridge log + file log, auto-scroll to bottom.
    /// Automatically captures error-like messages to the ErrorStore.
    pub fn push_bridge_log(&mut self, msg: String) {
        crate::logging::log(&format!("[BRIDGE] {msg}"));
        if is_error_message(&msg) {
            self.error_store.log(&crate::error::ErrorEntry::new(
                crate::error::ErrorCategory::Bridge,
                classify_severity(&msg),
                "bridge",
                &msg,
                "",
            ));
        }
        self.bridge_log.push(msg);
        if self.bridge_log.len() > 100 {
            self.bridge_log.drain(0..50);
        }
        self.bridge_log_scroll = u16::MAX;
    }

    /// Log an error from the Welcome page (dependency checks)
    pub fn push_welcome_error(&self, dep_name: &str, install_hint: &str) {
        self.error_store.log(&crate::error::ErrorEntry::new(
            crate::error::ErrorCategory::Welcome,
            crate::error::ErrorSeverity::Error,
            "dependency_check",
            format!("Required dependency missing: {}", dep_name),
            format!("Install with: {}", install_hint),
        ));
    }

    /// Whether Redis configuration should be shown (transport is Redis)
    pub fn show_redis_config(&self) -> bool {
        self.settings.active_transport == TransportKind::Redis
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
            ("Access", Page::AccessPortal, true),
            ("Bridge", Page::Bridge, self.ssh_test_passed),
        ]
    }

    /// Whether the API key field should be shown (access mode is ApiKey)
    pub fn show_api_key_field(&self) -> bool {
        self.access_mode == AccessMode::ApiKey
    }

    /// Recalculate transport recommendation based on health
    pub fn update_transport_recommendation(&mut self) {
        // Priority: Redis (fastest) > MCP > rsync (most reliable fallback)
        let candidates = [
            (TransportKind::Redis, 2, "lowest latency"),
            (TransportKind::Mcp, 1, "low latency"),
            (TransportKind::Rsync, 0, "most reliable"),
        ];

        for (kind, idx, reason) in &candidates {
            if self.transport_health[*idx] && self.active_transport != *kind {
                self.transport_recommendation = Some((*kind, reason.to_string()));
                return;
            }
        }

        // Current transport is already the best, or nothing healthy
        self.transport_recommendation = None;
    }

    /// Session duration as HH:MM string
    pub fn session_duration(&self) -> Option<String> {
        self.connected_at.map(|start| {
            let elapsed = start.elapsed();
            let mins = elapsed.as_secs() / 60;
            let hours = mins / 60;
            format!("{:02}:{:02}", hours, mins % 60)
        })
    }
}

/// Check if a log message looks like an error
fn is_error_message(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("fail") || lower.contains("error") || lower.contains("abort")
        || msg.starts_with("FAIL")
}

/// Classify error severity from message content
fn classify_severity(msg: &str) -> crate::error::ErrorSeverity {
    let lower = msg.to_lowercase();
    if lower.contains("critical") || lower.contains("fatal") || lower.contains("abort") {
        crate::error::ErrorSeverity::Critical
    } else if lower.contains("fail") || lower.contains("error") {
        crate::error::ErrorSeverity::Error
    } else {
        crate::error::ErrorSeverity::Warning
    }
}
