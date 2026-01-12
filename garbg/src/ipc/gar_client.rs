//! Client for gar window manager's IPC

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Client for communicating with gar window manager
pub struct GarIpcClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl GarIpcClient {
    /// Connect to gar's IPC socket
    pub async fn connect() -> Result<Self> {
        let path = Self::socket_path()?;

        let stream = UnixStream::connect(&path)
            .await
            .with_context(|| format!("Failed to connect to gar at {}", path.display()))?;

        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);

        Ok(Self { reader, writer: write_half })
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
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;

        // Read the response (success or error)
        let mut line = String::new();
        self.reader.read_line(&mut line).await?;

        // Check for success
        let response: serde_json::Value = serde_json::from_str(&line)
            .context("Failed to parse subscribe response")?;

        if response.get("success") == Some(&serde_json::Value::Bool(true)) {
            Ok(())
        } else {
            let err = response.get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Subscribe failed: {}", err);
        }
    }

    /// Read the next event from gar
    pub async fn read_event(&mut self) -> Result<GarEvent> {
        let mut line = String::new();

        self.reader.read_line(&mut line).await?;

        if line.is_empty() {
            anyhow::bail!("Connection closed");
        }

        // gar uses { "event": "name", "data": {...} } format
        let raw: RawGarEvent = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse gar event: {}", line.trim()))?;

        Ok(raw.into())
    }
}

/// Raw event format from gar: { "event": "name", "data": {...} }
#[derive(Debug, Deserialize)]
struct RawGarEvent {
    event: String,
    #[serde(default)]
    data: serde_json::Value,
}

/// Events from gar window manager
#[derive(Debug, Clone)]
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
    Unknown,
}

impl From<RawGarEvent> for GarEvent {
    fn from(raw: RawGarEvent) -> Self {
        match raw.event.as_str() {
            "workspace" => {
                let current = raw.data.get("current")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as usize;
                let previous = raw.data.get("previous")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                GarEvent::Workspace { current, previous }
            }
            "monitor" => {
                let name = raw.data.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let action = raw.data.get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                GarEvent::Monitor { name, action }
            }
            "focus" => {
                let window_id = raw.data.get("window_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let workspace = raw.data.get("workspace")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as usize;
                GarEvent::Focus { window_id, workspace }
            }
            _ => GarEvent::Unknown,
        }
    }
}
