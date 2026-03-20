use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::app::DeliveryStatus;

/// Timestamped delivery status entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEntry {
    pub msg_id: String,
    pub status: String,
    pub timestamp: DateTime<Utc>,
}

/// Tracks delivery status for all messages via .status/ directory
pub struct StatusTracker {
    status_dir: PathBuf,
}

impl StatusTracker {
    pub fn new(interclaude_dir: &Path) -> Result<Self> {
        let status_dir = interclaude_dir.join(".status");
        std::fs::create_dir_all(&status_dir)?;
        Ok(Self { status_dir })
    }

    /// Update the status for a message
    pub fn update(&self, msg_id: &str, status: DeliveryStatus) -> Result<()> {
        let entry = StatusEntry {
            msg_id: msg_id.to_string(),
            status: status.label().to_string(),
            timestamp: Utc::now(),
        };

        let path = self.status_dir.join(format!("{}.json", msg_id));

        // Read existing history or start fresh
        let mut history: Vec<StatusEntry> = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        history.push(entry);
        let json = serde_json::to_string_pretty(&history)?;
        std::fs::write(&path, json)?;

        Ok(())
    }

    /// Get the latest status for a message
    pub fn get_latest(&self, msg_id: &str) -> Result<Option<StatusEntry>> {
        let path = self.status_dir.join(format!("{}.json", msg_id));
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)?;
        let history: Vec<StatusEntry> = serde_json::from_str(&content)?;
        Ok(history.into_iter().last())
    }

    /// Get full status history for a message
    pub fn get_history(&self, msg_id: &str) -> Result<Vec<StatusEntry>> {
        let path = self.status_dir.join(format!("{}.json", msg_id));
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&path)?;
        let history: Vec<StatusEntry> = serde_json::from_str(&content)?;
        Ok(history)
    }
}
