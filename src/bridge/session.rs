use anyhow::{Context, Result};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::config::Settings;

/// Launch a Claude Code session on the remote machine to execute a command
pub async fn remote_claude_exec(settings: &Settings, task: &str) -> Result<ClaudeExecResult> {
    let dest = settings.ssh_destination();
    let remote_dir = &settings.remote_dir;

    // Escape the task for shell
    let escaped_task = task.replace('\'', "'\\''");

    let cmd = format!(
        "cd {remote_dir} && claude -p '{escaped_task}' < /dev/null 2>/dev/null"
    );

    let mut args = settings.ssh_args();
    args.extend([
        "-o".to_string(), "ConnectTimeout=10".to_string(),
        dest,
        cmd,
    ]);

    let start = std::time::Instant::now();

    let output = Command::new("ssh")
        .args(&args)
        .output()
        .await
        .context("Failed to execute remote Claude session")?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok(ClaudeExecResult {
        stdout,
        stderr,
        exit_code,
        duration_ms,
    })
}

pub struct ClaudeExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

/// Streaming variant: spawns SSH + `claude -p` and sends each line of stdout as it arrives.
/// The sender receives accumulated text (not just the new line) so the TUI can replace-in-place.
pub async fn remote_claude_exec_streaming(
    settings: &Settings,
    task: &str,
    chunk_tx: mpsc::Sender<String>,
) -> Result<ClaudeExecResult> {
    let dest = settings.ssh_destination();
    let remote_dir = &settings.remote_dir;

    let escaped_task = task.replace('\'', "'\\''");

    // Use -tt to force PTY allocation so Claude's output is flushed line-by-line
    let cmd = format!(
        "cd {remote_dir} && claude -p '{escaped_task}' < /dev/null 2>/dev/null"
    );

    let mut args = settings.ssh_args();
    args.extend([
        "-o".to_string(), "ConnectTimeout=10".to_string(),
        "-tt".to_string(), // force PTY for line-buffered output
        dest,
        cmd,
    ]);

    let start = std::time::Instant::now();

    let mut child = Command::new("ssh")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to spawn streaming remote Claude session")?;

    let stdout = child.stdout.take().context("No stdout pipe")?;
    let mut reader = tokio::io::BufReader::new(stdout);
    let mut accumulated = String::new();
    let mut line_buf = String::new();

    loop {
        line_buf.clear();
        match reader.read_line(&mut line_buf).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                // Strip PTY carriage returns
                let clean = line_buf.replace('\r', "");
                accumulated.push_str(&clean);
                // Best-effort send — if TUI is slow, skip this chunk
                let _ = chunk_tx.try_send(accumulated.clone());
            }
            Err(e) => {
                crate::logging::log(&format!("Stream read error: {e}"));
                break;
            }
        }
    }

    let status = child.wait().await.context("Failed to wait for SSH process")?;
    let duration_ms = start.elapsed().as_millis() as u64;
    let exit_code = status.code().unwrap_or(-1);

    Ok(ClaudeExecResult {
        stdout: accumulated,
        stderr: String::new(),
        exit_code,
        duration_ms,
    })
}

/// Launch `interclaude --slave` on the remote machine via SSH.
/// The binary must already be installed at ~/.local/bin/interclaude on remote
/// (done by the PushInstall step in setup).
pub async fn launch_slave_watcher(settings: &Settings) -> Result<tokio::process::Child> {
    let dest = settings.ssh_destination();

    // Run the proper interclaude binary in slave mode on remote
    // Use absolute path since ~/.local/bin may not be in remote PATH
    let slave_cmd = "$HOME/.local/bin/interclaude --slave".to_string();

    let mut args = settings.ssh_args();
    args.extend([
        "-o".to_string(), "ConnectTimeout=10".to_string(),
        "-o".to_string(), "ServerAliveInterval=15".to_string(),
        "-o".to_string(), "ServerAliveCountMax=3".to_string(),
        "-t".to_string(), "-t".to_string(), // force PTY so remote process dies when SSH drops
        dest,
        slave_cmd,
    ]);

    let child = Command::new("ssh")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to launch interclaude --slave on remote")?;

    Ok(child)
}
