//! IPC protocol definitions

use serde::{Deserialize, Serialize};

use crate::config::ScaleMode;

/// Commands that can be sent to the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Set wallpaper
    Set {
        source: String,
        #[serde(default)]
        mode: Option<ScaleMode>,
        #[serde(default)]
        monitor: Option<String>,
        /// Slideshow interval in seconds (None = no auto-rotation)
        #[serde(default)]
        interval_secs: Option<u64>,
        /// Shuffle the playlist
        #[serde(default)]
        shuffle: bool,
    },

    /// Set wallpaper for a specific workspace
    SetWorkspace {
        workspace: usize,
        source: String,
        #[serde(default)]
        mode: Option<ScaleMode>,
    },

    /// Next wallpaper in slideshow
    Next {
        #[serde(default)]
        monitor: Option<String>,
    },

    /// Previous wallpaper in slideshow
    Prev {
        #[serde(default)]
        monitor: Option<String>,
    },

    /// Random wallpaper from current source
    Random {
        #[serde(default)]
        monitor: Option<String>,
    },

    /// Reload configuration
    Reload,

    /// Pause animations/slideshow
    Pause,

    /// Resume animations/slideshow
    Resume,

    /// Toggle pause state
    Toggle,

    /// Get current status
    Status,

    /// List wallpapers from a source
    List { source: String },

    /// Clear cache
    ClearCache,

    /// Subscribe to events
    Subscribe { events: Vec<String> },

    /// Unsubscribe from events
    Unsubscribe { events: Vec<String> },
}

/// Response to a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Whether the command succeeded
    pub success: bool,

    /// Response data (command-specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok() -> Self {
        Self {
            success: true,
            data: None,
            error: None,
        }
    }

    pub fn ok_with_data(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// Events sent to subscribed clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Wallpaper was changed
    WallpaperChanged {
        monitor: String,
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        workspace: Option<usize>,
    },

    /// Source was updated (new wallpapers available)
    SourceUpdated { source: String, count: usize },

    /// Animation state changed
    AnimationState { playing: bool },

    /// Slideshow advanced
    SlideshowAdvanced {
        current: usize,
        total: usize,
        source: String,
    },

    /// Error occurred
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
}
