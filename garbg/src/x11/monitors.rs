//! Multi-monitor detection via RandR

use anyhow::Result;

/// Represents a connected monitor/output
#[derive(Debug, Clone)]
pub struct Monitor {
    /// Output name (e.g., "DP-1", "HDMI-1")
    pub name: String,
    /// X position
    pub x: i16,
    /// Y position
    pub y: i16,
    /// Width in pixels
    pub width: u16,
    /// Height in pixels
    pub height: u16,
    /// Whether this is the primary monitor
    pub primary: bool,
}

impl Monitor {
    /// Get all connected monitors
    pub fn get_all(_conn: &super::Connection) -> Result<Vec<Monitor>> {
        // TODO: Implement RandR monitor detection
        // For now, return a single monitor covering the whole screen
        Ok(vec![])
    }
}
