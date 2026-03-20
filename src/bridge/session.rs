use anyhow::{Context, Result};
use tokio::process::Command;

use crate::config::Settings;

/// Launch a Claude Code session on the remote machine to execute a command
pub async fn remote_claude_exec(settings: &Settings, task: &str) -> Result<ClaudeExecResult> {
    let dest = settings.ssh_destination();
    let remote_dir = &settings.remote_dir;

    // Escape the task for shell
    let escaped_task = task.replace('\'', "'\\''");

    let cmd = format!(
        "cd {remote_dir} && claude -p '{escaped_task}' 2>&1"
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

/// Launch the slave watcher daemon on the remote machine
/// This starts a process that monitors the Slave/Inbox for new command files
/// and executes them via `claude -p`
pub async fn launch_slave_watcher(settings: &Settings) -> Result<tokio::process::Child> {
    let dest = settings.ssh_destination();
    let remote_dir = &settings.remote_dir;

    // The slave watcher script: monitors inbox, processes commands, writes responses to outbox
    let watcher_script = format!(r#"
cd {remote_dir}
echo "Slave watcher started at $(date)"
PROCESSED=""

while true; do
    for f in Slave/Inbox/*.json; do
        [ -f "$f" ] || continue

        # Skip already processed
        BASENAME=$(basename "$f")
        echo "$PROCESSED" | grep -q "$BASENAME" && continue

        # Check ledger
        MSG_ID=$(python3 -c "import json,sys; print(json.load(open('$f'))['msg_id'])" 2>/dev/null || echo "")
        if [ -n "$MSG_ID" ] && grep -q "$MSG_ID" .ledger 2>/dev/null; then
            PROCESSED="$PROCESSED $BASENAME"
            continue
        fi

        echo "Processing: $f"

        # Extract task from command JSON
        TASK=$(python3 -c "import json,sys; print(json.load(open('$f'))['payload']['task'])" 2>/dev/null || echo "")

        if [ -z "$TASK" ]; then
            echo "Could not parse task from $f"
            PROCESSED="$PROCESSED $BASENAME"
            continue
        fi

        # Execute via claude
        RESPONSE=$(claude -p "$TASK" 2>&1)
        EXIT_CODE=$?

        # Write response to outbox
        TIMESTAMP=$(date +%Y%m%d_%H%M%S)
        SEQ=$(echo "$BASENAME" | grep -o '[0-9]\{{4\}}' | head -1 || echo "0000")
        RESP_FILE="Slave/Outbox/${{TIMESTAMP}}_${{SEQ}}_response.json"

        python3 -c "
import json, datetime, uuid
resp = {{
    'msg_id': str(uuid.uuid4()),
    'msg_type': 'response',
    'timestamp': datetime.datetime.utcnow().isoformat() + 'Z',
    'sequence': int('$SEQ'),
    'sender_role': 'slave',
    'transport_used': 'rsync',
    'payload': {{
        'reply_to': '$MSG_ID',
        'stdout': '''$RESPONSE''',
        'stderr': '',
        'exit_code': $EXIT_CODE,
        'files_modified': [],
        'duration_ms': 0
    }}
}}
json.dump(resp, open('$RESP_FILE', 'w'), indent=2)
print(f'Response written to $RESP_FILE')
"

        # Mark as processed
        [ -n "$MSG_ID" ] && echo "$MSG_ID" >> .ledger
        PROCESSED="$PROCESSED $BASENAME"
    done

    sleep 2
done
"#);

    let mut args = settings.ssh_args();
    args.extend([
        "-o".to_string(), "ConnectTimeout=10".to_string(),
        "-o".to_string(), "ServerAliveInterval=15".to_string(),
        "-o".to_string(), "ServerAliveCountMax=3".to_string(),
        dest,
        watcher_script,
    ]);

    let child = Command::new("ssh")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to launch slave watcher")?;

    Ok(child)
}
