//! XDG cache storage with TTL support.
//!
//! Provides a simple key-value cache backed by owner-only, atomically replaced
//! files. Each v2 entry stores a fixed expiry header followed by the raw value.
//! Expired entries are treated as missing and cleaned up on access.
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

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use atomic_write_file::AtomicWriteFile;
use base64::Engine;

use crate::error::{CacheError, Result};

const CACHE_MAGIC: &[u8; 8] = b"LBRCA02\0";
const CACHE_HEADER_LEN: usize = 16;

/// File-based cache with TTL support.
#[derive(Debug)]
pub struct Cache {
    dir: PathBuf,
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
    /// Returns [`Error::Cache`](crate::Error::Cache) if the entry cannot be written.
    pub fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
        self.set_parts(key, &[value], ttl)
    }

    pub(crate) fn set_parts(&self, key: &str, parts: &[&[u8]], ttl: Duration) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(CacheError::from)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let expires_at = now.as_secs().saturating_add(ttl.as_secs());
        let mut header = [0_u8; CACHE_HEADER_LEN];
        header[..CACHE_MAGIC.len()].copy_from_slice(CACHE_MAGIC);
        header[CACHE_MAGIC.len()..].copy_from_slice(&expires_at.to_be_bytes());

        let path = self.key_path(key);
        write_entry(&path, &header, parts).map_err(CacheError::from)?;

        tracing::debug!(key, expires_at, "cache entry written");
        Ok(())
    }

    /// Retrieve a value if it exists and hasn't expired.
    ///
    /// Returns `Ok(None)` for missing or expired entries.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`](crate::Error::Cache) on I/O errors or invalid
    /// cache framing.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.key_path(key);
        let (expires_at, value) = match read_entry(&path) {
            Ok(entry) => entry,
            Err(CacheError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now >= expires_at {
            tracing::debug!(key, "cache entry expired");
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }

        Ok(Some(value))
    }

    /// Remove a cached entry.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`](crate::Error::Cache) on I/O errors (missing entries are not errors).
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
    /// Returns [`Error::Cache`](crate::Error::Cache) if the cache directory cannot be read.
    pub fn clear(&self) -> Result<()> {
        if self.dir.exists() {
            for entry in std::fs::read_dir(&self.dir)
                .map_err(CacheError::from)?
                .flatten()
            {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("cache") {
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
        // URL-safe base64 is filesystem-safe and preserves the complete key,
        // unlike lossy character replacement where `foo/bar` and `foo:bar`
        // collapse to the same filename. The version prefix leaves room for a
        // future encoding change without mistaking old files for new ones.
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.as_bytes());
        self.dir.join(format!("v2-{encoded}.cache"))
    }
}

fn read_entry(path: &Path) -> std::result::Result<(u64, Vec<u8>), CacheError> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0_u8; CACHE_HEADER_LEN];
    if let Err(error) = file.read_exact(&mut header) {
        return if error.kind() == std::io::ErrorKind::UnexpectedEof {
            Err(CacheError::Format("truncated header".to_string()))
        } else {
            Err(error.into())
        };
    }
    if &header[..CACHE_MAGIC.len()] != CACHE_MAGIC {
        return Err(CacheError::Format(
            "unsupported magic or version".to_string(),
        ));
    }

    let expires_at = u64::from_be_bytes(
        header[CACHE_MAGIC.len()..]
            .try_into()
            .expect("cache expiry occupies eight bytes"),
    );
    let mut value = Vec::new();
    file.read_to_end(&mut value)?;
    Ok((expires_at, value))
}

fn write_entry(path: &Path, header: &[u8], parts: &[&[u8]]) -> std::io::Result<()> {
    let mut options = AtomicWriteFile::options();
    #[cfg(unix)]
    {
        use atomic_write_file::unix::OpenOptionsExt as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).preserve_mode(false);
    }
    let mut file = options.open(path)?;
    file.write_all(header)?;
    for part in parts {
        file.write_all(part)?;
    }
    file.sync_all()?;
    file.commit()
}

/// Get the default cache directory for an application.
///
/// - macOS: `~/Library/Caches/{app}/librebar/`
/// - Linux: `$XDG_CACHE_HOME/{app}/librebar/`
pub fn default_cache_dir(app_name: &str) -> Option<PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Some(proj_dirs.cache_dir().join("librebar"))
}
