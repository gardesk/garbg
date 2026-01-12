//! Video decoding for video wallpapers
//!
//! Provides frame-by-frame video decoding using ffmpeg.
//! Requires the `video` feature and system ffmpeg libraries.

#![cfg(feature = "video")]

use anyhow::{Context, Result};
use ffmpeg_next as ffmpeg;
use ffmpeg_next::format::{input, Pixel};
use ffmpeg_next::media::Type;
use ffmpeg_next::software::scaling::{context::Context as ScalerContext, flag::Flags};
use ffmpeg_next::util::frame::video::Video as VideoFrame;
use image::RgbaImage;
use std::path::Path;
use std::sync::Once;
use std::time::Duration;

static FFMPEG_INIT: Once = Once::new();

/// Initialize ffmpeg (call once at startup)
pub fn init() -> Result<()> {
    let mut init_result = Ok(());
    FFMPEG_INIT.call_once(|| {
        if let Err(e) = ffmpeg::init() {
            init_result = Err(anyhow::anyhow!("Failed to initialize ffmpeg: {}", e));
        }
    });
    init_result
}

/// Information about a video file
#[derive(Debug, Clone)]
pub struct VideoInfo {
    /// Video width in pixels
    pub width: u32,
    /// Video height in pixels
    pub height: u32,
    /// Duration in seconds
    pub duration: f64,
    /// Frame rate (FPS)
    pub frame_rate: f64,
    /// Estimated total frames
    pub frame_count: usize,
    /// Video codec name
    pub codec: String,
}

/// A decoded video frame
pub struct DecodedFrame {
    /// Frame image data (RGBA)
    pub image: RgbaImage,
    /// Presentation timestamp (seconds from start)
    pub pts: f64,
    /// Frame index
    pub index: usize,
}

/// Video decoder for extracting frames
pub struct VideoDecoder {
    /// Input context
    input: ffmpeg::format::context::Input,
    /// Video stream index
    stream_index: usize,
    /// Decoder
    decoder: ffmpeg::decoder::Video,
    /// Scaler for format conversion
    scaler: ScalerContext,
    /// Video info
    info: VideoInfo,
    /// Current frame index
    frame_index: usize,
    /// Time base for PTS conversion
    time_base: f64,
}

impl VideoDecoder {
    /// Open a video file for decoding
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        init()?;

        let path = path.as_ref();
        let input = input(&path)
            .with_context(|| format!("Failed to open video: {}", path.display()))?;

        Self::from_input(input)
    }

    /// Open a video from bytes (writes to temp file)
    pub fn open_bytes(data: &[u8]) -> Result<Self> {
        init()?;

        // ffmpeg-next doesn't support reading from memory directly,
        // so we write to a temp file
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!("garbg_video_{}.mp4", std::process::id()));
        std::fs::write(&temp_path, data)
            .context("Failed to write video to temp file")?;

        let result = Self::open(&temp_path);

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        result
    }

    /// Create decoder from an open input context
    fn from_input(input: ffmpeg::format::context::Input) -> Result<Self> {
        // Find the best video stream
        let stream = input
            .streams()
            .best(Type::Video)
            .context("No video stream found")?;

        let stream_index = stream.index();
        let time_base = stream.time_base();

        // Get codec parameters
        let codec_params = stream.parameters();
        let codec = ffmpeg::codec::context::Context::from_parameters(codec_params)
            .context("Failed to create codec context")?;

        let decoder = codec.decoder().video()
            .context("Failed to create video decoder")?;

        let width = decoder.width();
        let height = decoder.height();

        // Create scaler to convert to RGBA
        let scaler = ScalerContext::get(
            decoder.format(),
            width,
            height,
            Pixel::RGBA,
            width,
            height,
            Flags::BILINEAR,
        ).context("Failed to create video scaler")?;

        // Calculate video info
        let duration = input.duration() as f64 / ffmpeg::ffi::AV_TIME_BASE as f64;
        let frame_rate = stream.avg_frame_rate();
        let fps = if frame_rate.denominator() != 0 {
            frame_rate.numerator() as f64 / frame_rate.denominator() as f64
        } else {
            30.0 // Default
        };

        let codec_name = decoder.codec()
            .map(|c| c.name().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let info = VideoInfo {
            width,
            height,
            duration,
            frame_rate: fps,
            frame_count: (duration * fps) as usize,
            codec: codec_name,
        };

        Ok(Self {
            input,
            stream_index,
            decoder,
            scaler,
            info,
            frame_index: 0,
            time_base: time_base.numerator() as f64 / time_base.denominator() as f64,
        })
    }

    /// Get video information
    pub fn info(&self) -> &VideoInfo {
        &self.info
    }

    /// Get frame delay based on frame rate
    pub fn frame_delay(&self) -> Duration {
        if self.info.frame_rate > 0.0 {
            Duration::from_secs_f64(1.0 / self.info.frame_rate)
        } else {
            Duration::from_millis(33) // ~30 FPS default
        }
    }

    /// Decode the next frame
    pub fn next_frame(&mut self) -> Result<Option<DecodedFrame>> {
        let mut decoded = VideoFrame::empty();

        // Read packets until we get a frame
        for (stream, packet) in self.input.packets() {
            if stream.index() != self.stream_index {
                continue;
            }

            self.decoder.send_packet(&packet)
                .context("Failed to send packet to decoder")?;

            while self.decoder.receive_frame(&mut decoded).is_ok() {
                // Convert to RGBA
                let mut rgb_frame = VideoFrame::empty();
                self.scaler.run(&decoded, &mut rgb_frame)
                    .context("Failed to scale frame")?;

                // Convert to RgbaImage
                let image = frame_to_image(&rgb_frame)?;
                let pts = decoded.pts().unwrap_or(0) as f64 * self.time_base;

                let frame = DecodedFrame {
                    image,
                    pts,
                    index: self.frame_index,
                };

                self.frame_index += 1;
                return Ok(Some(frame));
            }
        }

        // Flush the decoder
        self.decoder.send_eof()
            .context("Failed to flush decoder")?;

        while self.decoder.receive_frame(&mut decoded).is_ok() {
            let mut rgb_frame = VideoFrame::empty();
            self.scaler.run(&decoded, &mut rgb_frame)
                .context("Failed to scale frame")?;

            let image = frame_to_image(&rgb_frame)?;
            let pts = decoded.pts().unwrap_or(0) as f64 * self.time_base;

            let frame = DecodedFrame {
                image,
                pts,
                index: self.frame_index,
            };

            self.frame_index += 1;
            return Ok(Some(frame));
        }

        Ok(None)
    }

    /// Seek to a specific time (seconds)
    pub fn seek(&mut self, time_secs: f64) -> Result<()> {
        let timestamp = (time_secs / self.time_base) as i64;
        self.input.seek(timestamp, ..)
            .context("Failed to seek in video")?;
        self.decoder.flush();
        Ok(())
    }

    /// Reset to the beginning of the video
    pub fn reset(&mut self) -> Result<()> {
        self.seek(0.0)?;
        self.frame_index = 0;
        Ok(())
    }

    /// Get current frame index
    pub fn current_index(&self) -> usize {
        self.frame_index
    }
}

/// Convert an ffmpeg video frame to an image::RgbaImage
fn frame_to_image(frame: &VideoFrame) -> Result<RgbaImage> {
    let width = frame.width();
    let height = frame.height();
    let data = frame.data(0);
    let linesize = frame.stride(0);

    let mut pixels = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        let row_start = (y as usize) * linesize;
        let row_end = row_start + (width as usize * 4);
        pixels.extend_from_slice(&data[row_start..row_end]);
    }

    RgbaImage::from_raw(width, height, pixels)
        .context("Failed to create image from frame data")
}

/// Check if a file is a video (by extension)
pub fn is_video_file<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    matches!(ext.as_deref(), Some("mp4" | "webm" | "mkv" | "avi" | "mov" | "m4v"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_video_file() {
        assert!(is_video_file("test.mp4"));
        assert!(is_video_file("test.webm"));
        assert!(is_video_file("test.mkv"));
        assert!(!is_video_file("test.png"));
        assert!(!is_video_file("test.gif"));
    }
}
