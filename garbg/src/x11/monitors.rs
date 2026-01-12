//! Multi-monitor detection via RandR

use anyhow::{Context, Result};
use x11rb::connection::Connection as X11Connection;
use x11rb::protocol::randr::{self, ConnectionExt as RandrExt};
use x11rb::protocol::xproto::Timestamp;

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
    /// Get all connected monitors via RandR
    pub fn get_all(conn: &super::Connection) -> Result<Vec<Monitor>> {
        let root = conn.root();
        let xconn = conn.conn();

        // Query RandR version to ensure it's available
        let version = xconn.randr_query_version(1, 5)?
            .reply()
            .context("RandR extension not available")?;

        tracing::debug!("RandR version: {}.{}", version.major_version, version.minor_version);

        // Get screen resources
        let resources = xconn.randr_get_screen_resources_current(root)?
            .reply()
            .context("Failed to get screen resources")?;

        // Get primary output (if any)
        let primary = xconn.randr_get_output_primary(root)?
            .reply()
            .context("Failed to get primary output")?
            .output;

        let mut monitors = Vec::new();

        // Process each output
        for &output in &resources.outputs {
            match Self::get_output_info(xconn, output, resources.config_timestamp, primary) {
                Ok(Some(monitor)) => {
                    tracing::debug!(
                        "Found monitor: {} ({}x{} at {},{}{})",
                        monitor.name,
                        monitor.width,
                        monitor.height,
                        monitor.x,
                        monitor.y,
                        if monitor.primary { ", primary" } else { "" }
                    );
                    monitors.push(monitor);
                }
                Ok(None) => {
                    // Output not connected or not active
                }
                Err(e) => {
                    tracing::warn!("Failed to get output info: {}", e);
                }
            }
        }

        // Sort monitors by x position (left to right)
        monitors.sort_by_key(|m| m.x);

        Ok(monitors)
    }

    /// Get info for a single output
    fn get_output_info<C: X11Connection>(
        conn: &C,
        output: randr::Output,
        timestamp: Timestamp,
        primary_output: randr::Output,
    ) -> Result<Option<Monitor>> {
        let output_info = conn.randr_get_output_info(output, timestamp)?
            .reply()
            .context("Failed to get output info")?;

        // Skip disconnected outputs
        if output_info.connection != randr::Connection::CONNECTED {
            return Ok(None);
        }

        // Skip outputs without a CRTC (not active)
        let crtc = output_info.crtc;
        if crtc == 0 {
            return Ok(None);
        }

        // Get CRTC info for position and dimensions
        let crtc_info = conn.randr_get_crtc_info(crtc, timestamp)?
            .reply()
            .context("Failed to get CRTC info")?;

        // Get output name
        let name = String::from_utf8_lossy(&output_info.name).to_string();

        Ok(Some(Monitor {
            name,
            x: crtc_info.x,
            y: crtc_info.y,
            width: crtc_info.width,
            height: crtc_info.height,
            primary: output == primary_output,
        }))
    }
}
