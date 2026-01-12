//! Playlist state management for slideshow functionality

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::ScaleMode;

/// Type of source for the playlist
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Local,
    GitHub,
    Http,
}

/// Persistent playlist state for slideshow navigation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistState {
    /// Original source path/URI
    pub source: String,

    /// Type of source
    pub source_type: SourceType,

    /// List of image paths/URLs in playlist order
    pub images: Vec<String>,

    /// Current position in the playlist
    pub current_index: usize,

    /// Whether the playlist was shuffled
    pub shuffled: bool,

    /// Scaling mode for wallpapers
    pub mode: ScaleMode,

    /// When the state was last updated
    pub last_updated: DateTime<Utc>,
}

impl PlaylistState {
    /// Get the path to the state file
    pub fn state_path() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("garbg")
            .join("state.json")
    }

    /// Create a new playlist state
    pub fn new(
        source: String,
        source_type: SourceType,
        images: Vec<String>,
        shuffled: bool,
        mode: ScaleMode,
    ) -> Self {
        Self {
            source,
            source_type,
            images,
            current_index: 0,
            shuffled,
            mode,
            last_updated: Utc::now(),
        }
    }

    /// Load state from disk, returns None if file doesn't exist
    pub fn load() -> Result<Option<Self>> {
        let path = Self::state_path();

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read state file: {}", path.display()))?;

        let state: Self = serde_json::from_str(&content)
            .with_context(|| "Failed to parse state file")?;

        Ok(Some(state))
    }

    /// Save state to disk
    pub fn save(&mut self) -> Result<()> {
        self.last_updated = Utc::now();

        let path = Self::state_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create cache directory: {}", parent.display()))?;
        }

        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize state")?;

        fs::write(&path, content)
            .with_context(|| format!("Failed to write state file: {}", path.display()))?;

        Ok(())
    }

    /// Reload state from disk (for when another process may have modified it)
    pub fn reload(&mut self) -> Result<()> {
        if let Some(loaded) = Self::load()? {
            *self = loaded;
        }
        Ok(())
    }

    /// Get the current image
    pub fn current(&self) -> Option<&str> {
        self.images.get(self.current_index).map(|s| s.as_str())
    }

    /// Advance to the next image, returns the new current image
    /// Re-shuffles on wrap-around if in shuffle mode
    pub fn next(&mut self) -> &str {
        if self.images.is_empty() {
            return "";
        }

        self.current_index += 1;

        // Wrap around
        if self.current_index >= self.images.len() {
            self.current_index = 0;

            // Re-shuffle on wrap if in shuffle mode
            if self.shuffled {
                self.reshuffle();
            }
        }

        &self.images[self.current_index]
    }

    /// Go to the previous image, returns the new current image
    pub fn prev(&mut self) -> &str {
        if self.images.is_empty() {
            return "";
        }

        // Wrap around
        if self.current_index == 0 {
            self.current_index = self.images.len() - 1;
        } else {
            self.current_index -= 1;
        }

        &self.images[self.current_index]
    }

    /// Shuffle the playlist (keeps current image but resets index to 0)
    pub fn reshuffle(&mut self) {
        let mut rng = rand::thread_rng();
        self.images.shuffle(&mut rng);
        // After reshuffle, we're at the start of a new random order
        // current_index is already 0 from wrap-around
    }

    /// Get total number of images
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Check if playlist is empty
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

/// Detect the source type from a URI/path
pub fn detect_source_type(source: &str) -> SourceType {
    if source.starts_with("github://") || source.contains("github.com") {
        SourceType::GitHub
    } else if source.starts_with("http://") || source.starts_with("https://") {
        SourceType::Http
    } else {
        SourceType::Local
    }
}
