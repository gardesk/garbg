//! HTTP/HTTPS source provider

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;

use super::{FetchedImage, MediaType, SourceProvider, WallpaperEntry};
use crate::media::ImageLoader;

/// Provider for HTTP/HTTPS URLs
pub struct HttpProvider {
    client: reqwest::Client,
}

impl HttpProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("garbg/0.1")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Determine media type from URL or content-type
    fn media_type_from_url(url: &str) -> MediaType {
        let path = url.split('?').next().unwrap_or(url);
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        match ext.as_deref() {
            Some("gif") => MediaType::AnimatedImage,
            Some("mp4" | "webm" | "mkv") => MediaType::Video,
            _ => MediaType::StaticImage,
        }
    }
}

#[async_trait]
impl SourceProvider for HttpProvider {
    fn id(&self) -> &str {
        "http"
    }

    fn can_handle(&self, uri: &str) -> bool {
        uri.starts_with("http://") || uri.starts_with("https://")
    }

    async fn list(&self, uri: &str) -> Result<Vec<WallpaperEntry>> {
        // For HTTP, we treat the URL as a single entry
        // Directory listing is handled by DirectoryIndexProvider
        let name = uri
            .split('/')
            .last()
            .unwrap_or("image")
            .split('?')
            .next()
            .unwrap_or("image")
            .to_string();

        Ok(vec![WallpaperEntry {
            uri: uri.to_string(),
            name,
            media_type: Self::media_type_from_url(uri),
            size: None,
            metadata: Default::default(),
        }])
    }

    async fn fetch(&self, entry: &WallpaperEntry) -> Result<FetchedImage> {
        let response = self
            .client
            .get(&entry.uri)
            .send()
            .await
            .with_context(|| format!("Failed to fetch: {}", entry.uri))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("HTTP error {}: {}", status, entry.uri);
        }

        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("Failed to read response body: {}", entry.uri))?;

        let image = ImageLoader::load_bytes(&bytes, None)?;

        Ok(FetchedImage {
            image,
            uri: entry.uri.clone(),
            media_type: entry.media_type,
        })
    }
}
