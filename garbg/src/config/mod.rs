//! Configuration management
//!
//! Handles TOML configuration files and runtime settings.

mod types;

pub use types::{Config, ScaleMode, WorkspaceConfig, MonitorConfig, SlideshowConfig};

use anyhow::{Context, Result};
use std::path::Path;

impl Config {
    /// Load configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }

    /// Load configuration from the default location
    pub fn load_default() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .context("Could not determine config directory")?
            .join("garbg");

        let config_path = config_dir.join("config.toml");

        if config_path.exists() {
            Self::load(&config_path)
        } else {
            Ok(Self::default())
        }
    }

    /// Get the default config file path
    pub fn default_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("garbg").join("config.toml"))
    }
}
