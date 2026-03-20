use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bridge::message::{Message, MessagePayload, MessageType};
use crate::config::Role;

/// Handshake message payload for role negotiation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakePayload {
    pub proposed_role: String,
    pub machine_id: String,
    pub session_id: String,
    pub protocol_version: String,
}

/// Handshake state machine
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeState {
    /// Not yet initiated
    Idle,
    /// We sent a handshake proposal, waiting for response
    Proposed,
    /// Handshake accepted, role confirmed
    Confirmed(Role),
    /// Handshake failed
    Failed(String),
}

/// Manages the role negotiation handshake between master and slave
pub struct Handshake {
    pub state: HandshakeState,
    pub local_machine_id: String,
    pub session_id: String,
}

impl Handshake {
    pub fn new(session_id: &str) -> Self {
        // Generate a stable machine ID from hostname
        let machine_id = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| Uuid::now_v7().to_string());

        Self {
            state: HandshakeState::Idle,
            local_machine_id: machine_id,
            session_id: session_id.to_string(),
        }
    }

    /// Create a handshake proposal message
    /// First machine to initiate becomes Master
    pub fn create_proposal(&mut self, desired_role: Role) -> Message {
        self.state = HandshakeState::Proposed;

        let role_str = match desired_role {
            Role::Master => "master",
            Role::Slave => "slave",
        };

        Message {
            msg_id: Uuid::now_v7().to_string(),
            msg_type: MessageType::Command, // Reuse command type for handshake
            timestamp: Utc::now(),
            sequence: 0,
            sender_role: role_str.to_string(),
            transport_used: "handshake".to_string(),
            payload: MessagePayload::Command(crate::bridge::message::CommandPayload {
                task: serde_json::to_string(&HandshakePayload {
                    proposed_role: role_str.to_string(),
                    machine_id: self.local_machine_id.clone(),
                    session_id: self.session_id.clone(),
                    protocol_version: "1.0".to_string(),
                })
                .unwrap_or_default(),
                working_dir: Some("__handshake__".to_string()),
                expected_output: Some("handshake".to_string()),
            }),
        }
    }

    /// Process a received handshake message
    pub fn process_handshake(&mut self, msg: &Message) -> Result<HandshakeResponse> {
        // Check if this is a handshake message
        let is_handshake = match &msg.payload {
            MessagePayload::Command(cmd) => {
                cmd.working_dir.as_deref() == Some("__handshake__")
                    && cmd.expected_output.as_deref() == Some("handshake")
            }
            _ => false,
        };

        if !is_handshake {
            return Ok(HandshakeResponse::NotHandshake);
        }

        let payload = match &msg.payload {
            MessagePayload::Command(cmd) => {
                serde_json::from_str::<HandshakePayload>(&cmd.task)?
            }
            _ => unreachable!(),
        };

        // If we're Idle and receive a proposal, the other side initiated first
        // They become Master (first to initiate = Master)
        match &self.state {
            HandshakeState::Idle => {
                // Other side proposed first — they're master, we're slave
                let our_role = if payload.proposed_role == "master" {
                    Role::Slave
                } else {
                    Role::Master
                };
                self.state = HandshakeState::Confirmed(our_role);
                Ok(HandshakeResponse::Accepted(our_role))
            }
            HandshakeState::Proposed => {
                // Both sides proposed simultaneously — use machine_id to break tie
                // Lexicographically smaller machine_id becomes Master
                let our_role = if self.local_machine_id < payload.machine_id {
                    Role::Master
                } else {
                    Role::Slave
                };
                self.state = HandshakeState::Confirmed(our_role);
                Ok(HandshakeResponse::Accepted(our_role))
            }
            HandshakeState::Confirmed(role) => {
                // Already confirmed, respond with current role
                Ok(HandshakeResponse::AlreadyConfirmed(*role))
            }
            HandshakeState::Failed(reason) => {
                Ok(HandshakeResponse::Error(reason.clone()))
            }
        }
    }

    /// Create a handshake acceptance response message
    pub fn create_acceptance(&self, accepted_role: Role) -> Message {
        let role_str = match accepted_role {
            Role::Master => "master",
            Role::Slave => "slave",
        };

        Message {
            msg_id: Uuid::now_v7().to_string(),
            msg_type: MessageType::Response,
            timestamp: Utc::now(),
            sequence: 0,
            sender_role: role_str.to_string(),
            transport_used: "handshake".to_string(),
            payload: MessagePayload::Response(crate::bridge::message::ResponsePayload {
                reply_to: "handshake".to_string(),
                stdout: format!("role_accepted:{}", role_str),
                stderr: String::new(),
                exit_code: 0,
                files_modified: vec![],
                duration_ms: 0,
            }),
        }
    }

    /// Check if a role swap is allowed and create swap messages
    pub fn create_role_swap(&mut self, new_role: Role) -> Message {
        let role_str = match new_role {
            Role::Master => "master",
            Role::Slave => "slave",
        };

        self.state = HandshakeState::Proposed;

        Message {
            msg_id: Uuid::now_v7().to_string(),
            msg_type: MessageType::Command,
            timestamp: Utc::now(),
            sequence: 0,
            sender_role: role_str.to_string(),
            transport_used: "handshake".to_string(),
            payload: MessagePayload::Command(crate::bridge::message::CommandPayload {
                task: serde_json::to_string(&HandshakePayload {
                    proposed_role: role_str.to_string(),
                    machine_id: self.local_machine_id.clone(),
                    session_id: self.session_id.clone(),
                    protocol_version: "1.0".to_string(),
                })
                .unwrap_or_default(),
                working_dir: Some("__handshake__".to_string()),
                expected_output: Some("role_swap".to_string()),
            }),
        }
    }
}

/// Response from processing a handshake
#[derive(Debug)]
pub enum HandshakeResponse {
    /// Not a handshake message
    NotHandshake,
    /// Handshake accepted, role assigned
    Accepted(Role),
    /// Already confirmed with existing role
    AlreadyConfirmed(Role),
    /// Handshake error
    Error(String),
}
