//! Image source providers
//!
//! Supports fetching wallpapers from various sources:
//! - Local files
//! - HTTP/HTTPS URLs
//! - GitHub repositories
//! - Directory indexes (Apache/nginx)
//! - S3-compatible storage (optional)

mod provider;
mod file;
mod http;
mod github;
mod directory;

pub use provider::{SourceProvider, ProviderRegistry, WallpaperEntry, MediaType, FetchedImage};
pub use file::FileProvider;
pub use http::HttpProvider;
pub use github::GitHubProvider;
pub use directory::DirectoryIndexProvider;
