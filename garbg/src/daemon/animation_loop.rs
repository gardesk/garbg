//! Animation playback loop for GIF wallpapers
//!
//! Manages frame timing and rendering for animated wallpapers.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::ScaleMode;
use crate::media::{scale_image_fast, AnimatedGif};
use crate::x11::{AnimationRenderer, Connection};

/// Default memory budget for pre-scaled animation frames (256 MB)
const DEFAULT_MEMORY_BUDGET: u64 = 256 * 1024 * 1024;

/// Configuration for the animation loop
#[derive(Debug, Clone)]
pub struct AnimationConfig {
    /// Maximum FPS (0 = unlimited, capped at source FPS)
    pub max_fps: u32,
    /// Whether to skip frames when running behind
    pub adaptive_skip: bool,
    /// Scaling mode for frames
    pub scale_mode: ScaleMode,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            max_fps: 60,
            adaptive_skip: true,
            scale_mode: ScaleMode::Fill,
        }
    }
}

/// Animation loop state
pub struct AnimationLoop {
    /// The animated GIF being played
    gif: AnimatedGif,
    /// X11 animation renderer
    renderer: AnimationRenderer,
    /// Pre-scaled frames (empty if streaming mode)
    scaled_frames: Vec<image::RgbaImage>,
    /// Whether to scale frames on-the-fly instead of pre-scaling
    streaming: bool,
    /// Screen dimensions (used for on-the-fly scaling)
    screen_width: u32,
    screen_height: u32,
    /// Configuration
    config: AnimationConfig,
    /// Whether the animation is paused
    paused: Arc<AtomicBool>,
    /// Whether to stop the animation
    stop: Arc<AtomicBool>,
}

impl AnimationLoop {
    /// Create a new animation loop for a GIF
    pub fn new(
        gif: AnimatedGif,
        conn: &Connection,
        config: AnimationConfig,
    ) -> Result<Self> {
        let renderer = AnimationRenderer::new(conn)?;
        let (screen_width, screen_height) = renderer.dimensions();
        let sw = screen_width as u32;
        let sh = screen_height as u32;

        let per_frame_bytes = sw as u64 * sh as u64 * 4;
        let total_bytes = per_frame_bytes * gif.frame_count() as u64;

        let (scaled_frames, streaming) = if total_bytes <= DEFAULT_MEMORY_BUDGET {
            tracing::info!(
                "Animation fits in memory budget ({:.1} MB), pre-scaling all {} frames",
                total_bytes as f64 / (1024.0 * 1024.0),
                gif.frame_count(),
            );
            let frames: Vec<image::RgbaImage> = gif
                .frames()
                .iter()
                .map(|frame| {
                    scale_image_fast(&frame.image, sw, sh, config.scale_mode)
                })
                .collect();
            (frames, false)
        } else {
            tracing::info!(
                "Animation exceeds memory budget ({:.1} MB > {:.1} MB), scaling frames on-the-fly",
                total_bytes as f64 / (1024.0 * 1024.0),
                DEFAULT_MEMORY_BUDGET as f64 / (1024.0 * 1024.0),
            );
            (Vec::new(), true)
        };

        Ok(Self {
            gif,
            renderer,
            scaled_frames,
            streaming,
            screen_width: sw,
            screen_height: sh,
            config,
            paused: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Load a GIF from a file and create an animation loop
    pub fn from_file(
        path: &str,
        conn: &Connection,
        config: AnimationConfig,
    ) -> Result<Self> {
        let gif = AnimatedGif::load(path)?;
        Self::new(gif, conn, config)
    }

    /// Get a handle to pause/resume the animation
    pub fn pause_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.paused)
    }

    /// Get a handle to stop the animation
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }

    /// Run the animation loop (blocking)
    ///
    /// This will loop forever until `stop` is set to true.
    pub fn run(&mut self, conn: &mut Connection) -> Result<()> {
        if self.scaled_frames.is_empty() {
            anyhow::bail!("No frames to display");
        }

        // Calculate minimum frame duration based on max_fps
        let min_frame_duration = if self.config.max_fps > 0 {
            Duration::from_secs_f64(1.0 / self.config.max_fps as f64)
        } else {
            Duration::ZERO
        };

        let mut frame_index = 0;
        let mut next_frame_time = Instant::now();
        let mut frames_skipped = 0u64;

        tracing::info!(
            "Starting animation: {} frames, {:.1} FPS avg",
            self.scaled_frames.len(),
            self.gif.average_fps()
        );

        loop {
            // Check for stop signal
            if self.stop.load(Ordering::Relaxed) {
                tracing::info!("Animation stopped");
                break;
            }

            // Handle pause
            if self.paused.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
                next_frame_time = Instant::now();
                continue;
            }

            let now = Instant::now();

            // Check if we're behind schedule
            if self.config.adaptive_skip && now > next_frame_time {
                let behind = now - next_frame_time;

                // Skip frames if we're more than one frame behind
                while next_frame_time < now {
                    let frame_delay = self.gif.frames()[frame_index].delay;
                    next_frame_time += frame_delay.max(min_frame_duration);
                    frame_index = (frame_index + 1) % self.scaled_frames.len();
                    frames_skipped += 1;
                }

                if frames_skipped > 0 && frames_skipped % 100 == 0 {
                    tracing::debug!(
                        "Skipped {} frames total ({}ms behind)",
                        frames_skipped,
                        behind.as_millis()
                    );
                }
            }

            // Render current frame
            if self.streaming {
                let scaled = scale_image_fast(
                    &self.gif.frames()[frame_index].image,
                    self.screen_width,
                    self.screen_height,
                    self.config.scale_mode,
                );
                self.renderer.render_and_present(conn, &scaled)?;
            } else {
                let scaled_frame = &self.scaled_frames[frame_index];
                self.renderer.render_and_present(conn, scaled_frame)?;
            }

            // Get delay for current frame
            let frame_delay = self.gif.frames()[frame_index].delay;
            let actual_delay = frame_delay.max(min_frame_duration);

            // Advance to next frame
            frame_index = (frame_index + 1) % self.scaled_frames.len();
            next_frame_time += actual_delay;

            // Sleep until next frame
            let sleep_time = next_frame_time.saturating_duration_since(Instant::now());
            if !sleep_time.is_zero() {
                std::thread::sleep(sleep_time);
            }
        }

        Ok(())
    }

    /// Run a single iteration (non-blocking, for integration with async loops)
    ///
    /// Returns the duration to wait before the next frame.
    pub fn tick(&mut self, conn: &mut Connection) -> Result<Duration> {
        if self.scaled_frames.is_empty() {
            return Ok(Duration::from_millis(100));
        }

        if self.paused.load(Ordering::Relaxed) {
            return Ok(Duration::from_millis(50));
        }

        let frame_index = self.gif.current_index();

        // Render current frame
        if self.streaming {
            let scaled = scale_image_fast(
                &self.gif.frames()[frame_index].image,
                self.screen_width,
                self.screen_height,
                self.config.scale_mode,
            );
            self.renderer.render_and_present(conn, &scaled)?;
        } else {
            let scaled_frame = &self.scaled_frames[frame_index];
            self.renderer.render_and_present(conn, scaled_frame)?;
        }

        // Get delay for current frame
        let delay = self.gif.current_frame().delay;

        // Advance to next frame
        self.gif.advance();

        // Cap at max_fps
        let min_delay = if self.config.max_fps > 0 {
            Duration::from_secs_f64(1.0 / self.config.max_fps as f64)
        } else {
            Duration::ZERO
        };

        Ok(delay.max(min_delay))
    }

    /// Get animation info
    pub fn info(&self) -> AnimationInfo {
        AnimationInfo {
            frame_count: self.gif.frame_count(),
            current_frame: self.gif.current_index(),
            total_duration: self.gif.total_duration,
            average_fps: self.gif.average_fps(),
            dimensions: self.gif.dimensions(),
            paused: self.paused.load(Ordering::Relaxed),
        }
    }

    /// Clean up resources
    pub fn destroy(self, conn: &Connection) {
        self.renderer.destroy(conn);
    }
}

/// Information about the current animation
#[derive(Debug, Clone)]
pub struct AnimationInfo {
    pub frame_count: usize,
    pub current_frame: usize,
    pub total_duration: Duration,
    pub average_fps: f64,
    pub dimensions: (u32, u32),
    pub paused: bool,
}
