//! Animation support with double buffering
//!
//! Provides smooth animation playback using double-buffered pixmaps.

use anyhow::Result;
use x11rb::protocol::xproto::*;
use x11rb::connection::Connection as X11Connection;
use x11rb::wrapper::ConnectionExt as _;

use super::Connection;

/// Double buffer for smooth animation rendering
pub struct DoubleBuffer {
    /// Front buffer (currently displayed)
    front: Pixmap,
    /// Back buffer (being rendered to)
    back: Pixmap,
    /// Screen dimensions
    width: u16,
    height: u16,
}

impl DoubleBuffer {
    /// Create a new double buffer with the given dimensions
    pub fn new(conn: &Connection) -> Result<Self> {
        let (width, height) = conn.screen_dimensions();
        let depth = conn.depth();
        let root = conn.root();
        let x11_conn = conn.conn();

        // Create front buffer
        let front = x11_conn.generate_id()?;
        x11_conn.create_pixmap(depth, front, root, width, height)?;

        // Create back buffer
        let back = x11_conn.generate_id()?;
        x11_conn.create_pixmap(depth, back, root, width, height)?;

        Ok(Self {
            front,
            back,
            width,
            height,
        })
    }

    /// Get the back buffer to render to
    pub fn back_buffer(&self) -> Pixmap {
        self.back
    }

    /// Get the front buffer (currently displayed)
    pub fn front_buffer(&self) -> Pixmap {
        self.front
    }

    /// Swap front and back buffers
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.front, &mut self.back);
    }

    /// Get dimensions
    pub fn dimensions(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Free the pixmaps
    pub fn destroy(&self, conn: &Connection) {
        let x11_conn = conn.conn();
        let _ = x11_conn.free_pixmap(self.front);
        let _ = x11_conn.free_pixmap(self.back);
    }
}

/// Animation renderer using double buffering
pub struct AnimationRenderer {
    /// Double buffer for smooth rendering
    buffer: DoubleBuffer,
    /// Reusable BGRA conversion buffer (avoids per-frame allocation)
    bgra_buf: Vec<u8>,
}

impl AnimationRenderer {
    /// Create a new animation renderer
    pub fn new(conn: &Connection) -> Result<Self> {
        let buffer = DoubleBuffer::new(conn)?;
        Ok(Self { buffer, bgra_buf: Vec::new() })
    }

    /// Render a frame to the back buffer
    pub fn render_frame(&mut self, conn: &mut Connection, frame: &image::RgbaImage) -> Result<()> {
        let (width, height) = self.buffer.dimensions();

        // Convert RGBA to BGRA in-place using reusable buffer
        rgba_to_bgra_into(frame, &mut self.bgra_buf);
        let bgra_data = &self.bgra_buf;

        let gc = conn.gc();
        let x11_conn = conn.conn();
        let depth = conn.depth();
        let back = self.buffer.back_buffer();

        // Calculate chunking for large images
        let max_request_bytes = x11_conn.setup().maximum_request_length as usize * 4;
        let bytes_per_row = width as usize * 4;
        let request_overhead = 28;
        let max_rows_per_request = ((max_request_bytes - request_overhead) / bytes_per_row).max(1) as u16;

        // Send image in chunks
        let mut y_offset: u16 = 0;
        while y_offset < height {
            let rows_to_send = (height - y_offset).min(max_rows_per_request);
            let start_byte = y_offset as usize * bytes_per_row;
            let end_byte = (y_offset as usize + rows_to_send as usize) * bytes_per_row;
            let chunk = &bgra_data[start_byte..end_byte];

            x11_conn.put_image(
                ImageFormat::Z_PIXMAP,
                back,
                gc,
                width,
                rows_to_send,
                0,
                y_offset as i16,
                0,
                depth,
                chunk,
            )?;

            y_offset += rows_to_send;
        }

        Ok(())
    }

    /// Present the back buffer (swap and display)
    pub fn present(&mut self, conn: &mut Connection) -> Result<()> {
        // Swap buffers
        self.buffer.swap();

        // Set the new front buffer as root background
        let front = self.buffer.front_buffer();
        let root = conn.root();
        let x11_conn = conn.conn();
        let atoms = conn.atoms();

        // Set the standard atoms for compatibility
        x11_conn.change_property32(
            PropMode::REPLACE,
            root,
            atoms.xrootpmap_id,
            AtomEnum::PIXMAP,
            &[front],
        )?;

        x11_conn.change_property32(
            PropMode::REPLACE,
            root,
            atoms.esetroot_pmap_id,
            AtomEnum::PIXMAP,
            &[front],
        )?;

        // Set as background and clear
        x11_conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().background_pixmap(front),
        )?;

        let (width, height) = self.buffer.dimensions();
        x11_conn.clear_area(false, root, 0, 0, width, height)?;
        x11_conn.flush()?;

        Ok(())
    }

    /// Render and present in one call
    pub fn render_and_present(&mut self, conn: &mut Connection, frame: &image::RgbaImage) -> Result<()> {
        self.render_frame(conn, frame)?;
        self.present(conn)
    }

    /// Get dimensions
    pub fn dimensions(&self) -> (u16, u16) {
        self.buffer.dimensions()
    }

    /// Clean up resources
    pub fn destroy(self, conn: &Connection) {
        self.buffer.destroy(conn);
    }
}

/// Convert RGBA to BGRA into a reusable buffer (X11 native format for 32-bit visuals)
fn rgba_to_bgra_into(image: &image::RgbaImage, buf: &mut Vec<u8>) {
    let raw = image.as_raw();
    buf.clear();
    buf.reserve(raw.len());
    // Process 4 bytes at a time (one pixel)
    for chunk in raw.chunks_exact(4) {
        buf.push(chunk[2]); // B
        buf.push(chunk[1]); // G
        buf.push(chunk[0]); // R
        buf.push(chunk[3]); // A
    }
}
