//! Disk cache for remote images

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Disk cache for remote images
pub struct DiskCache {
    cache_dir: PathBuf,
    max_size: u64,
    index: CacheIndex,
}

/// Cache index stored on disk
#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheIndex {
    entries: HashMap<String, CacheEntry>,
    total_size: u64,
}

/// Individual cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// Hash of the original URI
    uri_hash: String,
    /// Original URI
    original_uri: String,
    /// Path to cached file (relative to cache dir)
    file_path: String,
    /// File size in bytes
    size: u64,
    /// When the file was fetched
    fetched_at: SystemTime,
    /// Last access time
    last_accessed: SystemTime,
    /// HTTP ETag for conditional requests
    etag: Option<String>,
}

impl DiskCache {
    /// Create a new disk cache
    pub fn new(cache_dir: PathBuf, max_size_mb: u64) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache directory: {}", cache_dir.display()))?;

        let index_path = cache_dir.join("index.json");
        let index = if index_path.exists() {
            let data = std::fs::read_to_string(&index_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            CacheIndex::default()
        };

        Ok(Self {
            cache_dir,
            max_size: max_size_mb * 1024 * 1024,
            index,
        })
    }

    /// Get the default cache directory
    pub fn default_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("garbg"))
    }

    /// Check if a URI is cached
    pub fn is_cached(&self, uri: &str) -> bool {
        let hash = Self::hash_uri(uri);
        if let Some(entry) = self.index.entries.get(&hash) {
            let path = self.cache_dir.join(&entry.file_path);
            path.exists()
        } else {
            false
        }
    }

    /// Get cached file path for a URI
    pub fn get(&mut self, uri: &str) -> Option<PathBuf> {
        let hash = Self::hash_uri(uri);
        if let Some(entry) = self.index.entries.get_mut(&hash) {
            let path = self.cache_dir.join(&entry.file_path);
            if path.exists() {
                entry.last_accessed = SystemTime::now();
                return Some(path);
            }
        }
        None
    }

    /// Store data in the cache
    pub fn store(&mut self, uri: &str, data: &[u8], etag: Option<String>) -> Result<PathBuf> {
        let hash = Self::hash_uri(uri);

        // Create subdirectory based on first 2 chars of hash
        let subdir = &hash[..2];
        let dir = self.cache_dir.join(subdir);
        std::fs::create_dir_all(&dir)?;

        // Write file
        let file_path = format!("{}/{}", subdir, hash);
        let full_path = self.cache_dir.join(&file_path);
        std::fs::write(&full_path, data)?;

        // Update index
        let entry = CacheEntry {
            uri_hash: hash.clone(),
            original_uri: uri.to_string(),
            file_path,
            size: data.len() as u64,
            fetched_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            etag,
        };

        // Remove old entry if exists
        if let Some(old) = self.index.entries.remove(&hash) {
            self.index.total_size -= old.size;
        }

        self.index.total_size += entry.size;
        self.index.entries.insert(hash, entry);

        // Evict if over size limit
        self.evict_if_needed()?;

        // Save index
        self.save_index()?;

        Ok(full_path)
    }

    /// Evict old entries if cache is over size limit
    fn evict_if_needed(&mut self) -> Result<()> {
        if self.index.total_size <= self.max_size {
            return Ok(());
        }

        // Sort by last accessed time (oldest first)
        let mut entries: Vec<_> = self.index.entries.values().cloned().collect();
        entries.sort_by_key(|e| e.last_accessed);

        // Remove oldest until under limit
        for entry in entries {
            if self.index.total_size <= self.max_size {
                break;
            }

            let path = self.cache_dir.join(&entry.file_path);
            if path.exists() {
                std::fs::remove_file(&path)?;
            }

            self.index.total_size -= entry.size;
            self.index.entries.remove(&entry.uri_hash);
        }

        Ok(())
    }

    /// Save the cache index to disk
    fn save_index(&self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");
        let data = serde_json::to_string_pretty(&self.index)?;
        std::fs::write(index_path, data)?;
        Ok(())
    }

    /// Clear the entire cache
    pub fn clear(&mut self) -> Result<()> {
        for entry in self.index.entries.values() {
            let path = self.cache_dir.join(&entry.file_path);
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
        }
        self.index.entries.clear();
        self.index.total_size = 0;
        self.save_index()?;
        Ok(())
    }

    /// Hash a URI for cache key
    fn hash_uri(uri: &str) -> String {
        let hash = blake3::hash(uri.as_bytes());
        hash.to_hex().to_string()
    }
}
