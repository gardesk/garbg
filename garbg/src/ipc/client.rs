//! IPC client for communicating with the garbg daemon

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::protocol::{Command, Response};
use super::server::IpcServer;

/// Send a command to the daemon asynchronously
pub async fn send_command(cmd: &Command) -> Result<Response> {
    let socket_path = IpcServer::socket_path()?;

    let mut stream = UnixStream::connect(&socket_path)
        .await
        .with_context(|| format!("Failed to connect to daemon at {}", socket_path.display()))?;

    // Send command
    let json = serde_json::to_string(cmd)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;

    // Read response
    let mut reader = BufReader::new(&mut stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: Response = serde_json::from_str(&line)
        .context("Failed to parse response from daemon")?;

    Ok(response)
}

/// Send a command to the daemon (blocking)
pub fn send_command_blocking(cmd: &Command) -> Result<Response> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(send_command(cmd))
}

/// Check if the daemon is running by attempting to connect
pub fn is_daemon_running() -> bool {
    let socket_path = match IpcServer::socket_path() {
        Ok(p) => p,
        Err(_) => return false,
    };

    if !socket_path.exists() {
        return false;
    }

    // Try to connect to verify daemon is actually responding
    match std::os::unix::net::UnixStream::connect(&socket_path) {
        Ok(_) => true,
        Err(_) => {
            // Socket exists but can't connect - stale socket, clean it up
            let _ = std::fs::remove_file(&socket_path);
            false
        }
    }
}
