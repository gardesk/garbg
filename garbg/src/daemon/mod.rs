//! Daemon mode for garbg
//!
//! Runs as a background service managing wallpapers.

mod state;

pub use state::{Daemon, DaemonState};
