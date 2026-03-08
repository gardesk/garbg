//! Media loading and processing
//!
//! Handles image loading, decoding, and scaling for wallpapers.

mod loader;
mod scaler;
mod gif;
mod webp;
mod apng;
mod frame_buffer;

#[cfg(feature = "video")]
mod video;

pub use loader::ImageLoader;
pub use scaler::{scale_image, scale_image_fast};
pub use gif::{AnimatedGif, AnimationFrame, is_animated_gif};
pub use webp::{AnimatedWebP, is_animated_webp, is_animated_webp_bytes};
pub use apng::{AnimatedPng, is_animated_png, is_animated_png_bytes};
pub use frame_buffer::{BufferedFrame, FrameRingBuffer, LoopingFrameBuffer, FrameBufferStats};

#[cfg(feature = "video")]
pub use video::{VideoDecoder, VideoInfo, DecodedFrame, is_video_file, init as init_video};

// Re-export ScaleMode from config for convenience
pub use crate::config::ScaleMode;
