//! GitHub repository source provider

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::Path;

use super::{FetchedImage, MediaType, SourceProvider, WallpaperEntry};
use crate::media::ImageLoader;

/// Provider for GitHub repositories
///
/// Supports URIs like:
/// - `github://user/repo/path/to/file.png`
/// - `github://user/repo/path/to/directory`
pub struct GitHubProvider {
    client: reqwest::Client,
    /// Optional personal access token for higher rate limits
    token: Option<String>,
}

impl GitHubProvider {
    pub fn new() -> Self {
        Self::with_token(None)
    }

    pub fn with_token(token: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("garbg/0.1")
            .build()
            .expect("Failed to create HTTP client");

        Self { client, token }
    }

    /// Parse a github:// URI into (user, repo, path)
    fn parse_uri(uri: &str) -> Result<(String, String, String)> {
        let path = uri
            .strip_prefix("github://")
            .context("Invalid GitHub URI")?;

        let parts: Vec<&str> = path.splitn(3, '/').collect();
        if parts.len() < 2 {
            anyhow::bail!("GitHub URI must be github://user/repo[/path]");
        }

        let user = parts[0].to_string();
        let repo = parts[1].to_string();
        let path = parts.get(2).map(|s| s.to_string()).unwrap_or_default();

        Ok((user, repo, path))
    }

    /// Determine media type from path
    fn media_type_from_path(path: &str) -> MediaType {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        match ext.as_deref() {
            Some("gif") => MediaType::AnimatedImage,
            Some("mp4" | "webm") => MediaType::Video,
            _ => MediaType::StaticImage,
        }
    }
}

#[derive(Deserialize)]
struct GitHubContent {
    name: String,
    path: String,
    #[serde(rename = "type")]
    content_type: String,
    size: Option<u64>,
    download_url: Option<String>,
}

#[async_trait]
impl SourceProvider for GitHubProvider {
    fn id(&self) -> &str {
        "github"
    }

    fn can_handle(&self, uri: &str) -> bool {
        uri.starts_with("github://")
    }

    async fn list(&self, uri: &str) -> Result<Vec<WallpaperEntry>> {
        let (user, repo, path) = Self::parse_uri(uri)?;

        let api_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}",
            user, repo, path
        );

        let mut request = self.client.get(&api_url);

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("token {}", token));
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("Failed to fetch GitHub API: {}", api_url))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error {}: {}", status, body);
        }

        let text = response.text().await?;

        // Try to parse as array (directory) or single object (file)
        let contents: Vec<GitHubContent> = if text.starts_with('[') {
            serde_json::from_str(&text)?
        } else {
            let single: GitHubContent = serde_json::from_str(&text)?;
            vec![single]
        };

        let entries: Vec<WallpaperEntry> = contents
            .into_iter()
            .filter(|c| {
                c.content_type == "file"
                    && c.download_url.is_some()
                    && ImageLoader::is_supported_format(Path::new(&c.name))
            })
            .map(|c| WallpaperEntry {
                uri: c.download_url.unwrap(),
                name: c.name,
                media_type: Self::media_type_from_path(&c.path),
                size: c.size,
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
