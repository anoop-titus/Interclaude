use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Append-only deduplication ledger tracking processed message IDs
pub struct DedupLedger {
    path: PathBuf,
    seen: HashSet<String>,
}

impl DedupLedger {
    pub fn new(interclaude_dir: &Path) -> Result<Self> {
        let path = interclaude_dir.join(".ledger");
        let mut seen = HashSet::new();

        if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            for line in contents.lines() {
                let id = line.trim();
                if !id.is_empty() {
                    seen.insert(id.to_string());
                }
            }
        }

        Ok(Self { path, seen })
    }

    /// Check if a message ID has already been processed
    pub fn is_seen(&self, msg_id: &str) -> bool {
        self.seen.contains(msg_id)
    }

    /// Record a message ID as processed (append to ledger file)
    pub fn mark_seen(&mut self, msg_id: &str) -> Result<()> {
        if self.seen.insert(msg_id.to_string()) {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            writeln!(file, "{}", msg_id)?;
        }
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.seen.len()
    }
}
