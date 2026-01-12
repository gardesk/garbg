//! Directory index source provider
//!
//! Parses Apache/nginx autoindex HTML pages to list available files.

use anyhow::{Context, Result};
use async_trait::async_trait;
use scraper::{Html, Selector};
use std::path::Path;

use super::{FetchedImage, MediaType, SourceProvider, WallpaperEntry};
use crate::media::ImageLoader;

/// Provider for HTTP directory indexes (Apache/nginx autoindex)
pub struct DirectoryIndexProvider {
    client: reqwest::Client,
}

impl DirectoryIndexProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("garbg/0.1")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Determine media type from filename
    fn media_type_from_filename(name: &str) -> MediaType {
        let ext = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        match ext.as_deref() {
            Some("gif") => MediaType::AnimatedImage,
            Some("mp4" | "webm") => MediaType::Video,
            _ => MediaType::StaticImage,
        }
    }

    /// Check if a filename is a supported image format
    fn is_image_file(name: &str) -> bool {
        ImageLoader::is_supported_format(Path::new(name))
    }

    /// Parse links from HTML directory listing
    fn parse_directory_html(html: &str, base_url: &str) -> Vec<(String, String)> {
        let document = Html::parse_document(html);
        let selector = Selector::parse("a[href]").unwrap();

        let mut links = Vec::new();

        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                // Skip parent directory links
                if href == "../" || href == ".." || href.starts_with('?') {
                    continue;
                }

                // Get the display name (either href or element text)
                let name = element.text().collect::<String>();
                let name = name.trim();
                let name = if name.is_empty() { href } else { name };

                // Build full URL
                let full_url = if href.starts_with("http://") || href.starts_with("https://") {
                    href.to_string()
                } else if href.starts_with('/') {
                    // Absolute path
                    let url = url::Url::parse(base_url).ok();
                    url.map(|u| format!("{}://{}{}", u.scheme(), u.host_str().unwrap_or(""), href))
                        .unwrap_or_else(|| href.to_string())
                } else {
                    // Relative path
                    let base = if base_url.ends_with('/') {
                        base_url.to_string()
                    } else {
                        format!("{}/", base_url)
                    };
                    format!("{}{}", base, href)
                };

                links.push((name.to_string(), full_url));
            }
        }

        links
    }
}

#[async_trait]
impl SourceProvider for DirectoryIndexProvider {
    fn id(&self) -> &str {
        "directory"
    }

    fn can_handle(&self, uri: &str) -> bool {
        // This provider handles HTTP URLs that end with / (directory listings)
        // Or are explicitly marked as directory indexes
        (uri.starts_with("http://") || uri.starts_with("https://"))
            && (uri.ends_with('/') || uri.contains("?C=") || uri.contains("autoindex"))
    }

    async fn list(&self, uri: &str) -> Result<Vec<WallpaperEntry>> {
        let response = self
            .client
            .get(uri)
            .send()
            .await
            .with_context(|| format!("Failed to fetch directory: {}", uri))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("HTTP error {}: {}", status, uri);
        }

        let html = response.text().await?;
        let links = Self::parse_directory_html(&html, uri);

        let entries: Vec<WallpaperEntry> = links
            .into_iter()
            .filter(|(name, _url)| Self::is_image_file(name))
            .map(|(name, url)| WallpaperEntry {
                uri: url,
                name: name.clone(),
                media_type: Self::media_type_from_filename(&name),
                size: None,
                metadata: Default::default(),
            })
            .collect();

        Ok(entries)
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

        let bytes = response.bytes().await?;
        let image = ImageLoader::load_bytes(&bytes, None)?;

        Ok(FetchedImage {
            image,
            uri: entry.uri.clone(),
            media_type: entry.media_type,
        })
    }
}
