//! Image loading and format detection

use anyhow::{Context, Result};
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::fs;
use std::path::Path;

/// Image loader supporting multiple formats
pub struct ImageLoader;

impl ImageLoader {
    /// Load an image from a file path
    pub fn load_file<P: AsRef<Path>>(path: P) -> Result<RgbaImage> {
        let path = path.as_ref();
        let data = fs::read(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        Self::load_bytes(&data, Self::guess_format(path))
    }

    /// Load an image from bytes with optional format hint
    pub fn load_bytes(data: &[u8], format: Option<ImageFormat>) -> Result<RgbaImage> {
        let img = if let Some(fmt) = format {
            image::load_from_memory_with_format(data, fmt)
                .context("Failed to decode image with specified format")?
        } else {
            image::load_from_memory(data)
                .context("Failed to decode image")?
        };

        Ok(img.into_rgba8())
    }

    /// Guess image format from file extension
    fn guess_format(path: &Path) -> Option<ImageFormat> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_lowercase().as_str() {
                "png" => Some(ImageFormat::Png),
                "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
                "gif" => Some(ImageFormat::Gif),
                "webp" => Some(ImageFormat::WebP),
                "bmp" => Some(ImageFormat::Bmp),
                "tiff" | "tif" => Some(ImageFormat::Tiff),
                _ => None,
            })
    }

    /// Check if a path points to a supported image format
    pub fn is_supported_format(path: &Path) -> bool {
        Self::guess_format(path).is_some()
    }

    /// Get list of supported extensions
    pub fn supported_extensions() -> &'static [&'static str] {
        &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "tif"]
    }
}
