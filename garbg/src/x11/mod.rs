//! X11 integration for garbg
//!
//! Handles connection to the X server, root window manipulation,
//! and pixmap-based wallpaper rendering.

mod connection;
mod renderer;
mod monitors;
mod animation;

pub use connection::{Connection, X11Error};
pub use renderer::Renderer;
pub use monitors::Monitor;
pub use animation::{AnimationRenderer, DoubleBuffer};
