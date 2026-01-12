//! High-level wallpaper rendering abstraction

use anyhow::Result;
use image::RgbaImage;

use super::Connection;

/// Wallpaper renderer using X11 pixmaps
pub struct Renderer {
    conn: Connection,
}

impl Renderer {
    /// Create a new renderer
    pub fn new() -> Result<Self> {
        let conn = Connection::new()?;
        Ok(Self { conn })
    }

    /// Set a static wallpaper
    pub fn set_wallpaper(&mut self, image: &RgbaImage) -> Result<()> {
        self.conn.set_wallpaper(image)
    }

    /// Get screen dimensions
    pub fn screen_dimensions(&self) -> (u16, u16) {
        self.conn.screen_dimensions()
    }
}
