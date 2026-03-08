//! Image scaling with various modes

use image::{imageops::FilterType, RgbaImage};

use crate::config::ScaleMode;

/// Scale an image according to the specified mode (high quality, Lanczos3)
pub fn scale_image(
    image: &RgbaImage,
    target_width: u32,
    target_height: u32,
    mode: ScaleMode,
) -> RgbaImage {
    scale_image_inner(image, target_width, target_height, mode, FilterType::Lanczos3)
}

/// Scale an image using a fast filter (Triangle) — suited for animation frames
pub fn scale_image_fast(
    image: &RgbaImage,
    target_width: u32,
    target_height: u32,
    mode: ScaleMode,
) -> RgbaImage {
    scale_image_inner(image, target_width, target_height, mode, FilterType::Triangle)
}

fn scale_image_inner(
    image: &RgbaImage,
    target_width: u32,
    target_height: u32,
    mode: ScaleMode,
    filter: FilterType,
) -> RgbaImage {
    match mode {
        ScaleMode::Fill => scale_fill(image, target_width, target_height, filter),
        ScaleMode::Fit => scale_fit(image, target_width, target_height, filter),
        ScaleMode::Stretch => scale_stretch(image, target_width, target_height, filter),
        ScaleMode::Center => scale_center(image, target_width, target_height),
        ScaleMode::Tile => scale_tile(image, target_width, target_height),
    }
}

/// Fill: Scale to cover entire area, crop excess
fn scale_fill(image: &RgbaImage, target_width: u32, target_height: u32, filter: FilterType) -> RgbaImage {
    let (src_width, src_height) = image.dimensions();

    // Calculate scale factor to cover the entire target
    let scale_x = target_width as f64 / src_width as f64;
    let scale_y = target_height as f64 / src_height as f64;
    let scale = scale_x.max(scale_y);

    let scaled_width = (src_width as f64 * scale).round() as u32;
    let scaled_height = (src_height as f64 * scale).round() as u32;

    // Scale image
    let scaled = image::imageops::resize(image, scaled_width, scaled_height, filter);

    // Crop to target size (center crop)
    let crop_x = (scaled_width.saturating_sub(target_width)) / 2;
    let crop_y = (scaled_height.saturating_sub(target_height)) / 2;

    image::imageops::crop_imm(&scaled, crop_x, crop_y, target_width, target_height).to_image()
}

/// Fit: Scale to fit within area, letterbox if needed
fn scale_fit(image: &RgbaImage, target_width: u32, target_height: u32, filter: FilterType) -> RgbaImage {
    let (src_width, src_height) = image.dimensions();

    // Calculate scale factor to fit within target
    let scale_x = target_width as f64 / src_width as f64;
    let scale_y = target_height as f64 / src_height as f64;
    let scale = scale_x.min(scale_y);

    let scaled_width = (src_width as f64 * scale).round() as u32;
    let scaled_height = (src_height as f64 * scale).round() as u32;

    // Scale image
    let scaled = image::imageops::resize(image, scaled_width, scaled_height, filter);

    // Create output with black background
    let mut output = RgbaImage::from_pixel(target_width, target_height, image::Rgba([0, 0, 0, 255]));

    // Center the scaled image
    let offset_x = (target_width.saturating_sub(scaled_width)) / 2;
    let offset_y = (target_height.saturating_sub(scaled_height)) / 2;

    image::imageops::overlay(&mut output, &scaled, offset_x as i64, offset_y as i64);

    output
}

/// Stretch: Scale to exact target size, ignoring aspect ratio
fn scale_stretch(image: &RgbaImage, target_width: u32, target_height: u32, filter: FilterType) -> RgbaImage {
    image::imageops::resize(image, target_width, target_height, filter)
}

/// Center: Display at original size, centered
fn scale_center(image: &RgbaImage, target_width: u32, target_height: u32) -> RgbaImage {
    let (src_width, src_height) = image.dimensions();

    // Create output with black background
    let mut output = RgbaImage::from_pixel(target_width, target_height, image::Rgba([0, 0, 0, 255]));

    // Calculate offset to center
    let offset_x = (target_width as i64 - src_width as i64) / 2;
    let offset_y = (target_height as i64 - src_height as i64) / 2;

    // If image is larger than target, we need to crop it
    if offset_x < 0 || offset_y < 0 {
        let crop_x = (-offset_x).max(0) as u32;
        let crop_y = (-offset_y).max(0) as u32;
        let crop_width = src_width.min(target_width);
        let crop_height = src_height.min(target_height);

        let cropped = image::imageops::crop_imm(image, crop_x, crop_y, crop_width, crop_height);

        let paste_x = offset_x.max(0);
        let paste_y = offset_y.max(0);

        image::imageops::overlay(&mut output, &cropped.to_image(), paste_x, paste_y);
    } else {
        image::imageops::overlay(&mut output, image, offset_x, offset_y);
    }

    output
}

/// Tile: Repeat image to fill area
fn scale_tile(image: &RgbaImage, target_width: u32, target_height: u32) -> RgbaImage {
    let (src_width, src_height) = image.dimensions();

    let mut output = RgbaImage::new(target_width, target_height);

    let tiles_x = (target_width + src_width - 1) / src_width;
    let tiles_y = (target_height + src_height - 1) / src_height;

    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            let x = (tx * src_width) as i64;
            let y = (ty * src_height) as i64;
            image::imageops::overlay(&mut output, image, x, y);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]))
    }

    #[test]
    fn test_scale_stretch() {
        let img = test_image(100, 100);
        let scaled = scale_stretch(&img, 200, 150, FilterType::Lanczos3);
        assert_eq!(scaled.dimensions(), (200, 150));
    }

    #[test]
    fn test_scale_fill() {
        let img = test_image(100, 100);
        let scaled = scale_fill(&img, 200, 150, FilterType::Lanczos3);
        assert_eq!(scaled.dimensions(), (200, 150));
    }

    #[test]
    fn test_scale_fit() {
        let img = test_image(100, 100);
        let scaled = scale_fit(&img, 200, 150, FilterType::Lanczos3);
        assert_eq!(scaled.dimensions(), (200, 150));
    }

    #[test]
    fn test_scale_center() {
        let img = test_image(100, 100);
        let scaled = scale_center(&img, 200, 200);
        assert_eq!(scaled.dimensions(), (200, 200));
    }

    #[test]
    fn test_scale_tile() {
        let img = test_image(100, 100);
        let scaled = scale_tile(&img, 250, 250);
        assert_eq!(scaled.dimensions(), (250, 250));
    }
}
