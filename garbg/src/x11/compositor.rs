//! Multi-monitor wallpaper compositor
//!
//! Composites individual monitor wallpapers onto a single root window pixmap.

use image::RgbaImage;

use super::monitors::Monitor;
use crate::config::ScaleMode;
use crate::media::scale_image;

/// Represents a wallpaper for a specific monitor region
#[derive(Debug, Clone)]
pub struct MonitorWallpaper {
    /// Monitor this wallpaper is for
    pub monitor: Monitor,
    /// Scaled wallpaper image (sized to monitor dimensions)
    pub image: RgbaImage,
}

/// Compositor for combining multiple monitor wallpapers
pub struct Compositor {
    /// Total width of the root window
    pub total_width: u32,
    /// Total height of the root window
    pub total_height: u32,
}

impl Compositor {
    /// Create a new compositor for the given monitors
    pub fn new(monitors: &[Monitor]) -> Self {
        // Calculate bounding box that contains all monitors
        let (total_width, total_height) = Self::calculate_bounds(monitors);
        Self {
            total_width,
            total_height,
        }
    }

    /// Calculate the bounding box that contains all monitors
    fn calculate_bounds(monitors: &[Monitor]) -> (u32, u32) {
        if monitors.is_empty() {
            return (0, 0);
        }

        let mut max_x: i32 = 0;
        let mut max_y: i32 = 0;

        for m in monitors {
            let right = m.x as i32 + m.width as i32;
            let bottom = m.y as i32 + m.height as i32;
            max_x = max_x.max(right);
            max_y = max_y.max(bottom);
        }

        (max_x as u32, max_y as u32)
    }

    /// Composite multiple monitor wallpapers into a single image
    ///
    /// Returns a single RgbaImage that covers the entire root window,
    /// with each monitor's wallpaper placed at the correct position.
    pub fn composite(&self, wallpapers: &[MonitorWallpaper]) -> RgbaImage {
        // Create a blank canvas
        let mut canvas = RgbaImage::new(self.total_width, self.total_height);

        // Fill with black background (for any uncovered areas)
        for pixel in canvas.pixels_mut() {
            *pixel = image::Rgba([0, 0, 0, 255]);
        }

        // Composite each wallpaper at its monitor position
        for wp in wallpapers {
            let monitor = &wp.monitor;
            let image = &wp.image;

            // Copy pixels from wallpaper to canvas at monitor position
            for (x, y, pixel) in image.enumerate_pixels() {
                let canvas_x = monitor.x as u32 + x;
                let canvas_y = monitor.y as u32 + y;

                if canvas_x < self.total_width && canvas_y < self.total_height {
                    canvas.put_pixel(canvas_x, canvas_y, *pixel);
                }
            }
        }

        canvas
    }

    /// Create a wallpaper for a single monitor by scaling the source image
    pub fn create_monitor_wallpaper(
        monitor: &Monitor,
        source_image: &RgbaImage,
        mode: ScaleMode,
    ) -> MonitorWallpaper {
        let scaled = scale_image(
            source_image,
            monitor.width as u32,
            monitor.height as u32,
            mode,
        );

        MonitorWallpaper {
            monitor: monitor.clone(),
            image: scaled,
        }
    }

    /// Create wallpapers for all monitors using the same source image
    pub fn create_wallpapers_uniform(
        monitors: &[Monitor],
        source_image: &RgbaImage,
        mode: ScaleMode,
    ) -> Vec<MonitorWallpaper> {
        monitors
            .iter()
            .map(|m| Self::create_monitor_wallpaper(m, source_image, mode))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_monitor(name: &str, x: i16, y: i16, width: u16, height: u16) -> Monitor {
        Monitor {
            name: name.to_string(),
            x,
            y,
            width,
            height,
            primary: false,
        }
    }

    #[test]
    fn test_calculate_bounds_single() {
        let monitors = vec![make_monitor("DP-1", 0, 0, 1920, 1080)];
        let (w, h) = Compositor::calculate_bounds(&monitors);
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
    }

    #[test]
    fn test_calculate_bounds_dual_horizontal() {
        let monitors = vec![
            make_monitor("DP-1", 0, 0, 1920, 1080),
            make_monitor("DP-2", 1920, 0, 1920, 1080),
        ];
        let (w, h) = Compositor::calculate_bounds(&monitors);
        assert_eq!(w, 3840);
        assert_eq!(h, 1080);
    }

    #[test]
    fn test_calculate_bounds_dual_vertical() {
        let monitors = vec![
            make_monitor("DP-1", 0, 0, 1920, 1080),
            make_monitor("DP-2", 0, 1080, 1920, 1080),
        ];
        let (w, h) = Compositor::calculate_bounds(&monitors);
        assert_eq!(w, 1920);
        assert_eq!(h, 2160);
    }

    #[test]
    fn test_calculate_bounds_mixed() {
        let monitors = vec![
            make_monitor("DP-1", 0, 0, 2560, 1440),
            make_monitor("DP-2", 2560, 200, 1920, 1080),
        ];
        let (w, h) = Compositor::calculate_bounds(&monitors);
        assert_eq!(w, 4480);
        assert_eq!(h, 1440); // max(1440, 200+1080) = max(1440, 1280) = 1440
    }
}
