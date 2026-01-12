//! X11 connection management and atom interning

use anyhow::{Context, Result};
use x11rb::connection::Connection as X11Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// X11 connection errors with helpful messages
#[derive(Debug, thiserror::Error)]
pub enum X11Error {
    #[error("DISPLAY environment variable not set. Is an X11 server running?")]
    NoDisplay,

    #[error("Failed to connect to X server at '{display}': {source}. Is the X server running?")]
    ConnectionFailed {
        display: String,
        #[source]
        source: x11rb::errors::ConnectError,
    },

    #[error("X11 operation failed: {0}")]
    Protocol(#[from] x11rb::errors::ConnectionError),

    #[error("X11 reply error: {0}")]
    Reply(#[from] x11rb::errors::ReplyError),
}

/// Interned X11 atoms for wallpaper operations
pub struct Atoms {
    /// Standard atom for root pixmap (used by many apps)
    pub xrootpmap_id: Atom,
    /// Esetroot compatibility atom
    pub esetroot_pmap_id: Atom,
}

impl Atoms {
    fn intern(conn: &RustConnection) -> Result<Self> {
        let xrootpmap_id = conn
            .intern_atom(false, b"_XROOTPMAP_ID")?
            .reply()
            .context("Failed to intern _XROOTPMAP_ID")?
            .atom;

        let esetroot_pmap_id = conn
            .intern_atom(false, b"ESETROOT_PMAP_ID")?
            .reply()
            .context("Failed to intern ESETROOT_PMAP_ID")?
            .atom;

        Ok(Self {
            xrootpmap_id,
            esetroot_pmap_id,
        })
    }
}

/// X11 connection wrapper for garbg
pub struct Connection {
    conn: RustConnection,
    screen_num: usize,
    root: Window,
    depth: u8,
    visual: Visualid,
    gc: Gcontext,
    atoms: Atoms,
    /// Currently set root pixmap (if any)
    current_pixmap: Option<Pixmap>,
}

impl Connection {
    /// Create a new X11 connection
    ///
    /// Returns helpful error messages if X11 is not available:
    /// - Checks for DISPLAY environment variable
    /// - Provides actionable error messages for common failures
    pub fn new() -> Result<Self> {
        // Check DISPLAY environment variable first for a better error message
        let display = std::env::var("DISPLAY").ok();
        if display.is_none() {
            return Err(X11Error::NoDisplay.into());
        }

        let (conn, screen_num) = match RustConnection::connect(None) {
            Ok(result) => result,
            Err(e) => {
                return Err(X11Error::ConnectionFailed {
                    display: display.unwrap_or_else(|| "unknown".to_string()),
                    source: e,
                }.into());
            }
        };

        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let depth = screen.root_depth;
        let visual = screen.root_visual;

        // Create a graphics context for drawing
        let gc = conn.generate_id()?;
        conn.create_gc(gc, root, &CreateGCAux::new())?;

        let atoms = Atoms::intern(&conn)?;

        Ok(Self {
            conn,
            screen_num,
            root,
            depth,
            visual,
            gc,
            atoms,
            current_pixmap: None,
        })
    }

    /// Get screen dimensions (width, height)
    pub fn screen_dimensions(&self) -> (u16, u16) {
        let screen = &self.conn.setup().roots[self.screen_num];
        (screen.width_in_pixels, screen.height_in_pixels)
    }

    /// Get the root window ID
    pub fn root(&self) -> Window {
        self.root
    }

    /// Get the connection reference
    pub fn conn(&self) -> &RustConnection {
        &self.conn
    }

    /// Get screen depth
    pub fn depth(&self) -> u8 {
        self.depth
    }

    /// Get visual ID
    pub fn visual(&self) -> Visualid {
        self.visual
    }

    /// Get graphics context
    pub fn gc(&self) -> Gcontext {
        self.gc
    }

    /// Get atoms
    pub fn atoms(&self) -> &Atoms {
        &self.atoms
    }

    /// Check if the X11 connection is still alive
    ///
    /// Performs a round-trip to the X server to verify connectivity.
    /// Returns false if the connection is broken.
    pub fn is_alive(&self) -> bool {
        // GetInputFocus is a cheap round-trip to verify connection health
        self.conn.get_input_focus().is_ok()
            && self.conn.sync().is_ok()
    }

    /// Set a wallpaper from BGRA image data
    pub fn set_wallpaper(&mut self, image: &image::RgbaImage) -> Result<()> {
        let (width, height) = self.screen_dimensions();

        // Convert RGBA to BGRA (X11 native format)
        let bgra_data = rgba_to_bgra(image);

        // Create a new pixmap
        let pixmap = self.conn.generate_id()?;
        self.conn.create_pixmap(self.depth, pixmap, self.root, width, height)?;

        // X11 has a maximum request size. We need to send large images in chunks.
        // Calculate how many rows we can send per request.
        // Request overhead is ~28 bytes, max request size from setup.
        let max_request_bytes = self.conn.setup().maximum_request_length as usize * 4;
        let bytes_per_row = width as usize * 4; // 4 bytes per pixel (BGRA)
        let request_overhead = 28; // PutImage request header size
        let max_rows_per_request = (max_request_bytes - request_overhead) / bytes_per_row;
        let max_rows_per_request = max_rows_per_request.max(1) as u16;

        // Send image in chunks
        let mut y_offset: u16 = 0;
        while y_offset < height {
            let rows_to_send = (height - y_offset).min(max_rows_per_request);
            let start_byte = y_offset as usize * bytes_per_row;
            let end_byte = (y_offset as usize + rows_to_send as usize) * bytes_per_row;
            let chunk = &bgra_data[start_byte..end_byte];

            self.conn.put_image(
                ImageFormat::Z_PIXMAP,
                pixmap,
                self.gc,
                width,
                rows_to_send,
                0,
                y_offset as i16,
                0,
                self.depth,
                chunk,
            )?;

            y_offset += rows_to_send;
        }

        // Set the pixmap as root window background
        self.set_root_pixmap(pixmap)?;

        // Free the old pixmap if we had one
        if let Some(old_pixmap) = self.current_pixmap.take() {
            self.conn.free_pixmap(old_pixmap)?;
        }

        self.current_pixmap = Some(pixmap);
        self.conn.flush()?;

        Ok(())
    }

    /// Set a pixmap as the root window background
    fn set_root_pixmap(&self, pixmap: Pixmap) -> Result<()> {
        // Set the standard atoms so other applications can detect the wallpaper
        self.conn.change_property32(
            PropMode::REPLACE,
            self.root,
            self.atoms.xrootpmap_id,
            AtomEnum::PIXMAP,
            &[pixmap],
        )?;

        self.conn.change_property32(
            PropMode::REPLACE,
            self.root,
            self.atoms.esetroot_pmap_id,
            AtomEnum::PIXMAP,
            &[pixmap],
        )?;

        // Set the pixmap as the actual background
        self.conn.change_window_attributes(
            self.root,
            &ChangeWindowAttributesAux::new().background_pixmap(pixmap),
        )?;

        // Clear the root window to display the new background
        let (width, height) = self.screen_dimensions();
        self.conn.clear_area(false, self.root, 0, 0, width, height)?;

        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // Clean up the pixmap when we're done
        if let Some(pixmap) = self.current_pixmap.take() {
            let _ = self.conn.free_pixmap(pixmap);
        }
        let _ = self.conn.free_gc(self.gc);
    }
}

/// Convert RGBA to BGRA (X11 native format for 32-bit visuals)
fn rgba_to_bgra(image: &image::RgbaImage) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(image.len());
    for pixel in image.pixels() {
        bgra.push(pixel[2]); // B
        bgra.push(pixel[1]); // G
        bgra.push(pixel[0]); // R
        bgra.push(pixel[3]); // A
    }
    bgra
}
