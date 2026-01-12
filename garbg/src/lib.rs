//! garbg - A bespoke wallpaper daemon for the gar window manager
//!
//! Features:
//! - Static images (PNG, JPEG, WebP, AVIF)
//! - Animated images (GIF, APNG, animated WebP)
//! - Video wallpapers (MP4, WebM) - optional
//! - Multiple image sources (local, HTTP, GitHub, S3)
//! - Per-workspace wallpapers
//! - Slideshow/rotation support

pub mod cache;
pub mod config;
pub mod daemon;
pub mod ipc;
pub mod media;
pub mod sources;
pub mod state;
pub mod x11;

pub use config::Config;
pub use daemon::Daemon;

/// Result type alias using anyhow for error handling
pub type Result<T> = anyhow::Result<T>;
