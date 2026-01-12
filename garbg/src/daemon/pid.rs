//! PID file management for daemon lifecycle

use anyhow::{Context, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process;

/// Get the path to the PID file
pub fn pid_file_path() -> Result<PathBuf> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/tmp".to_string());
    Ok(PathBuf::from(runtime_dir).join("garbg.pid"))
}

/// Write the current process PID to the PID file
pub fn write_pid_file() -> Result<()> {
    let path = pid_file_path()?;
    let pid = process::id();

    let mut file = fs::File::create(&path)
        .with_context(|| format!("Failed to create PID file: {}", path.display()))?;

    write!(file, "{}", pid)
        .with_context(|| format!("Failed to write PID to: {}", path.display()))?;

    tracing::debug!("PID file written: {} (pid: {})", path.display(), pid);
    Ok(())
}

/// Remove the PID file (for graceful shutdown)
pub fn remove_pid_file() -> Result<()> {
    let path = pid_file_path()?;

    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove PID file: {}", path.display()))?;
        tracing::debug!("PID file removed: {}", path.display());
    }

    Ok(())
}

/// Read the PID from the PID file, if it exists
pub fn read_pid_file() -> Result<Option<u32>> {
    let path = pid_file_path()?;

    if !path.exists() {
        return Ok(None);
    }

    let mut file = fs::File::open(&path)
        .with_context(|| format!("Failed to open PID file: {}", path.display()))?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .with_context(|| format!("Failed to read PID file: {}", path.display()))?;

    let pid = contents.trim().parse::<u32>()
        .with_context(|| format!("Invalid PID in file: {}", contents.trim()))?;

    Ok(Some(pid))
}

/// Check if a process with the given PID is running
fn is_process_running(pid: u32) -> bool {
    // On Unix, we can check if a process exists by sending signal 0
    #[cfg(unix)]
    {
        use std::ffi::c_int;
        extern "C" {
            fn kill(pid: c_int, sig: c_int) -> c_int;
        }
        // Signal 0 doesn't send a signal, just checks if process exists
        unsafe { kill(pid as c_int, 0) == 0 }
    }

    #[cfg(not(unix))]
    {
        // On non-Unix, assume process is running if we can't check
        true
    }
}

/// Check for stale PID file and clean up if necessary
///
/// Returns:
/// - Ok(None) if no PID file exists
/// - Ok(Some(pid)) if a daemon is already running
/// - Removes stale file and returns Ok(None) if daemon is not running
pub fn check_stale_pid() -> Result<Option<u32>> {
    let pid = match read_pid_file()? {
        Some(pid) => pid,
        None => return Ok(None),
    };

    // Check if the process is actually running
    if is_process_running(pid) {
        // Daemon is already running
        tracing::debug!("Found running daemon with PID: {}", pid);
        return Ok(Some(pid));
    }

    // Stale PID file - daemon crashed or was killed without cleanup
    let path = pid_file_path()?;
    tracing::info!("Cleaning up stale PID file (pid {} not running)", pid);

    fs::remove_file(&path)
        .with_context(|| format!("Failed to remove stale PID file: {}", path.display()))?;

    Ok(None)
}

/// Check if another daemon instance is already running
pub fn is_daemon_running_by_pid() -> Result<bool> {
    Ok(check_stale_pid()?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_file_path() {
        let path = pid_file_path().unwrap();
        assert!(path.to_string_lossy().ends_with("garbg.pid"));
    }

    #[test]
    fn test_is_process_running() {
        // Our own process should be running
        assert!(is_process_running(process::id()));
        // Very high PID probably doesn't exist
        assert!(!is_process_running(999999999));
    }
}
