use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::correction::FixAction;

/// A fix queued for execution on next app startup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingFix {
    pub fix_action: FixAction,
    pub created_at: String,
    pub error_context: String,
    pub description: String,
}

fn pending_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".interclaude")
        .join("pending_fixes.json")
}

/// Save a pending fix to disk
pub fn save_pending(fix: &PendingFix) -> Result<()> {
    let path = pending_path();
    let mut fixes = load_pending().unwrap_or_default();
    fixes.push(fix.clone());

    let content = serde_json::to_string_pretty(&fixes)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Load all pending fixes from disk
pub fn load_pending() -> Result<Vec<PendingFix>> {
    let path = pending_path();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path)?;
    let fixes: Vec<PendingFix> = serde_json::from_str(&content)?;
    Ok(fixes)
}

/// Clear all pending fixes (after processing)
pub fn clear_pending() -> Result<()> {
    let path = pending_path();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
