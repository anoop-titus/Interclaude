use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::ErrorEntry;
use crate::api::anthropic::AnthropicClient;

/// Whether a fix can be applied within the current session or requires restart
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixType {
    InSession,
    OutOfSession,
}

/// Result of error analysis from the Claude API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub fix_type: FixType,
    pub summary: String,
    pub description: String,
    pub suggested_action: String,
    pub confidence: f32,
    pub original_error: ErrorEntry,
}

const SYSTEM_PROMPT: &str = r#"You are the Error Resolution Engine (ERE) for Interclaude, a cross-machine Claude Code bridge TUI application written in Rust.

Analyze the error and respond in EXACTLY this JSON format (no markdown, no extra text):
{
  "fix_type": "in_session" or "out_of_session",
  "summary": "One-line summary of the error",
  "description": "Detailed explanation of what went wrong",
  "suggested_action": "Specific action to fix it",
  "confidence": 0.0 to 1.0
}

Classification rules:
- "in_session": Connection retries, transport switches, config field corrections, re-running dep checks
- "out_of_session": Missing system packages requiring install, binary recompilation, system-level config changes, file permission issues

The application has these pages: Welcome (dependency checks), Setup (SSH connection config), Bridge (transport messaging).
Transports: rsync (file sync over SSH), MCP (JSON-RPC over SSH tunnel), Redis (pub/sub)."#;

/// Analyze an error using the Anthropic API
pub async fn analyze_error(
    error: &ErrorEntry,
    api_key: &str,
    model: &str,
) -> Result<AnalysisResult> {
    let client = AnthropicClient::new(api_key, model);

    let user_msg = format!(
        "Error from {} page:\nSeverity: {:?}\nSource: {}\nMessage: {}\nContext: {}",
        error.category.label(),
        error.severity,
        error.source,
        error.message,
        error.context,
    );

    let response = client.send(SYSTEM_PROMPT, &user_msg).await?;

    // Parse JSON response
    let parsed: serde_json::Value = serde_json::from_str(&response)
        .unwrap_or_else(|_| {
            // If the model didn't return valid JSON, construct a default
            serde_json::json!({
                "fix_type": "out_of_session",
                "summary": "Analysis incomplete",
                "description": response,
                "suggested_action": "Review the error manually",
                "confidence": 0.3
            })
        });

    let fix_type = match parsed.get("fix_type").and_then(|v| v.as_str()) {
        Some("in_session") => FixType::InSession,
        _ => FixType::OutOfSession,
    };

    Ok(AnalysisResult {
        fix_type,
        summary: parsed.get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error")
            .to_string(),
        description: parsed.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        suggested_action: parsed.get("suggested_action")
            .and_then(|v| v.as_str())
            .unwrap_or("Review manually")
            .to_string(),
        confidence: parsed.get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5) as f32,
        original_error: error.clone(),
    })
}
