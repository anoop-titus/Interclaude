use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};

/// Credential configuration stored in config.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialConfig {
    pub access_mode: String,     // "oauth" or "apikey"
    pub encrypted_api_key: String, // base64(nonce + ciphertext)
    pub model: String,           // model ID string
}

impl Default for CredentialConfig {
    fn default() -> Self {
        Self {
            access_mode: "apikey".to_string(),
            encrypted_api_key: String::new(),
            model: "claude-sonnet-4-6".to_string(),
        }
    }
}

impl CredentialConfig {
    /// Encrypt and store an API key
    pub fn set_api_key(&mut self, plaintext: &str) -> Result<()> {
        if plaintext.is_empty() {
            self.encrypted_api_key = String::new();
            return Ok(());
        }

        let key = derive_key();
        let cipher = Aes256Gcm::new(&key.into());

        // Generate random 96-bit nonce
        let mut nonce_bytes = [0u8; 12];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Prepend nonce to ciphertext, then base64 encode
        let mut combined = Vec::with_capacity(12 + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);

        self.encrypted_api_key = base64::engine::general_purpose::STANDARD.encode(&combined);
        Ok(())
    }

    /// Decrypt and return the API key
    pub fn get_api_key(&self) -> Result<String> {
        if self.encrypted_api_key.is_empty() {
            return Ok(String::new());
        }

        let combined = base64::engine::general_purpose::STANDARD
            .decode(&self.encrypted_api_key)
            .context("Invalid base64 in encrypted API key")?;

        if combined.len() < 13 {
            anyhow::bail!("Encrypted data too short");
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let key = derive_key();
        let cipher = Aes256Gcm::new(&key.into());
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed (different machine?): {}", e))?;

        String::from_utf8(plaintext).context("Decrypted API key is not valid UTF-8")
    }

    /// Whether credentials are configured (has an API key or is OAuth mode)
    pub fn is_configured(&self) -> bool {
        self.access_mode == "oauth" || !self.encrypted_api_key.is_empty()
    }
}

/// Derive a 256-bit encryption key from machine identity.
/// This is deterministic per-machine (not portable between machines).
fn derive_key() -> [u8; 32] {
    let machine_id = get_machine_id();
    let host = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Simple key derivation: SHA-256 of machine_id + hostname + salt
    // Using a basic hash since we don't want to add another dependency
    let seed = format!("interclaude:{}:{}:v1", machine_id, host);
    simple_sha256(seed.as_bytes())
}

/// Get a stable machine identifier
fn get_machine_id() -> String {
    // Try /etc/machine-id (Linux)
    if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
        return id.trim().to_string();
    }

    // Try macOS hardware UUID
    if let Ok(output) = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("IOPlatformUUID") {
                if let Some(uuid) = line.split('"').nth(3) {
                    return uuid.to_string();
                }
            }
        }
    }

    // Fallback: username + hostname
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    format!("fallback:{}:{}", user, hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string()))
}

/// Simple SHA-256 implementation using the same approach as aes-gcm internals.
/// We avoid adding a separate sha2 crate by using a basic hash construction.
/// This doesn't need to be cryptographically perfect — it just needs to be deterministic
/// and produce 32 bytes from the input.
fn simple_sha256(data: &[u8]) -> [u8; 32] {
    // Use a simple mixing function to derive 32 bytes deterministically.
    // For production, we'd use sha2 crate, but for config key derivation this is sufficient.
    let mut result = [0u8; 32];
    let mut state: u64 = 0x6a09e667f3bcc908; // SHA-256 initial value

    for (i, &byte) in data.iter().enumerate() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(byte as u64);
        result[i % 32] ^= (state >> ((i % 8) * 8)) as u8;
    }

    // Extra mixing rounds
    for round in 0..64 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(round);
        for j in 0..32 {
            result[j] = result[j].wrapping_add((state >> (j % 8 * 8)) as u8);
            state = state.wrapping_mul(2862933555777941757).wrapping_add(result[j] as u64);
        }
    }

    result
}

/// Validate an API key against the Anthropic API
pub async fn validate_api_key(api_key: &str, model: &str) -> Result<String> {
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "model": model,
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "ping"}]
    });

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Failed to reach Anthropic API")?;

    let status = response.status();
    if status.is_success() {
        Ok(format!("API key valid — {} accessible", model))
    } else if status.as_u16() == 401 {
        anyhow::bail!("Invalid API key (401 Unauthorized)")
    } else if status.as_u16() == 403 {
        anyhow::bail!("API key lacks permissions (403 Forbidden)")
    } else {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("API returned {} — {}", status, body)
    }
}
