//! Daemon mode for garbg
//!
//! Runs as a background service managing wallpapers.

mod state;
mod animation_loop;
mod pid;

pub use state::{Daemon, DaemonState};
pub use animation_loop::{AnimationLoop, AnimationConfig, AnimationInfo};
pub use pid::{check_stale_pid, is_daemon_running_by_pid, pid_file_path, read_pid_file, remove_pid_file, write_pid_file};
