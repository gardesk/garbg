//! IPC (Inter-Process Communication)
//!
//! Provides Unix socket-based communication for controlling garbg.

mod protocol;
pub mod server;
mod gar_client;
pub mod client;

pub use protocol::{Command, Response, Event};
pub use server::IpcServer;
pub use gar_client::{GarIpcClient, GarEvent};
pub use client::{send_command, send_command_blocking, is_daemon_running};
