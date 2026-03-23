use serde::{Deserialize, Serialize};

use crate::transport::TransportKind;

/// Concrete fix actions that the ERE can apply
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FixAction {
    /// Retry the current connection (re-test SSH)
    RetryConnection,
    /// Switch to a different transport
    SwitchTransport(TransportKind),
    /// Update a config field
    UpdateConfig { field: String, value: String },
    /// Re-run dependency checks
    RerunDepCheck,
    /// Suggest installing a dependency (shows in log)
    InstallDep { name: String, command: String },
    /// Restart the bridge engine
    RestartBridge,
}

impl FixAction {
    pub fn label(&self) -> String {
        match self {
            Self::RetryConnection => "Retry connection".to_string(),
            Self::SwitchTransport(kind) => format!("Switch to {} transport", kind.label()),
            Self::UpdateConfig { field, value } => format!("Set {} = {}", field, value),
            Self::RerunDepCheck => "Re-run dependency checks".to_string(),
            Self::InstallDep { name, command } => format!("Install {} ({})", name, command),
            Self::RestartBridge => "Restart bridge engine".to_string(),
        }
    }
}

/// Parse the analysis suggested_action into a FixAction.
/// Uses keyword matching to map natural language suggestions to concrete actions.
pub fn parse_fix_action(suggested_action: &str, category: &str) -> FixAction {
    let lower = suggested_action.to_lowercase();

    if lower.contains("retry") || lower.contains("reconnect") || lower.contains("test connection") {
        return FixAction::RetryConnection;
    }

    if lower.contains("switch") && lower.contains("transport") {
        if lower.contains("rsync") {
            return FixAction::SwitchTransport(TransportKind::Rsync);
        }
        if lower.contains("mcp") {
            return FixAction::SwitchTransport(TransportKind::Mcp);
        }
        if lower.contains("redis") {
            return FixAction::SwitchTransport(TransportKind::Redis);
        }
    }

    if lower.contains("install") {
        // Extract package name if present
        let name = if lower.contains("mosh") { "mosh" }
            else if lower.contains("autossh") { "autossh" }
            else if lower.contains("rsync") { "rsync" }
            else if lower.contains("redis") { "redis" }
            else { "unknown" };

        let command = if lower.contains("brew") {
            format!("brew install {}", name)
        } else if lower.contains("apt") {
            format!("apt install {}", name)
        } else {
            format!("Install {}", name)
        };

        return FixAction::InstallDep {
            name: name.to_string(),
            command,
        };
    }

    if lower.contains("restart") && lower.contains("bridge") {
        return FixAction::RestartBridge;
    }

    if lower.contains("re-run") || lower.contains("recheck") || lower.contains("dependency") {
        if category == "welcome" {
            return FixAction::RerunDepCheck;
        }
    }

    // Default: retry connection for setup/bridge errors, rerun checks for welcome
    match category {
        "welcome" => FixAction::RerunDepCheck,
        _ => FixAction::RetryConnection,
    }
}
