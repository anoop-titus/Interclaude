use anyhow::{Context, Result};
use tokio::process::Command;

use crate::config::Settings;

/// Sync direction
pub enum SyncDirection {
    /// Push local outbox to remote inbox
    Push,
    /// Pull remote outbox to local inbox
    Pull,
}

/// Perform a one-shot rsync between local and remote
pub async fn rsync_once(settings: &Settings, direction: SyncDirection) -> Result<SyncResult> {
    let dest = settings.ssh_destination();
    let key = Settings::expand_path(&settings.key_path);
    let local_dir = settings.local_interclaude_dir();
    let remote_dir = &settings.remote_dir;

    // Build SSH command for rsync to use
    let mut ssh_cmd = format!("ssh -p {} -o StrictHostKeyChecking=accept-new", settings.ssh_port);
    if !key.is_empty() && std::path::Path::new(&key).exists() {
        ssh_cmd.push_str(&format!(" -i {key}"));
    }

    let (src, dst) = match direction {
        SyncDirection::Push => {
            // Master outbox -> remote Master inbox (for slave to read)
            // Also sync .status/ and .ledger
            let local_outbox = local_dir.join("Master/Outbox/");
            let remote_inbox = format!("{dest}:{remote_dir}/Master/Outbox/");
            (local_outbox.to_string_lossy().to_string(), remote_inbox)
        }
        SyncDirection::Pull => {
            // Remote Slave outbox -> local Slave inbox (for master to read)
            let remote_outbox = format!("{dest}:{remote_dir}/Slave/Outbox/");
            let local_inbox = local_dir.join("Slave/Outbox/");
            (remote_outbox, local_inbox.to_string_lossy().to_string())
        }
    };

    let start = std::time::Instant::now();

    let output = Command::new("rsync")
        .args([
            "-avz",                     // archive, verbose, compress
            "--timeout=10",             // 10s timeout
            "--itemize-changes",        // show what changed
            "-e", &ssh_cmd,             // use our SSH command
            &format!("{}/", src.trim_end_matches('/')),
            &dst,
        ])
        .output()
        .await
        .context("rsync failed to execute")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let elapsed = start.elapsed().as_millis() as u64;

    // Count transferred files from itemize output
    let files_transferred = stdout
        .lines()
        .filter(|l| l.starts_with('>') || l.starts_with('<'))
        .count();

    if output.status.success() {
        Ok(SyncResult {
            success: true,
            files_transferred,
            duration_ms: elapsed,
            message: if files_transferred > 0 {
                format!("{files_transferred} file(s) synced in {elapsed}ms")
            } else {
                format!("Up to date ({elapsed}ms)")
            },
        })
    } else {
        Ok(SyncResult {
            success: false,
            files_transferred: 0,
            duration_ms: elapsed,
            message: format!("rsync error: {stderr}"),
        })
    }
}

pub struct SyncResult {
    pub success: bool,
    pub files_transferred: usize,
    pub duration_ms: u64,
    pub message: String,
}

/// Sync the .ledger file bidirectionally (merge approach: union of both)
pub async fn sync_ledger(settings: &Settings) -> Result<()> {
    let dest = settings.ssh_destination();
    let key = Settings::expand_path(&settings.key_path);
    let local_dir = settings.local_interclaude_dir();
    let remote_dir = &settings.remote_dir;

    // Pull remote ledger content
    let mut ssh_args = settings.ssh_args();
    ssh_args.extend([
        dest.clone(),
        format!("cat {remote_dir}/.ledger 2>/dev/null || true"),
    ]);

    let output = Command::new("ssh")
        .args(&ssh_args)
        .output()
        .await
        .context("Failed to read remote ledger")?;

    let remote_ids: std::collections::HashSet<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    // Read local ledger
    let local_ledger_path = local_dir.join(".ledger");
    let local_content = std::fs::read_to_string(&local_ledger_path).unwrap_or_default();
    let local_ids: std::collections::HashSet<String> = local_content
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    // Merge: union of both
    let merged: std::collections::BTreeSet<String> = local_ids.union(&remote_ids).cloned().collect();
    let merged_content = merged.into_iter().collect::<Vec<_>>().join("\n") + "\n";

    // Write merged locally
    std::fs::write(&local_ledger_path, &merged_content)?;

    // Write merged to remote
    let mut ssh_args2 = settings.ssh_args();
    ssh_args2.extend([
        dest,
        format!("cat > {remote_dir}/.ledger"),
    ]);

    let mut child = Command::new("ssh")
        .args(&ssh_args2)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("Failed to write remote ledger")?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(merged_content.as_bytes()).await?;
        stdin.shutdown().await?;
    }

    child.wait().await?;

    Ok(())
}

/// Sync the .status/ directory (rsync both ways, newer wins)
pub async fn sync_status(settings: &Settings) -> Result<()> {
    let dest = settings.ssh_destination();
    let key = Settings::expand_path(&settings.key_path);
    let local_dir = settings.local_interclaude_dir();
    let remote_dir = &settings.remote_dir;

    let mut ssh_cmd = format!("ssh -p {} -o StrictHostKeyChecking=accept-new", settings.ssh_port);
    if !key.is_empty() && std::path::Path::new(&key).exists() {
        ssh_cmd.push_str(&format!(" -i {key}"));
    }

    // Push local status to remote (update flag = skip newer remote files)
    let local_status = local_dir.join(".status/");
    let remote_status = format!("{dest}:{remote_dir}/.status/");

    let _ = Command::new("rsync")
        .args(["-avz", "--update", "-e", &ssh_cmd,
            &format!("{}/", local_status.to_string_lossy()),
            &remote_status])
        .output()
        .await;

    // Pull remote status to local
    let _ = Command::new("rsync")
        .args(["-avz", "--update", "-e", &ssh_cmd,
            &remote_status,
            &format!("{}/", local_status.to_string_lossy())])
        .output()
        .await;

    Ok(())
}
