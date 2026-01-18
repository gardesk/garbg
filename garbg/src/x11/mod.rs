//! X11 integration for garbg
//!
//! Handles connection to the X server, root window manipulation,
//! and pixmap-based wallpaper rendering.

mod connection;
mod renderer;
mod monitors;
mod animation;
mod compositor;

pub use connection::{Connection, X11Error, CloseDownMode};
pub use renderer::Renderer;
pub use monitors::Monitor;
pub use animation::{AnimationRenderer, DoubleBuffer};
pub use compositor::{Compositor, MonitorWallpaper};
