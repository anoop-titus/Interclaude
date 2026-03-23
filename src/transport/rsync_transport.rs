use anyhow::Result;
use std::path::PathBuf;

use crate::bridge::message::Message;
use crate::bridge::sync::{self, SyncDirection};
use crate::config::Settings;
use crate::transport::{Transport, TransportKind};

/// File-based transport using rsync over SSH
pub struct RsyncTransport {
    settings: Settings,
    inbox_dir: PathBuf,
    outbox_dir: PathBuf,
}

impl RsyncTransport {
    pub fn new(settings: &Settings) -> Self {
        let base = settings.local_interclaude_dir();
        let (inbox_dir, outbox_dir) = match settings.role {
            crate::config::Role::Master => (
                base.join("Master/Inbox"),
                base.join("Master/Outbox"),
            ),
            crate::config::Role::Slave => (
                base.join("Slave/Inbox"),
                base.join("Slave/Outbox"),
            ),
        };

        Self {
            settings: settings.clone(),
            inbox_dir,
            outbox_dir,
        }
    }
}

impl Transport for RsyncTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Rsync
    }

    async fn send(&self, msg: &Message) -> Result<()> {
        // Write message to outbox as JSON file (always succeeds locally)
        let filename = msg.filename();
        let path = self.outbox_dir.join(&filename);
        let json = serde_json::to_string_pretty(msg)?;
        std::fs::write(&path, &json)?;

        // Trigger rsync push (best-effort — may fail on slave where remote=self)
        if let Err(e) = sync::rsync_once(&self.settings, SyncDirection::Push).await {
            crate::logging::log(&format!("rsync push failed (file saved locally): {e}"));
        }

        Ok(())
    }

    async fn receive(&self) -> Result<Vec<Message>> {
        // Trigger rsync pull (best-effort — don't let failure prevent reading local inbox)
        if let Err(e) = sync::rsync_once(&self.settings, SyncDirection::Pull).await {
            crate::logging::log(&format!("rsync pull failed (reading local inbox anyway): {e}"));
        }

        // Read new files from inbox — always attempt even if rsync pull failed,
        // because files may have been placed here by the remote's push
        let mut messages = Vec::new();
        let mut processed_paths = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.inbox_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "json") {
                    match std::fs::read_to_string(&path) {
                        Ok(content) => {
                            match serde_json::from_str::<Message>(&content) {
                                Ok(msg) => {
                                    messages.push(msg);
                                    processed_paths.push(path);
                                }
                                Err(e) => crate::logging::log(&format!("Failed to parse {}: {e}", path.display())),
                            }
                        }
                        Err(e) => crate::logging::log(&format!("Failed to read {}: {e}", path.display())),
                    }
                }
            }
        }

        // Clean up processed files to prevent re-reading and unbounded growth
        for path in processed_paths {
            let _ = std::fs::remove_file(&path);
        }

        // Sort by sequence number
        messages.sort_by_key(|m| m.sequence);

        Ok(messages)
    }

    async fn health_check(&self) -> Result<bool> {
        // Try a quick rsync dry-run to check connectivity
        let dest = self.settings.ssh_destination();
        let key = Settings::expand_path(&self.settings.key_path);

        let mut ssh_cmd = format!("ssh -F /dev/null -p {} -o StrictHostKeyChecking=accept-new -o ConnectTimeout=5",
            self.settings.ssh_port);
        if !key.is_empty() && std::path::Path::new(&key).exists() {
            ssh_cmd.push_str(&format!(" -i {key}"));
        }

        let output = tokio::process::Command::new("rsync")
            .args([
                "--dry-run",
                "-e", &ssh_cmd,
                &format!("{}:{}/", dest, self.settings.remote_dir),
                "/dev/null",
            ])
            .output()
            .await?;

        Ok(output.status.success())
    }
}
