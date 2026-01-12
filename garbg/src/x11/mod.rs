//! X11 integration for garbg
//!
//! Handles connection to the X server, root window manipulation,
//! and pixmap-based wallpaper rendering.

mod connection;
mod renderer;
mod monitors;

pub use connection::Connection;
pub use renderer::Renderer;
pub use monitors::Monitor;
