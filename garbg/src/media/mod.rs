//! Media loading and processing
//!
//! Handles image loading, decoding, and scaling for wallpapers.

mod loader;
mod scaler;
mod gif;

pub use loader::ImageLoader;
pub use scaler::scale_image;
pub use gif::{AnimatedGif, AnimationFrame, is_animated_gif};

// Re-export ScaleMode from config for convenience
pub use crate::config::ScaleMode;
