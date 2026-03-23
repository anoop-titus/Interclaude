use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use super::{ErrorCategory, ErrorEntry};

/// File-based error store writing JSON-lines per category
pub struct ErrorStore {
    errors_dir: PathBuf,
    /// In-memory buffer of recent errors (for overlay display)
    recent: Mutex<Vec<ErrorEntry>>,
}

impl ErrorStore {
    pub fn new() -> Self {
        let errors_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".interclaude")
            .join("errors");
        let _ = std::fs::create_dir_all(&errors_dir);

        Self {
            errors_dir,
            recent: Mutex::new(Vec::new()),
        }
    }

    /// Log an error entry to the appropriate category file and in-memory buffer
    pub fn log(&self, entry: &ErrorEntry) {
        // Write to file
        let filename = format!("{}.jsonl", entry.category.label());
        let path = self.errors_dir.join(&filename);

        if let Ok(json) = serde_json::to_string(entry) {
            // Rotate if file exceeds 1MB
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.len() > 1_048_576 {
                    let backup = self.errors_dir.join(format!("{}.jsonl.old", entry.category.label()));
                    let _ = std::fs::rename(&path, backup);
                }
            }

            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(file, "{}", json);
            }
        }

        // Also log to main log file
        crate::logging::log(&format!(
            "[ERE:{}:{}] {} — {}",
            entry.category.label(),
            entry.severity.label(),
            entry.source,
            entry.message
        ));

        // Add to in-memory recent buffer (keep last 50)
        if let Ok(mut recent) = self.recent.lock() {
            recent.push(entry.clone());
            let len = recent.len();
            if len > 50 {
                recent.drain(0..len - 50);
            }
        }
    }

    /// Get the most recent error (for overlay trigger)
    pub fn latest(&self) -> Option<ErrorEntry> {
        self.recent.lock().ok()?.last().cloned()
    }

    /// Get recent errors for a specific category
    pub fn recent_by_category(&self, category: ErrorCategory) -> Vec<ErrorEntry> {
        self.recent
            .lock()
            .ok()
            .map(|r| r.iter().filter(|e| e.category == category).cloned().collect())
            .unwrap_or_default()
    }

    /// Get all recent errors
    pub fn all_recent(&self) -> Vec<ErrorEntry> {
        self.recent.lock().ok().map(|r| r.clone()).unwrap_or_default()
    }
}

impl super::ErrorSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Warning => "WARN",
            Self::Error => "ERROR",
            Self::Critical => "CRIT",
        }
    }
}
