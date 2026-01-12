# Phase 3: Remote Image Sources

## Goal
Implement multiple image source providers for fetching wallpapers from various remote locations.

## Tasks

### 3.1 Provider Architecture
- [ ] Define `SourceProvider` trait
- [ ] Create `ProviderRegistry` for provider lookup
- [ ] URI-based provider selection
- [ ] Async fetch operations

### 3.2 HTTP Provider
- [ ] Fetch images from direct URLs
- [ ] Support redirects and HTTPS
- [ ] Handle common HTTP errors
- [ ] Respect Content-Type headers

### 3.3 GitHub Provider
- [ ] Parse `github://user/repo/path` URIs
- [ ] Use GitHub API for directory listings
- [ ] Fetch raw file content
- [ ] Handle rate limiting (with optional token)

### 3.4 Directory Index Provider
- [ ] Parse Apache autoindex HTML
- [ ] Parse nginx autoindex HTML
- [ ] Extract image links from listings
- [ ] Support recursive directory traversal

### 3.5 S3 Provider (Optional Feature)
- [ ] Parse `s3://bucket/prefix` URIs
- [ ] List objects with prefix
- [ ] Support S3-compatible endpoints (MinIO, etc.)
- [ ] Handle authentication

### 3.6 Disk Cache
- [ ] Cache fetched images to ~/.cache/garbg/
- [ ] LRU eviction when cache exceeds size limit
- [ ] Store metadata (URL, fetch time, ETag)
- [ ] Conditional requests for cache validation

## Deliverables
- `garbg set https://example.com/wallpaper.png` fetches and displays
- `garbg set github://user/repo/wallpapers/` lists and picks random
- Directory index URLs work for bulk wallpaper sources
- Fetched images are cached locally

## Provider Trait

```rust
#[async_trait]
pub trait SourceProvider: Send + Sync {
    /// Provider identifier
    fn id(&self) -> &str;

    /// Check if this provider handles a URI
    fn can_handle(&self, uri: &str) -> bool;

    /// List available wallpapers from source
    async fn list(&self, uri: &str) -> Result<Vec<WallpaperEntry>>;

    /// Fetch a specific wallpaper
    async fn fetch(&self, entry: &WallpaperEntry) -> Result<FetchedImage>;
}

pub struct WallpaperEntry {
    pub uri: String,
    pub name: String,
    pub media_type: MediaType,
    pub size: Option<u64>,
}

pub enum MediaType {
    StaticImage,
    AnimatedImage,
    Video,
}
```

## URI Schemes

| Scheme | Example | Provider |
|--------|---------|----------|
| (none) | `/path/to/file.png` | FileProvider |
| `file://` | `file:///path/to/file.png` | FileProvider |
| `http://` | `http://example.com/img.png` | HttpProvider |
| `https://` | `https://example.com/img.png` | HttpProvider |
| `github://` | `github://user/repo/path` | GitHubProvider |
| `s3://` | `s3://bucket/prefix` | S3Provider |

## Cache Structure

```
~/.cache/garbg/
├── index.json          # Cache index with metadata
├── ab/                 # First 2 chars of hash
│   └── abcd1234...     # Cached image file
├── cd/
│   └── cdef5678...
└── ...
```

## Files Modified/Created
- `/garbg/garbg/src/sources/mod.rs` - Sources module
- `/garbg/garbg/src/sources/provider.rs` - Provider trait
- `/garbg/garbg/src/sources/file.rs` - Local file provider
- `/garbg/garbg/src/sources/http.rs` - HTTP provider
- `/garbg/garbg/src/sources/github.rs` - GitHub provider
- `/garbg/garbg/src/sources/directory.rs` - Directory index parser
- `/garbg/garbg/src/sources/s3.rs` - S3 provider (optional)
- `/garbg/garbg/src/cache/mod.rs` - Cache module
- `/garbg/garbg/src/cache/disk.rs` - Disk cache implementation
