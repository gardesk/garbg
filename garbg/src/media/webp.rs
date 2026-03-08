//! Animated WebP decoder with frame-by-frame access
//!
//! Provides frame extraction and timing information for animated WebP images.

use anyhow::{Context, Result};
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, Frame};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use super::gif::AnimationFrame;

/// Decoded animated WebP with all frames
pub struct AnimatedWebP {
    /// All frames in order
    frames: Vec<AnimationFrame>,
    /// Current frame index
    current_index: usize,
    /// Whether to loop forever
    pub loops: bool,
    /// Total duration of one loop
    pub total_duration: Duration,
}

impl AnimatedWebP {
    /// Load an animated WebP from a file path
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let data = fs::read(path)
            .with_context(|| format!("Failed to read WebP: {}", path.display()))?;

        Self::load_from_bytes(&data)
    }

    /// Load an animated WebP from bytes
    pub fn load_from_bytes(data: &[u8]) -> Result<Self> {
        let cursor = Cursor::new(data);
        let decoder = WebPDecoder::new(cursor)
            .context("Failed to create WebP decoder")?;

        // Check if this is an animated WebP
        if !decoder.has_animation() {
            anyhow::bail!("WebP is not animated");
        }

        let raw_frames = decoder.into_frames();
        let mut frames = Vec::new();
        let mut total_duration = Duration::ZERO;

        for frame_result in raw_frames {
            let frame: Frame = frame_result.context("Failed to decode WebP frame")?;
            let delay = frame_delay_to_duration(&frame);

            // WebP with 0 delay means "as fast as possible"
            // Default to 100ms (10 fps) for reasonable playback
            let delay = if delay.is_zero() {
                Duration::from_millis(100)
            } else {
                delay
            };

            total_duration += delay;

            frames.push(AnimationFrame {
                image: frame.into_buffer(),
                delay,
            });
        }

        if frames.is_empty() {
            anyhow::bail!("WebP contains no frames");
        }

        Ok(Self {
            frames,
            current_index: 0,
            loops: true,
            total_duration,
        })
    }

    /// Get the number of frames
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Check if this is actually animated (more than one frame)
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// Get the current frame
    pub fn current_frame(&self) -> &AnimationFrame {
        &self.frames[self.current_index]
    }

    /// Get a specific frame by index
    pub fn frame(&self, index: usize) -> Option<&AnimationFrame> {
        self.frames.get(index)
    }

    /// Get the current frame index
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Advance to the next frame, returning true if we looped
    pub fn advance(&mut self) -> bool {
        self.current_index += 1;
        if self.current_index >= self.frames.len() {
            self.current_index = 0;
            true // Looped
        } else {
            false
        }
    }

    /// Go back to the previous frame
    pub fn rewind(&mut self) -> bool {
        if self.current_index == 0 {
            self.current_index = self.frames.len() - 1;
            true // Looped
        } else {
            self.current_index -= 1;
            false
        }
    }

    /// Reset to the first frame
    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    /// Get all frames as a slice
    pub fn frames(&self) -> &[AnimationFrame] {
        &self.frames
    }

    /// Get average FPS
    pub fn average_fps(&self) -> f64 {
        if self.total_duration.is_zero() {
            return 0.0;
        }
        self.frames.len() as f64 / self.total_duration.as_secs_f64()
    }

    /// Get dimensions (width, height) from first frame
    pub fn dimensions(&self) -> (u32, u32) {
        let first = &self.frames[0].image;
        (first.width(), first.height())
    }
}

/// Convert frame delay ratio to Duration
fn frame_delay_to_duration(frame: &Frame) -> Duration {
    let (numerator, denominator) = frame.delay().numer_denom_ms();
    if denominator == 0 {
        Duration::ZERO
    } else {
        Duration::from_micros((numerator as u64 * 1000) / denominator as u64)
    }
}

/// Check if a file is an animated WebP
pub fn is_animated_webp<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref();

    // Quick extension check first
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    if ext.as_deref() != Some("webp") {
        return false;
    }

    // Try to load and check for animation
    match AnimatedWebP::load(path) {
        Ok(webp) => webp.is_animated(),
        Err(_) => false,
    }
}

/// Check if bytes represent an animated WebP
pub fn is_animated_webp_bytes(data: &[u8]) -> bool {
    match AnimatedWebP::load_from_bytes(data) {
        Ok(webp) => webp.is_animated(),
        Err(_) => false,
    }
}
