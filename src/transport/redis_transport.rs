use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::bridge::message::Message;
use crate::config::Settings;
use crate::transport::{Transport, TransportKind};

/// Redis Pub/Sub transport for near-real-time messaging
pub struct RedisTransport {
    settings: Settings,
    inbox_dir: PathBuf,
    outbox_dir: PathBuf,
    session_id: String,
    /// Buffer of messages received via subscription
    received: Arc<Mutex<Vec<Message>>>,
    /// Whether the subscriber task is running
    subscriber_running: Arc<Mutex<bool>>,
}

impl RedisTransport {
    pub fn new(settings: &Settings, session_id: &str) -> Self {
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
            session_id: session_id.to_string(),
            received: Arc::new(Mutex::new(Vec::new())),
            subscriber_running: Arc::new(Mutex::new(false)),
        }
    }

    /// Channel name for this session's messages directed at us
    fn subscribe_channel(&self) -> String {
        let role = match self.settings.role {
            crate::config::Role::Master => "master",
            crate::config::Role::Slave => "slave",
        };
        format!("interclaude:{}:{}", self.session_id, role)
    }

    /// Channel name for sending messages to the other side
    fn publish_channel(&self) -> String {
        let target_role = match self.settings.role {
            crate::config::Role::Master => "slave",
            crate::config::Role::Slave => "master",
        };
        format!("interclaude:{}:{}", self.session_id, target_role)
    }

    /// Build Redis connection URL
    fn redis_url(&self) -> String {
        let redis = &self.settings.redis;
        if redis.password.is_empty() {
            format!("redis://{}:{}/", redis.host, redis.port)
        } else {
            format!("redis://:{}@{}:{}/", redis.password, redis.host, redis.port)
        }
    }

    /// Get a Redis client connection
    async fn get_client(&self) -> Result<redis::aio::MultiplexedConnection> {
        let client = redis::Client::open(self.redis_url())
            .context("Failed to create Redis client")?;
        let con = client.get_multiplexed_async_connection().await
            .context("Failed to connect to Redis")?;
        Ok(con)
    }

    /// Start the background subscriber task
    pub fn start_subscriber(&self) -> tokio::task::JoinHandle<()> {
        let url = self.redis_url();
        let channel = self.subscribe_channel();
        let received = self.received.clone();
        let running = self.subscriber_running.clone();
        let inbox_dir = self.inbox_dir.clone();

        tokio::spawn(async move {
            *running.lock().await = true;

            loop {
                match subscribe_loop(&url, &channel, &received, &inbox_dir).await {
                    Ok(()) => break, // clean exit
                    Err(e) => {
                        eprintln!("Redis subscriber error: {e}, reconnecting in 2s...");
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }

            *running.lock().await = false;
        })
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

async fn subscribe_loop(
    url: &str,
    channel: &str,
    received: &Arc<Mutex<Vec<Message>>>,
    inbox_dir: &PathBuf,
) -> Result<()> {
    let client = redis::Client::open(url)
        .context("Failed to create Redis client for subscription")?;
    let mut pubsub = client.get_async_pubsub().await
        .context("Failed to get Redis pubsub connection")?;

    pubsub.subscribe(channel).await
        .context("Failed to subscribe to Redis channel")?;

    use futures_lite::StreamExt;
    let mut stream = pubsub.on_message();

    while let Some(msg) = stream.next().await {
        let payload: String = match msg.get_payload() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Redis message payload error: {e}");
                continue;
            }
        };

        match serde_json::from_str::<Message>(&payload) {
            Ok(message) => {
                // Write to inbox for audit trail
                let filename = message.filename();
                let path = inbox_dir.join(&filename);
                if !path.exists() {
                    let _ = std::fs::write(&path, serde_json::to_string_pretty(&message).unwrap_or_default());
                }

                received.lock().await.push(message);
            }
            Err(e) => {
                eprintln!("Failed to parse Redis message: {e}");
            }
        }
    }

    Ok(())
}

impl Transport for RedisTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Redis
    }

    async fn send(&self, msg: &Message) -> Result<()> {
        // Write to outbox for audit trail
        self.write_audit(msg)?;

        // Publish to Redis
        let mut con = self.get_client().await?;
        let channel = self.publish_channel();
        let json = serde_json::to_string(msg)?;

        redis::cmd("PUBLISH")
            .arg(&channel)
            .arg(&json)
            .query_async::<i64>(&mut con)
            .await
            .context("Failed to publish message to Redis")?;

        Ok(())
    }

    async fn receive(&self) -> Result<Vec<Message>> {
        // Drain buffered messages from the subscriber
        let mut buffer = self.received.lock().await;
        let messages = std::mem::take(&mut *buffer);
        Ok(messages)
    }

    async fn health_check(&self) -> Result<bool> {
        match self.get_client().await {
            Ok(mut con) => {
                let pong: Result<String, _> = redis::cmd("PING")
                    .query_async(&mut con)
                    .await;
                Ok(pong.is_ok())
            }
            Err(_) => Ok(false),
        }
    }
}
