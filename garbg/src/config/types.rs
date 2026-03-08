//! Configuration type definitions

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// General settings
    pub general: GeneralConfig,

    /// Animation settings
    pub animation: AnimationConfig,

    /// Cache settings
    pub cache: CacheConfig,

    /// Default wallpaper source
    pub default: DefaultConfig,

    /// Per-workspace wallpaper configurations
    #[serde(default)]
    pub workspaces: Vec<WorkspaceConfig>,

    /// Per-monitor wallpaper configurations
    #[serde(default)]
    pub monitors: Vec<MonitorConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            animation: AnimationConfig::default(),
            cache: CacheConfig::default(),
            default: DefaultConfig::default(),
            workspaces: Vec::new(),
            monitors: Vec::new(),
        }
    }
}

/// General configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Default scaling mode
    pub mode: ScaleMode,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            mode: ScaleMode::Fill,
        }
    }
}

/// Animation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationConfig {
    /// Whether animations are enabled
    pub enabled: bool,

    /// Maximum FPS for animations
    pub max_fps: u32,

    /// Pause animations when idle/locked
    pub pause_on_idle: bool,

    /// Maximum memory budget for pre-scaled animation frames (in MB).
    /// Animations exceeding this budget will stream-scale frames on the fly.
    pub memory_budget_mb: u64,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_fps: 60,
            pause_on_idle: true,
            memory_budget_mb: 256,
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Cache directory (default: ~/.cache/garbg)
    pub directory: Option<String>,

    /// Maximum cache size in MB
    pub max_size_mb: u64,

    /// Maximum age of cached items in days
    pub max_age_days: u32,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            directory: None,
            max_size_mb: 1024,
            max_age_days: 30,
        }
    }
}

/// Default wallpaper configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DefaultConfig {
    /// Source path or URI
    pub source: String,

    /// Scaling mode
    pub mode: ScaleMode,

    /// Slideshow configuration
    pub slideshow: Option<SlideshowConfig>,
}

impl Default for DefaultConfig {
    fn default() -> Self {
        Self {
            source: String::new(),
            mode: ScaleMode::Fill,
            slideshow: None,
        }
    }
}

/// Per-workspace wallpaper configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Workspace ID (1-indexed)
    pub id: usize,

    /// Source path or URI
    pub source: String,

    /// Scaling mode (optional, uses default if not specified)
    #[serde(default)]
    pub mode: Option<ScaleMode>,
}

/// Per-monitor wallpaper configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Monitor name (RandR output name, e.g., "DP-1")
    pub name: String,

    /// Source path or URI
    pub source: String,

    /// Scaling mode (optional, uses default if not specified)
    #[serde(default)]
    pub mode: Option<ScaleMode>,

    /// Slideshow configuration (optional)
    #[serde(default)]
    pub slideshow: Option<SlideshowConfig>,
}

/// Slideshow configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SlideshowConfig {
    /// Whether slideshow is enabled
    pub enabled: bool,

    /// Interval between slides (e.g., "5m", "1h")
    #[serde(with = "humantime_serde")]
    pub interval: Duration,

    /// Shuffle order
    pub shuffle: bool,
}

impl Default for SlideshowConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: Duration::from_secs(300), // 5 minutes
            shuffle: true,
        }
    }
}

/// Image scaling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ScaleMode {
    /// Scale to fill the screen, cropping excess
    #[default]
    Fill,
    /// Scale to fit within the screen, letterboxing if needed
    Fit,
    /// Stretch to exact screen size, ignoring aspect ratio
    Stretch,
    /// Display at original size, centered
    Center,
    /// Tile the image to fill the screen
    Tile,
}

impl FromStr for ScaleMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fill" => Ok(ScaleMode::Fill),
            "fit" => Ok(ScaleMode::Fit),
            "stretch" => Ok(ScaleMode::Stretch),
            "center" => Ok(ScaleMode::Center),
            "tile" => Ok(ScaleMode::Tile),
            _ => anyhow::bail!("Unknown scale mode: {}. Valid modes: fill, fit, stretch, center, tile", s),
        }
    }
}

impl std::fmt::Display for ScaleMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScaleMode::Fill => write!(f, "fill"),
            ScaleMode::Fit => write!(f, "fit"),
            ScaleMode::Stretch => write!(f, "stretch"),
            ScaleMode::Center => write!(f, "center"),
            ScaleMode::Tile => write!(f, "tile"),
        }
    }
}

/// Serde helper for humantime durations
mod humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = humantime::format_duration(*duration).to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        humantime::parse_duration(&s).map_err(serde::de::Error::custom)
    }
}
