//! S3 source provider
//!
//! Supports fetching wallpapers from S3 buckets and S3-compatible storage.
//!
//! URI format: `s3://bucket/prefix`
//!
//! Authentication is handled via:
//! - AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY environment variables
//! - AWS credentials file (~/.aws/credentials)
//! - IAM instance roles (when running on EC2)
//!
//! For S3-compatible endpoints (MinIO, etc.):
//! - Set AWS_ENDPOINT_URL environment variable

#![cfg(feature = "s3")]

use anyhow::{Context, Result};
use async_trait::async_trait;
use aws_sdk_s3::Client;
use std::path::Path;

use super::{FetchedImage, MediaType, SourceProvider, WallpaperEntry};
use crate::media::ImageLoader;

/// Provider for S3 and S3-compatible object storage
pub struct S3Provider {
    client: Client,
}

impl S3Provider {
    /// Create a new S3 provider
    ///
    /// Loads credentials from environment or AWS config files.
    pub async fn new() -> Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = Client::new(&config);

        Ok(Self { client })
    }

    /// Create with a custom endpoint URL (for S3-compatible services like MinIO)
    pub async fn with_endpoint(endpoint_url: &str) -> Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;

        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .endpoint_url(endpoint_url)
            .force_path_style(true) // Required for most S3-compatible services
            .build();

        let client = Client::from_conf(s3_config);

        Ok(Self { client })
    }

    /// Parse an s3:// URI into (bucket, prefix)
    fn parse_uri(uri: &str) -> Result<(String, String)> {
        let path = uri
            .strip_prefix("s3://")
            .context("Invalid S3 URI")?;

        let parts: Vec<&str> = path.splitn(2, '/').collect();
        let bucket = parts[0].to_string();
        let prefix = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

        if bucket.is_empty() {
            anyhow::bail!("S3 URI must include a bucket: s3://bucket/prefix");
        }

        Ok((bucket, prefix))
    }

    /// Determine media type from key (path)
    fn media_type_from_key(key: &str) -> MediaType {
        let ext = Path::new(key)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        match ext.as_deref() {
            Some("gif") => MediaType::AnimatedImage,
            Some("mp4" | "webm") => MediaType::Video,
            _ => MediaType::StaticImage,
        }
    }

    /// Check if a key is a supported image format
    fn is_image_key(key: &str) -> bool {
        ImageLoader::is_supported_format(Path::new(key))
    }
}

#[async_trait]
impl SourceProvider for S3Provider {
    fn id(&self) -> &str {
        "s3"
    }

    fn can_handle(&self, uri: &str) -> bool {
        uri.starts_with("s3://")
    }

    async fn list(&self, uri: &str) -> Result<Vec<WallpaperEntry>> {
        let (bucket, prefix) = Self::parse_uri(uri)?;

        tracing::debug!("Listing S3 objects: bucket={}, prefix={}", bucket, prefix);

        let mut entries = Vec::new();
        let mut continuation_token: Option<String> = None;

        // Paginate through results
        loop {
            let mut request = self.client
                .list_objects_v2()
                .bucket(&bucket)
                .prefix(&prefix);

            if let Some(token) = &continuation_token {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .with_context(|| format!("Failed to list S3 bucket: {}", bucket))?;

            if let Some(contents) = response.contents {
                for object in contents {
                    if let Some(key) = object.key {
                        // Skip directory markers
                        if key.ends_with('/') {
                            continue;
                        }

                        // Only include supported image formats
                        if !Self::is_image_key(&key) {
                            continue;
                        }

                        let name = Path::new(&key)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&key)
                            .to_string();

                        entries.push(WallpaperEntry {
                            uri: format!("s3://{}/{}", bucket, key),
                            name,
                            media_type: Self::media_type_from_key(&key),
                            size: object.size.map(|s| s as u64),
                            metadata: Default::default(),
                        });
                    }
                }
            }

            // Check for more results
            if response.is_truncated == Some(true) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }

        tracing::debug!("Found {} images in S3", entries.len());
        Ok(entries)
    }

    async fn fetch(&self, entry: &WallpaperEntry) -> Result<FetchedImage> {
        let (bucket, key) = Self::parse_uri(&entry.uri)?;

        tracing::debug!("Fetching S3 object: bucket={}, key={}", bucket, key);

        let response = self.client
            .get_object()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await
            .with_context(|| format!("Failed to fetch S3 object: {}", entry.uri))?;

        let bytes = response
            .body
            .collect()
            .await
            .context("Failed to read S3 object body")?
            .into_bytes();

        let image = ImageLoader::load_bytes(&bytes, None)?;

        Ok(FetchedImage {
            image,
            uri: entry.uri.clone(),
            media_type: entry.media_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uri() {
        let (bucket, prefix) = S3Provider::parse_uri("s3://my-bucket/wallpapers/").unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "wallpapers/");

        let (bucket, prefix) = S3Provider::parse_uri("s3://bucket").unwrap();
        assert_eq!(bucket, "bucket");
        assert_eq!(prefix, "");

        let (bucket, prefix) = S3Provider::parse_uri("s3://bucket/path/to/file.png").unwrap();
        assert_eq!(bucket, "bucket");
        assert_eq!(prefix, "path/to/file.png");
    }

    #[test]
    fn test_invalid_uri() {
        assert!(S3Provider::parse_uri("http://example.com").is_err());
        assert!(S3Provider::parse_uri("s3://").is_err());
    }
}
