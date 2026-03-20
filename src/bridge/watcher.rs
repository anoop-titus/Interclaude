use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

/// Events emitted by the file watcher
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// A new file appeared in a watched directory
    FileCreated(PathBuf),
    /// Watcher encountered an error
    Error(String),
}

/// Platform-aware file watcher that monitors a directory for new files
pub struct FileWatcher {
    watch_dir: PathBuf,
    tx: mpsc::Sender<WatchEvent>,
}

impl FileWatcher {
    /// Create a watcher for the given directory
    pub fn new(watch_dir: PathBuf, tx: mpsc::Sender<WatchEvent>) -> Self {
        Self { watch_dir, tx }
    }

    /// Start watching (spawns a background task, returns handle)
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(e) = self.run().await {
                let _ = self.tx.send(WatchEvent::Error(format!("Watcher error: {e}"))).await;
            }
        })
    }

    async fn run(&self) -> Result<()> {
        // Determine platform and use appropriate watcher
        if cfg!(target_os = "macos") {
            self.watch_fswatch().await
        } else {
            self.watch_inotifywait().await
        }
    }

    /// macOS: use fswatch
    async fn watch_fswatch(&self) -> Result<()> {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let mut child = tokio::process::Command::new("fswatch")
            .args([
                "-0",               // null-separated output
                "--event", "Created",
                "--event", "Updated",
                "-r",               // recursive
            ])
            .arg(&self.watch_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to start fswatch. Install: brew install fswatch")?;

        let stdout = child.stdout.take().context("No stdout from fswatch")?;
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::new();

        loop {
            buf.clear();
            let n = reader.read_until(0, &mut buf).await?;
            if n == 0 {
                break; // fswatch exited
            }

            let path_str = String::from_utf8_lossy(&buf[..buf.len().saturating_sub(1)]);
            let path = PathBuf::from(path_str.trim());

            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let _ = self.tx.send(WatchEvent::FileCreated(path)).await;
            }
        }

        Ok(())
    }

    /// Linux: use inotifywait
    async fn watch_inotifywait(&self) -> Result<()> {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let mut child = tokio::process::Command::new("inotifywait")
            .args([
                "-m",               // monitor continuously
                "-r",               // recursive
                "-e", "create",
                "-e", "moved_to",
                "--format", "%w%f",
            ])
            .arg(&self.watch_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to start inotifywait. Install: apt install inotify-tools")?;

        let stdout = child.stdout.take().context("No stdout from inotifywait")?;
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            let path = PathBuf::from(line.trim());
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let _ = self.tx.send(WatchEvent::FileCreated(path)).await;
            }
        }

        Ok(())
    }
}

/// Polling-based fallback watcher (no external deps needed)
/// Checks directory for new .json files at a given interval
pub struct PollingWatcher {
    watch_dir: PathBuf,
    interval: std::time::Duration,
    tx: mpsc::Sender<WatchEvent>,
}

impl PollingWatcher {
    pub fn new(watch_dir: PathBuf, interval: std::time::Duration, tx: mpsc::Sender<WatchEvent>) -> Self {
        Self { watch_dir, interval, tx }
    }

    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

            // Initialize with existing files
            if let Ok(entries) = std::fs::read_dir(&self.watch_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "json") {
                        seen.insert(path);
                    }
                }
            }

            loop {
                tokio::time::sleep(self.interval).await;

                if let Ok(entries) = std::fs::read_dir(&self.watch_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().is_some_and(|ext| ext == "json") && seen.insert(path.clone()) {
                            let _ = self.tx.send(WatchEvent::FileCreated(path)).await;
                        }
                    }
                }
            }
        })
    }
}
