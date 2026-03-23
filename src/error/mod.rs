pub mod analysis;
pub mod correction;
pub mod logging;
pub mod pending;

use chrono::Local;
use serde::{Deserialize, Serialize};

/// Error category by TUI page source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorCategory {
    Welcome,
    Setup,
    Bridge,
}

impl ErrorCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::Setup => "setup",
            Self::Bridge => "bridge",
        }
    }
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorSeverity {
    Warning,
    Error,
    Critical,
}

/// A structured error entry captured by the ERE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEntry {
    pub timestamp: String,
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub source: String,
    pub message: String,
    pub context: String,
}

impl ErrorEntry {
    pub fn new(
        category: ErrorCategory,
        severity: ErrorSeverity,
        source: impl Into<String>,
        message: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
            category,
            severity,
            source: source.into(),
            message: message.into(),
            context: context.into(),
        }
    }
}
