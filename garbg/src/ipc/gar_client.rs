//! Client for gar window manager's IPC

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Client for communicating with gar window manager
pub struct GarIpcClient {
    stream: UnixStream,
}

impl GarIpcClient {
    /// Connect to gar's IPC socket
    pub async fn connect() -> Result<Self> {
        let path = Self::socket_path()?;

        let stream = UnixStream::connect(&path)
            .await
            .with_context(|| format!("Failed to connect to gar at {}", path.display()))?;

        Ok(Self { stream })
    }

    /// Get gar's socket path
    fn socket_path() -> Result<PathBuf> {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| "/tmp".to_string());

        Ok(PathBuf::from(runtime_dir).join("gar.sock"))
    }

    /// Subscribe to gar events
    pub async fn subscribe(&mut self, events: &[&str]) -> Result<()> {
        let cmd = serde_json::json!({
            "command": "subscribe",
            "args": {
                "events": events
            }
        });

        let json = serde_json::to_string(&cmd)?;
        self.stream.write_all(json.as_bytes()).await?;
        self.stream.write_all(b"\n").await?;

        Ok(())
    }

    /// Read the next event from gar
    pub async fn read_event(&mut self) -> Result<GarEvent> {
        let mut reader = BufReader::new(&mut self.stream);
        let mut line = String::new();

        reader.read_line(&mut line).await?;

        let event: GarEvent = serde_json::from_str(&line)
            .context("Failed to parse gar event")?;

        Ok(event)
    }
}

/// Events from gar window manager
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GarEvent {
    /// Workspace changed
    Workspace {
        current: usize,
        previous: usize,
    },

    /// Monitor configuration changed
    Monitor {
        name: String,
        action: String, // "added", "removed", "changed"
    },

    /// Window focused
    Focus {
        window_id: u32,
        workspace: usize,
    },

    /// Unknown event (for forward compatibility)
    #[serde(other)]
    Unknown,
}
