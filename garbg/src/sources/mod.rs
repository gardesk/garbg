//! Image source providers
//!
//! Supports fetching wallpapers from various sources:
//! - Local files
//! - HTTP/HTTPS URLs
//! - GitHub repositories
//! - Directory indexes (Apache/nginx)
//! - S3-compatible storage (optional, requires `s3` feature)

mod provider;
mod file;
mod http;
mod github;
mod directory;

#[cfg(feature = "s3")]
mod s3;

pub use provider::{SourceProvider, ProviderRegistry, WallpaperEntry, MediaType, FetchedImage};
pub use file::FileProvider;
pub use http::HttpProvider;
pub use github::GitHubProvider;
pub use directory::DirectoryIndexProvider;

#[cfg(feature = "s3")]
pub use s3::S3Provider;
