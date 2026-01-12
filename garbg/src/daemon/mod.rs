//! Daemon mode for garbg
//!
//! Runs as a background service managing wallpapers.

mod state;
mod animation_loop;

pub use state::{Daemon, DaemonState};
pub use animation_loop::{AnimationLoop, AnimationConfig, AnimationInfo};
