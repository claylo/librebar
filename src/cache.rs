//! XDG cache storage with TTL support.
//!
//! Provides a simple key-value cache backed by the filesystem. Each entry
//! is a JSON file containing the value (base64-encoded) and an expiry
//! timestamp. Expired entries are treated as missing and cleaned up on access.
//!
//! # Example
//!
//! ```no_run
//! use std::time::Duration;
//!
//! let cache = librebar::cache::Cache::default_for("myapp").unwrap();
//! cache.set("api-response", b"cached data", Duration::from_secs(3600)).unwrap();
//!
//! if let Some(data) = cache.get("api-response").unwrap() {
//!     // Use cached data
//!     # drop(data);
//! }
//! # Ok::<(), librebar::Error>(())
//! ```
//!
//! # Cache directory
//!
//! Default: `~/Library/Caches/{app}/librebar/` on macOS,
//! `$XDG_CACHE_HOME/{app}/librebar/` on Linux.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;

use crate::error::{CacheError, Result};

/// File-based cache with TTL support.
#[derive(Debug)]
pub struct Cache {
    dir: PathBuf,
}

/// Serialized cache entry stored as JSON.
#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    /// Expiry as seconds since Unix epoch.
    expires_at: u64,
    /// Base64-encoded value.
    value: String,
}

impl Cache {
    /// Create a cache targeting the given directory.
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// Create a cache in the default platform directory.
    ///
    /// Returns `None` if the platform cache directory cannot be determined.
    pub fn default_for(app_name: &str) -> Option<Self> {
        default_cache_dir(app_name).map(|dir| Self::new(&dir))
    }

    /// Store a value with a TTL.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`] if the entry cannot be written.
    pub fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(CacheError::from)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let expires_at = now.as_secs() + ttl.as_secs();

        let entry = CacheEntry {
            expires_at,
            value: base64::engine::general_purpose::STANDARD.encode(value),
        };

        let path = self.key_path(key);
        let json = serde_json::to_vec(&entry).map_err(CacheError::from)?;
        std::fs::write(&path, json).map_err(CacheError::from)?;

        tracing::debug!(key, expires_at, "cache entry written");
        Ok(())
    }

    /// Retrieve a value if it exists and hasn't expired.
    ///
    /// Returns `Ok(None)` for missing or expired entries.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`] on I/O or deserialization errors.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.key_path(key);
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(CacheError::from(e).into()),
        };

        let entry: CacheEntry = serde_json::from_slice(&data).map_err(CacheError::Json)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now >= entry.expires_at {
            tracing::debug!(key, "cache entry expired");
            // Best-effort cleanup: stale entry will be overwritten on next set().
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }

        let value = base64::engine::general_purpose::STANDARD
            .decode(&entry.value)
            .map_err(CacheError::from)?;

        Ok(Some(value))
    }

    /// Remove a cached entry.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`] on I/O errors (missing entries are not errors).
    pub fn remove(&self, key: &str) -> Result<()> {
        let path = self.key_path(key);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CacheError::from(e).into()),
        }
    }

    /// Clear all cached entries.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`] if the cache directory cannot be read.
    pub fn clear(&self) -> Result<()> {
        if self.dir.exists() {
            for entry in std::fs::read_dir(&self.dir)
                .map_err(CacheError::from)?
                .flatten()
            {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    // Best-effort: skip files that can't be removed (permissions, etc.)
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        Ok(())
    }

    /// Path to the cache directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn key_path(&self, key: &str) -> PathBuf {
        // Sanitize key for filesystem safety
        let safe_key = key.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");
        self.dir.join(format!("{safe_key}.json"))
    }
}

/// Get the default cache directory for an application.
///
/// - macOS: `~/Library/Caches/{app}/librebar/`
/// - Linux: `$XDG_CACHE_HOME/{app}/librebar/`
pub fn default_cache_dir(app_name: &str) -> Option<PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Some(proj_dirs.cache_dir().join("librebar"))
}
