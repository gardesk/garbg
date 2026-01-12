//! Source provider trait and registry

use anyhow::Result;
use async_trait::async_trait;
use image::RgbaImage;
use std::collections::HashMap;

/// Type of media content
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    /// Static image (PNG, JPEG, WebP, etc.)
    StaticImage,
    /// Animated image (GIF, APNG, animated WebP)
    AnimatedImage,
    /// Video file (MP4, WebM)
    Video,
}

/// Entry representing a wallpaper from a source
#[derive(Debug, Clone)]
pub struct WallpaperEntry {
    /// Full URI to the wallpaper
    pub uri: String,
    /// Display name
    pub name: String,
    /// Type of media
    pub media_type: MediaType,
    /// File size in bytes (if known)
    pub size: Option<u64>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Result of fetching a wallpaper
pub struct FetchedImage {
    /// The decoded image data
    pub image: RgbaImage,
    /// Original URI
    pub uri: String,
    /// Media type
    pub media_type: MediaType,
}

/// Trait for wallpaper source providers
#[async_trait]
pub trait SourceProvider: Send + Sync {
    /// Provider identifier (e.g., "file", "http", "github")
    fn id(&self) -> &str;

    /// Check if this provider can handle a given URI
    fn can_handle(&self, uri: &str) -> bool;

    /// List available wallpapers from a source
    ///
    /// For single-file sources, returns a single entry.
    /// For directory sources, returns all entries.
    async fn list(&self, uri: &str) -> Result<Vec<WallpaperEntry>>;

    /// Fetch a specific wallpaper
    async fn fetch(&self, entry: &WallpaperEntry) -> Result<FetchedImage>;

    /// Whether this source supports streaming (for video)
    fn supports_streaming(&self) -> bool {
        false
    }
}

/// Registry of all available providers
pub struct ProviderRegistry {
    providers: Vec<Box<dyn SourceProvider>>,
}

impl ProviderRegistry {
    /// Create a new registry with default providers
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Register a provider
    pub fn register(&mut self, provider: Box<dyn SourceProvider>) {
        self.providers.push(provider);
    }

    /// Find a provider that can handle a URI
    pub fn find_provider(&self, uri: &str) -> Option<&dyn SourceProvider> {
        self.providers
            .iter()
            .find(|p| p.can_handle(uri))
            .map(|p| p.as_ref())
    }

    /// Get all registered providers
    pub fn providers(&self) -> &[Box<dyn SourceProvider>] {
        &self.providers
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        let mut registry = Self::new();

        // Register default providers
        registry.register(Box::new(super::FileProvider::new()));
        // HTTP and other providers will be registered when needed

        registry
    }
}
