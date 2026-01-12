//! IPC server for garbg daemon

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use super::protocol::{Command, Event, Response};

/// IPC server for accepting client connections
pub struct IpcServer {
    listener: UnixListener,
    socket_path: PathBuf,
}

impl IpcServer {
    /// Create a new IPC server
    pub async fn new() -> Result<Self> {
        let socket_path = Self::socket_path()?;

        // Remove existing socket if present
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        // Create parent directory if needed
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("Failed to bind to {}", socket_path.display()))?;

        // Set permissions to user-only (0600)
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;

        Ok(Self {
            listener,
            socket_path,
        })
    }

    /// Get the socket path
    pub fn socket_path() -> Result<PathBuf> {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| "/tmp".to_string());

        Ok(PathBuf::from(runtime_dir).join("garbg.sock"))
    }

    /// Accept a new client connection
    pub async fn accept(&self) -> Result<UnixStream> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream)
    }

    /// Get the path this server is bound to
    pub fn path(&self) -> &PathBuf {
        &self.socket_path
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        // Clean up socket file
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Handle a single client connection
pub struct IpcClient {
    stream: UnixStream,
    subscriptions: HashSet<String>,
}

impl IpcClient {
    pub fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            subscriptions: HashSet::new(),
        }
    }

    /// Read a command from the client
    pub async fn read_command(&mut self) -> Result<Option<Command>> {
        let mut reader = BufReader::new(&mut self.stream);
        let mut line = String::new();

        match reader.read_line(&mut line).await {
            Ok(0) => Ok(None), // EOF
            Ok(_) => {
                let cmd: Command = serde_json::from_str(&line)
                    .context("Failed to parse command")?;
                Ok(Some(cmd))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Send a response to the client
    pub async fn send_response(&mut self, response: &Response) -> Result<()> {
        let json = serde_json::to_string(response)?;
        self.stream.write_all(json.as_bytes()).await?;
        self.stream.write_all(b"\n").await?;
        Ok(())
    }

    /// Send an event to the client
    pub async fn send_event(&mut self, event: &Event) -> Result<()> {
        let json = serde_json::to_string(event)?;
        self.stream.write_all(json.as_bytes()).await?;
        self.stream.write_all(b"\n").await?;
        Ok(())
    }

    /// Subscribe to event types
    pub fn subscribe(&mut self, events: &[String]) {
        self.subscriptions.extend(events.iter().cloned());
    }

    /// Unsubscribe from event types
    pub fn unsubscribe(&mut self, events: &[String]) {
        for event in events {
            self.subscriptions.remove(event);
        }
    }

    /// Check if subscribed to an event type
    pub fn is_subscribed(&self, event_type: &str) -> bool {
        self.subscriptions.contains(event_type) || self.subscriptions.contains("*")
    }
}
