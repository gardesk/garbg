//! Local file source provider

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;

use super::{FetchedImage, MediaType, SourceProvider, WallpaperEntry};
use crate::media::ImageLoader;

/// Provider for local files
pub struct FileProvider;

impl FileProvider {
    pub fn new() -> Self {
        Self
    }

    /// Determine media type from file extension
    fn media_type_from_path(path: &Path) -> MediaType {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        match ext.as_deref() {
            Some("gif") => MediaType::AnimatedImage,
            Some("mp4" | "webm" | "mkv" | "avi") => MediaType::Video,
            _ => MediaType::StaticImage,
        }
    }
}

#[async_trait]
impl SourceProvider for FileProvider {
    fn id(&self) -> &str {
        "file"
    }

    fn can_handle(&self, uri: &str) -> bool {
        // Handle file:// URIs and bare paths
        uri.starts_with("file://") || uri.starts_with('/') || uri.starts_with('~')
    }

    async fn list(&self, uri: &str) -> Result<Vec<WallpaperEntry>> {
        let path_str = uri.strip_prefix("file://").unwrap_or(uri);
        let path_str = shellexpand::tilde(path_str);
        let path = Path::new(path_str.as_ref());

        if path.is_file() {
            // Single file
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let size = path.metadata().ok().map(|m| m.len());

            Ok(vec![WallpaperEntry {
                uri: uri.to_string(),
                name,
                media_type: Self::media_type_from_path(path),
                size,
                metadata: Default::default(),
            }])
        } else if path.is_dir() {
            // Directory - list all supported files
            let mut entries = Vec::new();

            let read_dir = std::fs::read_dir(path)
                .with_context(|| format!("Failed to read directory: {}", path.display()))?;

            for entry in read_dir.flatten() {
                let entry_path = entry.path();
                if entry_path.is_file() && ImageLoader::is_supported_format(&entry_path) {
                    let name = entry_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let size = entry.metadata().ok().map(|m| m.len());

                    entries.push(WallpaperEntry {
                        uri: entry_path.to_string_lossy().to_string(),
                        name,
                        media_type: Self::media_type_from_path(&entry_path),
                        size,
                        metadata: Default::default(),
                    });
                }
            }

            // Sort by name
            entries.sort_by(|a, b| a.name.cmp(&b.name));

            Ok(entries)
        } else {
            anyhow::bail!("Path does not exist: {}", path.display());
        }
    }

    async fn fetch(&self, entry: &WallpaperEntry) -> Result<FetchedImage> {
        let path_str = entry.uri.strip_prefix("file://").unwrap_or(&entry.uri);
        let path_str = shellexpand::tilde(path_str);

        let image = ImageLoader::load_file(path_str.as_ref())?;

        Ok(FetchedImage {
            image,
            uri: entry.uri.clone(),
            media_type: entry.media_type,
        })
    }
}
