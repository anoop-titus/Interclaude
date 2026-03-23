use anyhow::{Context, Result};
use tokio::process::Command;

use crate::config::{ConnectionKind, Settings};

/// Result of a connection test
pub struct ConnectionTestResult {
    pub success: bool,
    pub output: String,
    pub latency_ms: Option<u64>,
}

/// Test connectivity to the remote machine
pub async fn test_connection(settings: &Settings) -> ConnectionTestResult {
    let start = std::time::Instant::now();

    match settings.connection {
        ConnectionKind::Mosh => test_mosh(settings).await,
        ConnectionKind::Ssh => test_ssh(settings).await,
    }
    .unwrap_or_else(|e| ConnectionTestResult {
        success: false,
        output: format!("Connection failed: {e}"),
        latency_ms: Some(start.elapsed().as_millis() as u64),
    })
}

async fn test_ssh(settings: &Settings) -> Result<ConnectionTestResult> {
    let start = std::time::Instant::now();
    let dest = settings.ssh_destination();
    let mut args = settings.ssh_args();
    args.extend([
        "-o".to_string(), "ConnectTimeout=5".to_string(),
        dest,
        "echo INTERCLAUDE_OK && uname -a".to_string(),
    ]);

    let output = Command::new("ssh")
        .args(&args)
        .output()
        .await
        .context("Failed to execute ssh")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let latency = start.elapsed().as_millis() as u64;

    if output.status.success() && stdout.contains("INTERCLAUDE_OK") {
        let info = stdout.lines().skip(1).collect::<Vec<_>>().join("\n");
        Ok(ConnectionTestResult {
            success: true,
            output: format!("SSH OK ({latency}ms) - {info}"),
            latency_ms: Some(latency),
        })
    } else {
        Ok(ConnectionTestResult {
            success: false,
            output: format!("SSH failed: {stderr}"),
            latency_ms: Some(latency),
        })
    }
}

async fn test_mosh(settings: &Settings) -> Result<ConnectionTestResult> {
    // First check if mosh is available
    let mosh_check = Command::new("mosh").arg("--version").output().await;
    if mosh_check.is_err() {
        return Ok(ConnectionTestResult {
            success: false,
            output: "mosh not found. Install: brew install mosh (Mac) / apt install mosh (Linux)".to_string(),
            latency_ms: None,
        });
    }

    // MOSH doesn't have a simple "test" mode, so we test SSH first
    // (mosh uses SSH for initial handshake anyway)
    let ssh_result = test_ssh(settings).await?;
    if !ssh_result.success {
        return Ok(ConnectionTestResult {
            success: false,
            output: format!("MOSH requires SSH for handshake. {}", ssh_result.output),
            latency_ms: ssh_result.latency_ms,
        });
    }

    // Then verify mosh-server is available on remote
    let dest = settings.ssh_destination();
    let mut args = settings.ssh_args();
    args.extend([dest, "which mosh-server && mosh-server --version 2>&1 | head -1".to_string()]);

    let output = Command::new("ssh")
        .args(&args)
        .output()
        .await
        .context("Failed to check remote mosh-server")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if output.status.success() && !stdout.trim().is_empty() {
        Ok(ConnectionTestResult {
            success: true,
            output: format!("MOSH OK - SSH handshake: {}ms, remote mosh-server: {}",
                ssh_result.latency_ms.unwrap_or(0),
                stdout.lines().last().unwrap_or("found")),
            latency_ms: ssh_result.latency_ms,
        })
    } else {
        Ok(ConnectionTestResult {
            success: false,
            output: "mosh-server not found on remote. Install: apt install mosh".to_string(),
            latency_ms: ssh_result.latency_ms,
        })
    }
}

/// Setup the Interclaude directory structure on remote machine
pub async fn setup_remote_dirs(settings: &Settings) -> Result<String> {
    let dest = settings.ssh_destination();
    let remote_dir = &settings.remote_dir;
    let cmd = format!(
        "mkdir -p {remote_dir}/Master/Inbox {remote_dir}/Master/Outbox \
         {remote_dir}/Slave/Inbox {remote_dir}/Slave/Outbox \
         {remote_dir}/.status && \
         touch {remote_dir}/.ledger && \
         echo 'Directories created:' && ls -la {remote_dir}/"
    );

    let mut args = settings.ssh_args();
    args.extend([dest, cmd]);

    let output = Command::new("ssh")
        .args(&args)
        .output()
        .await
        .context("Failed to create remote directories")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        anyhow::bail!("Remote setup failed: {stderr}")
    }
}

/// Setup the Interclaude directory structure locally
pub fn setup_local_dirs(settings: &Settings) -> Result<String> {
    let base = settings.local_interclaude_dir();
    let dirs = [
        base.join("Master/Inbox"),
        base.join("Master/Outbox"),
        base.join("Slave/Inbox"),
        base.join("Slave/Outbox"),
        base.join(".status"),
    ];

    for dir in &dirs {
        std::fs::create_dir_all(dir)?;
    }

    // Create ledger file if it doesn't exist
    let ledger = base.join(".ledger");
    if !ledger.exists() {
        std::fs::write(&ledger, "")?;
    }

    Ok(format!("Local directories created at {}", base.display()))
}

/// Clean all file contents from remote Inbox/Outbox directories (best-effort).
/// Retains the directory structure itself.
pub async fn cleanup_remote_contents(settings: &Settings) {
    let dest = settings.ssh_destination();
    let remote_dir = &settings.remote_dir;
    let cmd = format!(
        "rm -f {remote_dir}/Master/Inbox/* {remote_dir}/Master/Outbox/* \
         {remote_dir}/Slave/Inbox/* {remote_dir}/Slave/Outbox/* \
         {remote_dir}/.status/* 2>/dev/null; \
         : > {remote_dir}/.ledger 2>/dev/null; \
         echo CLEANUP_OK"
    );

    let mut args = settings.ssh_args();
    args.extend([
        "-o".to_string(), "ConnectTimeout=5".to_string(),
        dest,
        cmd,
    ]);

    match Command::new("ssh").args(&args).output().await {
        Ok(output) => {
            if output.status.success() {
                crate::logging::log("Remote cleanup completed");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                crate::logging::log(&format!("Remote cleanup partial: {stderr}"));
            }
        }
        Err(e) => {
            crate::logging::log(&format!("Remote cleanup failed (non-fatal): {e}"));
        }
    }
}

/// Clean all file contents from local Inbox/Outbox directories.
pub fn cleanup_local_contents(settings: &Settings) {
    let base = settings.local_interclaude_dir();
    let dirs = [
        base.join("Master/Inbox"),
        base.join("Master/Outbox"),
        base.join("Slave/Inbox"),
        base.join("Slave/Outbox"),
        base.join(".status"),
    ];

    for dir in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    // Truncate ledger
    let ledger = base.join(".ledger");
    if ledger.exists() {
        let _ = std::fs::write(&ledger, "");
    }

    crate::logging::log("Local cleanup completed");
}

/// Start an autossh persistent tunnel for rsync
pub async fn start_autossh_tunnel(settings: &Settings, local_port: u16, remote_port: u16) -> Result<tokio::process::Child> {
    // Kill any stale autossh tunnel on the same local port
    let _ = Command::new("pkill")
        .args(["-f", &format!("autossh.*-L.*{}:localhost:{}", local_port, remote_port)])
        .output()
        .await;
    // Brief pause to let the port be released
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let dest = settings.ssh_destination();
    let key = Settings::expand_path(&settings.key_path);

    let mut args = vec![
        "-M".to_string(), "0".to_string(), // Disable autossh monitoring port, rely on ServerAlive
        "-N".to_string(),                   // No remote command
        "-F".to_string(), "/dev/null".to_string(), // Skip ~/.ssh/config (avoids Colima/OrbStack unsupported options)
        "-L".to_string(), format!("{local_port}:localhost:{remote_port}"),
        "-o".to_string(), "ServerAliveInterval=10".to_string(),
        "-o".to_string(), "ServerAliveCountMax=3".to_string(),
        "-o".to_string(), "StrictHostKeyChecking=accept-new".to_string(),
        "-p".to_string(), settings.ssh_port.to_string(),
    ];

    if !key.is_empty() && std::path::Path::new(&key).exists() {
        args.extend(["-i".to_string(), key]);
    }

    args.push(dest);

    let child = Command::new("autossh")
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to start autossh tunnel")?;

    Ok(child)
}

/// Push-install Interclaude to the remote machine
/// 1. Creates dirs on remote
/// 2. SCPs the local binary to remote ~/.local/bin/interclaude
/// 3. Makes it executable
pub async fn push_install_slave(settings: &Settings) -> Result<String> {
    let dest = settings.ssh_destination();

    // Step 1: Create remote directories
    let mkdir_cmd = format!(
        "mkdir -p ~/.local/bin {}/Master/Inbox {}/Master/Outbox {}/Slave/Inbox {}/Slave/Outbox {}/.status && touch {}/.ledger",
        settings.remote_dir, settings.remote_dir, settings.remote_dir,
        settings.remote_dir, settings.remote_dir, settings.remote_dir
    );

    let mut ssh_args = settings.ssh_args();
    ssh_args.extend([
        "-o".to_string(), "ConnectTimeout=10".to_string(),
        dest.clone(),
        mkdir_cmd,
    ]);

    let output = Command::new("ssh")
        .args(&ssh_args)
        .output()
        .await
        .context("Failed to create remote directories")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Remote mkdir failed: {stderr}");
    }

    // Step 2: Find cross-compiled Linux binary
    // Look for the musl-linked Linux binary, fall back to current exe
    let linux_candidates = [
        "target/x86_64-unknown-linux-musl/release/interclaude",
        "../target/x86_64-unknown-linux-musl/release/interclaude",
    ];

    let mut local_binary = None;
    for candidate in &linux_candidates {
        let path = std::path::PathBuf::from(candidate);
        if path.exists() {
            local_binary = Some(path);
            break;
        }
    }

    let local_binary = match local_binary {
        Some(p) => {
            crate::logging::log(&format!("Found Linux binary: {}", p.display()));
            p
        }
        None => {
            crate::logging::log("WARNING: No cross-compiled Linux binary found, pushing local binary");
            std::env::current_exe()
                .context("Could not determine local binary path")?
        }
    };

    if !local_binary.exists() {
        anyhow::bail!("Binary not found at {}", local_binary.display());
    }

    // Step 3: SCP binary to remote
    let remote_path = format!("{}:~/.local/bin/interclaude", dest);
    let key = Settings::expand_path(&settings.key_path);

    let mut scp_args = vec![
        "-F".to_string(), "/dev/null".to_string(),
        "-P".to_string(), settings.ssh_port.to_string(),
        "-o".to_string(), "StrictHostKeyChecking=accept-new".to_string(),
    ];
    if !key.is_empty() && std::path::Path::new(&key).exists() {
        scp_args.extend(["-i".to_string(), key]);
    }
    scp_args.push(local_binary.to_string_lossy().to_string());
    scp_args.push(remote_path);

    let output = Command::new("scp")
        .args(&scp_args)
        .output()
        .await
        .context("SCP failed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("SCP failed: {stderr}");
    }

    // Step 4: Make executable + push slave config
    let mut slave_settings = settings.clone();
    slave_settings.role = crate::config::Role::Slave;
    let slave_config = toml::to_string_pretty(&slave_settings).unwrap_or_default();

    let setup_cmd = format!(
        "chmod +x ~/.local/bin/interclaude && mkdir -p ~/.interclaude && cat > ~/.interclaude/config.toml << 'EOFCONFIG'\n{slave_config}\nEOFCONFIG"
    );

    let mut setup_args = settings.ssh_args();
    setup_args.extend([
        "-o".to_string(), "ConnectTimeout=10".to_string(),
        dest,
        setup_cmd,
    ]);

    let _ = Command::new("ssh")
        .args(&setup_args)
        .output()
        .await;

    Ok("Interclaude binary + slave config pushed to remote".to_string())
}

/// Execute a command on the remote machine via SSH
pub async fn remote_exec(settings: &Settings, command: &str) -> Result<(String, String, i32)> {
    let dest = settings.ssh_destination();
    let mut args = settings.ssh_args();
    args.extend([
        "-o".to_string(), "ConnectTimeout=10".to_string(),
        dest,
        command.to_string(),
    ]);

    let output = Command::new("ssh")
        .args(&args)
        .output()
        .await
        .context("SSH command failed")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    Ok((stdout, stderr, code))
}
