//! Media loading and processing
//!
//! Handles image loading, decoding, and scaling for wallpapers.

mod loader;
mod scaler;

pub use loader::ImageLoader;
pub use scaler::scale_image;

// Re-export ScaleMode from config for convenience
pub use crate::config::ScaleMode;
