use serde::{Deserialize, Serialize};

use crate::transport::TransportKind;
use super::credentials::CredentialConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub remote_host: String,
    pub ssh_user: String,
    pub ssh_port: u16,
    pub key_path: String,
    pub remote_dir: String,
    pub local_dir: String,
    pub sync_interval_secs: u64,
    pub role: Role,
    pub connection: ConnectionKind,
    pub active_transport: TransportKind,
    pub redis: RedisConfig,
    pub mcp_port: u16,
    pub message_timeout_secs: u64,
    #[serde(default)]
    pub credentials: CredentialConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Master,
    Slave,
}

/// Connection protocol: MOSH (recommended) or SSH
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionKind {
    Mosh,
    Ssh,
}

impl ConnectionKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Mosh => "MOSH (recommended)",
            Self::Ssh => "SSH",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            Self::Mosh => Self::Ssh,
            Self::Ssh => Self::Mosh,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            remote_host: String::new(),
            ssh_user: String::new(),
            ssh_port: 22,
            key_path: "~/.ssh/id_ed25519".to_string(),
            remote_dir: "~/Interclaude".to_string(),
            local_dir: "~/Interclaude".to_string(),
            sync_interval_secs: 2,
            role: Role::Master,
            connection: ConnectionKind::Mosh,
            active_transport: TransportKind::Rsync,
            redis: RedisConfig {
                host: "127.0.0.1".to_string(),
                port: 6379,
                password: String::new(),
            },
            mcp_port: 9876,
            message_timeout_secs: 300,
            credentials: CredentialConfig::default(),
        }
    }
}

impl Settings {
    pub fn config_dir() -> std::path::PathBuf {
        dirs::home_dir()
            .map(|p| p.join(".interclaude"))
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp/.interclaude"))
    }

    pub fn config_path() -> std::path::PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Load settings from disk, falling back to defaults
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(settings) => return settings,
                    Err(e) => crate::logging::log(&format!("Config parse error: {e}")),
                },
                Err(e) => crate::logging::log(&format!("Config read error: {e}")),
            }
        }
        Self::default()
    }

    /// Save settings to disk
    pub fn save(&self) -> anyhow::Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), content)?;
        Ok(())
    }

    /// Expand ~ in a path to the actual home directory
    pub fn expand_path(path: &str) -> String {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped).to_string_lossy().to_string();
            }
        }
        path.to_string()
    }

    /// Get the local Interclaude directory (expanded)
    pub fn local_interclaude_dir(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(Self::expand_path(&self.local_dir))
    }

    /// Build SSH connection string
    pub fn ssh_destination(&self) -> String {
        if self.ssh_user.is_empty() {
            self.remote_host.clone()
        } else {
            format!("{}@{}", self.ssh_user, self.remote_host)
        }
    }

    /// Build SSH command base args
    pub fn ssh_args(&self) -> Vec<String> {
        let mut args = vec![
            "-F".to_string(), "/dev/null".to_string(), // skip ~/.ssh/config (avoids Colima/OrbStack unsupported options)
            "-o".to_string(), "StrictHostKeyChecking=accept-new".to_string(),
            "-p".to_string(), self.ssh_port.to_string(),
        ];
        let key = Self::expand_path(&self.key_path);
        if !key.is_empty() && std::path::Path::new(&key).exists() {
            args.extend(["-i".to_string(), key]);
        }
        args
    }

    pub fn get_field(&self, field: &crate::app::SetupField) -> String {
        match field {
            crate::app::SetupField::RemoteHost => self.remote_host.clone(),
            crate::app::SetupField::Connection => self.connection.label().to_string(),
            crate::app::SetupField::SshUser => self.ssh_user.clone(),
            crate::app::SetupField::SshPort => self.ssh_port.to_string(),
            crate::app::SetupField::KeyPath => self.key_path.clone(),
            crate::app::SetupField::RemoteDir => self.remote_dir.clone(),
            crate::app::SetupField::Transport => self.active_transport.label().to_string(),
            crate::app::SetupField::RedisHost => self.redis.host.clone(),
            crate::app::SetupField::RedisPort => self.redis.port.to_string(),
            crate::app::SetupField::RedisPassword => {
                if self.redis.password.is_empty() {
                    String::new()
                } else {
                    "*".repeat(self.redis.password.len())
                }
            }
        }
    }

    pub fn set_field(&mut self, field: &crate::app::SetupField, value: &str) {
        match field {
            crate::app::SetupField::RemoteHost => self.remote_host = value.to_string(),
            crate::app::SetupField::Connection => {} // handled by cycle
            crate::app::SetupField::SshUser => self.ssh_user = value.to_string(),
            crate::app::SetupField::SshPort => {
                if let Ok(port) = value.parse() {
                    self.ssh_port = port;
                }
            }
            crate::app::SetupField::KeyPath => self.key_path = value.to_string(),
            crate::app::SetupField::RemoteDir => self.remote_dir = value.to_string(),
            crate::app::SetupField::Transport => {} // handled by cycle
            crate::app::SetupField::RedisHost => self.redis.host = value.to_string(),
            crate::app::SetupField::RedisPort => {
                if let Ok(port) = value.parse() {
                    self.redis.port = port;
                }
            }
            crate::app::SetupField::RedisPassword => self.redis.password = value.to_string(),
        }
    }
}
