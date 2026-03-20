use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message types in the Interclaude protocol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Command,
    Response,
    Status,
    Error,
    Heartbeat,
    TransportSwitch,
}

/// A message in the Interclaude protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub msg_id: String,
    pub msg_type: MessageType,
    pub timestamp: DateTime<Utc>,
    pub sequence: u64,
    pub sender_role: String,
    pub transport_used: String,
    pub payload: MessagePayload,
}

/// Message payload varies by type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessagePayload {
    Command(CommandPayload),
    Response(ResponsePayload),
    Status(StatusPayload),
    TransportSwitch(TransportSwitchPayload),
    Empty {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPayload {
    pub task: String,
    pub working_dir: Option<String>,
    pub expected_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePayload {
    pub reply_to: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub files_modified: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPayload {
    pub ref_msg_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportSwitchPayload {
    pub from: String,
    pub to: String,
}

impl Message {
    pub fn new_command(sequence: u64, task: String, role: &str, transport: &str) -> Self {
        Self {
            msg_id: Uuid::now_v7().to_string(),
            msg_type: MessageType::Command,
            timestamp: Utc::now(),
            sequence,
            sender_role: role.to_string(),
            transport_used: transport.to_string(),
            payload: MessagePayload::Command(CommandPayload {
                task,
                working_dir: None,
                expected_output: None,
            }),
        }
    }

    pub fn new_heartbeat(sequence: u64, role: &str, transport: &str) -> Self {
        Self {
            msg_id: Uuid::now_v7().to_string(),
            msg_type: MessageType::Heartbeat,
            timestamp: Utc::now(),
            sequence,
            sender_role: role.to_string(),
            transport_used: transport.to_string(),
            payload: MessagePayload::Empty {},
        }
    }

    /// Generate filename for this message
    pub fn filename(&self) -> String {
        let ts = self.timestamp.format("%Y%m%d_%H%M%S");
        let msg_type = match self.msg_type {
            MessageType::Command => "command",
            MessageType::Response => "response",
            MessageType::Status => "status",
            MessageType::Error => "error",
            MessageType::Heartbeat => "heartbeat",
            MessageType::TransportSwitch => "transport_switch",
        };
        format!("{}_{:04}_{}.json", ts, self.sequence, msg_type)
    }

    /// Content preview for TUI display
    pub fn preview(&self) -> String {
        match &self.payload {
            MessagePayload::Command(cmd) => {
                if cmd.task.len() > 60 {
                    format!("{}...", &cmd.task[..57])
                } else {
                    cmd.task.clone()
                }
            }
            MessagePayload::Response(resp) => {
                let prefix = if resp.exit_code == 0 { "OK" } else { "ERR" };
                let output = if resp.stdout.len() > 50 {
                    format!("{}...", &resp.stdout[..47])
                } else {
                    resp.stdout.clone()
                };
                format!("[{}] {}", prefix, output)
            }
            MessagePayload::Status(st) => format!("Status: {} for {}", st.status, st.ref_msg_id),
            MessagePayload::TransportSwitch(sw) => {
                format!("Switch: {} -> {}", sw.from, sw.to)
            }
            MessagePayload::Empty {} => "heartbeat".to_string(),
        }
    }
}
