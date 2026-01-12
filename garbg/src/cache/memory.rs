//! Memory cache for decoded frames

use image::RgbaImage;
use lru::LruCache;
use std::num::NonZeroUsize;

/// Key for frame cache
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FrameKey {
    /// Source URI
    pub uri: String,
    /// Frame index (0 for static images)
    pub frame_index: usize,
    /// Target width
    pub width: u16,
    /// Target height
    pub height: u16,
}

/// Cached frame data
pub struct CachedFrame {
    /// Decoded RGBA image
    pub image: RgbaImage,
    /// Memory size in bytes
    pub size: usize,
}

/// LRU cache for decoded frames
pub struct FrameCache {
    cache: LruCache<FrameKey, CachedFrame>,
    max_memory: usize,
    current_memory: usize,
}

impl FrameCache {
    /// Create a new frame cache with maximum memory limit
    pub fn new(max_memory_mb: usize) -> Self {
        // Allow up to 1000 frames
        let capacity = NonZeroUsize::new(1000).unwrap();
        Self {
            cache: LruCache::new(capacity),
            max_memory: max_memory_mb * 1024 * 1024,
            current_memory: 0,
        }
    }

    /// Get a frame from cache
    pub fn get(&mut self, key: &FrameKey) -> Option<&CachedFrame> {
        self.cache.get(key)
    }

    /// Put a frame in cache
    pub fn put(&mut self, key: FrameKey, frame: CachedFrame) {
        let size = frame.size;

        // Evict if adding this would exceed limit
        while self.current_memory + size > self.max_memory {
            if let Some((_, evicted)) = self.cache.pop_lru() {
                self.current_memory -= evicted.size;
            } else {
                break;
            }
        }

        // Add to cache
        if let Some((_, old)) = self.cache.push(key, frame) {
            self.current_memory -= old.size;
        }
        self.current_memory += size;
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.current_memory = 0;
    }

    /// Get current memory usage
    pub fn memory_usage(&self) -> usize {
        self.current_memory
    }
}
