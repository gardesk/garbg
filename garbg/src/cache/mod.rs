//! Caching for remote images
//!
//! Provides disk and memory caching for fetched wallpapers.

mod disk;
mod memory;

pub use disk::DiskCache;
pub use memory::FrameCache;
