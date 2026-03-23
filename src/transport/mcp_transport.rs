use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::bridge::message::Message;
use crate::config::Settings;
use crate::transport::{Transport, TransportKind};

/// MCP-style transport over SSH-tunneled TCP
/// Implements a simple JSON-RPC-like protocol:
///   Request:  {"method": "...", "params": {...}}\n
///   Response: {"result": ...}\n
pub struct McpTransport {
    settings: Settings,
    inbox_dir: PathBuf,
    outbox_dir: PathBuf,
}

impl McpTransport {
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

    /// Connect to the MCP server via SSH tunnel (localhost:mcp_port)
    async fn connect(&self) -> Result<TcpStream> {
        let addr = format!("127.0.0.1:{}", self.settings.mcp_port);
        let stream = TcpStream::connect(&addr)
            .await
            .context(format!("Failed to connect to MCP server at {addr}. Is the SSH tunnel running?"))?;
        Ok(stream)
    }

    /// Send a JSON-RPC request and get a response
    async fn rpc_call(&self, method: &str, params: &serde_json::Value) -> Result<serde_json::Value> {
        let mut stream = self.connect().await?;

        let request = serde_json::json!({
            "method": method,
            "params": params,
        });

        let mut req_bytes = serde_json::to_vec(&request)?;
        req_bytes.push(b'\n');
        stream.write_all(&req_bytes).await?;
        stream.flush().await?;

        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let response: serde_json::Value = serde_json::from_str(line.trim())?;
        Ok(response)
    }

    /// Write message to outbox for audit trail
    fn write_audit(&self, msg: &Message) -> Result<()> {
        let filename = msg.filename();
        let path = self.outbox_dir.join(&filename);
        let json = serde_json::to_string_pretty(msg)?;
        std::fs::write(&path, &json)?;
        Ok(())
    }
}

impl Transport for McpTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Mcp
    }

    async fn send(&self, msg: &Message) -> Result<()> {
        // Write to outbox for audit trail
        self.write_audit(msg)?;

        // Send via MCP
        let params = serde_json::to_value(msg)?;
        let response = self.rpc_call("send_message", &params).await?;

        if let Some(error) = response.get("error") {
            anyhow::bail!("MCP send failed: {}", error);
        }

        Ok(())
    }

    async fn receive(&self) -> Result<Vec<Message>> {
        let params = serde_json::json!({});
        let response = self.rpc_call("receive_messages", &params).await?;

        let messages: Vec<Message> = if let Some(result) = response.get("result") {
            serde_json::from_value(result.clone()).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Write received messages to inbox for audit trail
        for msg in &messages {
            let filename = msg.filename();
            let path = self.inbox_dir.join(&filename);
            if !path.exists() {
                let json = serde_json::to_string_pretty(msg)?;
                let _ = std::fs::write(&path, &json);
            }
        }

        Ok(messages)
    }

    async fn health_check(&self) -> Result<bool> {
        match self.rpc_call("health_check", &serde_json::json!({})).await {
            Ok(response) => {
                Ok(response.get("result")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false))
            }
            Err(_) => Ok(false),
        }
    }
}

// =========================================================================
// MCP Server — runs on the remote side, listens on a TCP port
// =========================================================================

use tokio::net::TcpListener;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Simple MCP server that receives messages and queues them
pub struct McpServer {
    port: u16,
    inbox_dir: PathBuf,
    outbox_dir: PathBuf,
    pending_outbound: Arc<Mutex<Vec<Message>>>,
}

impl McpServer {
    pub fn new(port: u16, interclaude_dir: &std::path::Path, role: crate::config::Role) -> Self {
        let (inbox_dir, outbox_dir) = match role {
            crate::config::Role::Master => (
                interclaude_dir.join("Master/Inbox"),
                interclaude_dir.join("Master/Outbox"),
            ),
            crate::config::Role::Slave => (
                interclaude_dir.join("Slave/Inbox"),
                interclaude_dir.join("Slave/Outbox"),
            ),
        };

        Self {
            port,
            inbox_dir,
            outbox_dir,
            pending_outbound: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Queue a message for sending (called by local code)
    pub async fn queue_message(&self, msg: Message) {
        self.pending_outbound.lock().await.push(msg);
    }

    /// Start the MCP server (blocking, run in a spawned task)
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))
            .await
            .context(format!("Failed to bind MCP server on port {}", self.port))?;

        loop {
            let (stream, _addr) = listener.accept().await?;
            let inbox_dir = self.inbox_dir.clone();
            let outbox_dir = self.outbox_dir.clone();
            let pending = self.pending_outbound.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_mcp_connection(stream, inbox_dir, outbox_dir, pending).await {
                    crate::logging::log(&format!("MCP connection error: {e}"));
                }
            });
        }
    }
}

async fn handle_mcp_connection(
    mut stream: TcpStream,
    inbox_dir: PathBuf,
    _outbox_dir: PathBuf,
    pending: Arc<Mutex<Vec<Message>>>,
) -> Result<()> {
    let (reader, mut writer) = stream.split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    while buf_reader.read_line(&mut line).await? > 0 {
        let request: serde_json::Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = serde_json::json!({"error": format!("Invalid JSON: {e}")});
                let mut resp_bytes = serde_json::to_vec(&err_resp)?;
                resp_bytes.push(b'\n');
                writer.write_all(&resp_bytes).await?;
                line.clear();
                continue;
            }
        };

        let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(serde_json::json!({}));

        let response = match method {
            "send_message" => {
                // Receive a message from the remote side — write to inbox
                match serde_json::from_value::<Message>(params) {
                    Ok(msg) => {
                        let filename = msg.filename();
                        let path = inbox_dir.join(&filename);
                        let json = serde_json::to_string_pretty(&msg)?;
                        std::fs::write(&path, &json)?;
                        serde_json::json!({"result": "ok"})
                    }
                    Err(e) => serde_json::json!({"error": format!("Invalid message: {e}")}),
                }
            }
            "receive_messages" => {
                // Send pending outbound messages to the caller
                let mut msgs = pending.lock().await;
                let result = serde_json::to_value(&*msgs)?;
                msgs.clear();
                serde_json::json!({"result": result})
            }
            "health_check" => {
                serde_json::json!({"result": true})
            }
            _ => {
                serde_json::json!({"error": format!("Unknown method: {method}")})
            }
        };

        let mut resp_bytes = serde_json::to_vec(&response)?;
        resp_bytes.push(b'\n');
        writer.write_all(&resp_bytes).await?;
        writer.flush().await?;
        line.clear();
    }

    Ok(())
}
