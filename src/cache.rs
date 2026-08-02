//! XDG cache storage with TTL support.
//!
//! Provides a simple key-value cache backed by owner-only, atomically replaced
//! files. Each v2 entry stores a fixed expiry header followed by the raw value.
//! Expired entries are treated as missing and cleaned up by explicit pruning
//! and periodic write-path maintenance.
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

use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;

use crate::error::{CacheError, Result};

const CACHE_MAGIC: &[u8; 8] = b"LBRCA02\0";
const CACHE_HEADER_LEN: usize = 16;
const AUTOMATIC_PRUNE_INTERVAL_SECS: u64 = 60 * 60;

/// File-based cache with TTL support.
///
/// Expired entries are removed when [`Cache::prune`] is called and
/// opportunistically before the first write and at most hourly afterward.
/// The cache does not impose an entry-count or byte-size ceiling.
#[derive(Clone, Debug)]
pub struct Cache {
    dir: PathBuf,
    last_prune: Arc<AtomicU64>,
}

impl Cache {
    /// Create a cache targeting the given directory.
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            last_prune: Arc::new(AtomicU64::new(0)),
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
            .unwrap_or_default()
            .as_secs();
        self.prune_if_due(now);
        let expires_at = now.saturating_add(ttl.as_secs());
        let mut header = [0_u8; CACHE_HEADER_LEN];
        header[..CACHE_MAGIC.len()].copy_from_slice(CACHE_MAGIC);
        header[CACHE_MAGIC.len()..].copy_from_slice(&expires_at.to_be_bytes());

        let path = self.key_path(key);
        let _lock = acquire_cache_lock(&self.dir);
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
            return Ok(None);
        }

        Ok(Some(value))
    }

    /// Remove expired entries and return the number removed.
    ///
    /// Only v2 cache files are inspected. Missing directories are treated as
    /// empty, and malformed or individually inaccessible entries are skipped.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`](crate::Error::Cache) if the cache directory
    /// cannot be read.
    pub fn prune(&self) -> Result<usize> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let removed = self.prune_at(now)?;
        self.last_prune.store(now, Ordering::Relaxed);
        Ok(removed)
    }

    fn prune_if_due(&self, now: u64) {
        let previous = self.last_prune.load(Ordering::Relaxed);
        if previous != 0 && now.saturating_sub(previous) < AUTOMATIC_PRUNE_INTERVAL_SECS {
            return;
        }
        if self
            .last_prune
            .compare_exchange(previous, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        if let Err(error) = self.prune_at(now) {
            let _ = self.last_prune.compare_exchange(
                now,
                previous,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
            tracing::warn!(error = %error, "automatic cache pruning failed");
        }
    }

    fn prune_at(&self, now: u64) -> Result<usize> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(CacheError::from(error).into()),
        };
        let _lock = acquire_cache_lock(&self.dir);
        let mut removed = 0;

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to inspect cache directory entry");
                    continue;
                }
            };
            let path = entry.path();
            if !is_v2_cache_path(&path) {
                continue;
            }
            let expires_at = match read_expiry_at(&path) {
                Ok(expires_at) => expires_at,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to inspect cache entry while pruning");
                    continue;
                }
            };
            if now < expires_at {
                continue;
            }

            match std::fs::remove_file(&path) {
                Ok(()) => removed += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(error = %error, "failed to remove expired cache entry");
                }
            }
        }

        Ok(removed)
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
    /// Best-effort: individual files that cannot be removed are silently skipped.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`](crate::Error::Cache) if the cache directory cannot be read.
    pub fn clear(&self) -> Result<()> {
        self.clear_report().map(|_| ())
    }

    /// Clear all cached entries and report results.
    ///
    /// Best-effort: individual files that cannot be removed are recorded in
    /// [`ClearReport::failed`] rather than aborting the operation.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`](crate::Error::Cache) if the cache directory cannot be read.
    pub fn clear_report(&self) -> Result<ClearReport> {
        let mut report = ClearReport {
            removed: 0,
            failed: Vec::new(),
        };
        if self.dir.exists() {
            for entry in std::fs::read_dir(&self.dir)
                .map_err(CacheError::from)?
                .flatten()
            {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("cache") {
                    match std::fs::remove_file(&path) {
                        Ok(()) => report.removed += 1,
                        Err(_) => report.failed.push(path),
                    }
                }
            }
        }
        Ok(report)
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

#[cfg(any(feature = "http-cache", feature = "update"))]
pub(crate) async fn run_io<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| CacheError::Io(std::io::Error::other(error)))?
}

fn read_entry(path: &Path) -> std::result::Result<(u64, Vec<u8>), CacheError> {
    let mut file = std::fs::File::open(path)?;
    let expires_at = read_expiry(&mut file)?;
    let mut value = Vec::new();
    file.read_to_end(&mut value)?;
    Ok((expires_at, value))
}

fn read_expiry(file: &mut std::fs::File) -> std::result::Result<u64, CacheError> {
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

    Ok(u64::from_be_bytes(
        header[CACHE_MAGIC.len()..]
            .try_into()
            .expect("cache expiry occupies eight bytes"),
    ))
}

fn read_expiry_at(path: &Path) -> std::result::Result<u64, CacheError> {
    let mut file = std::fs::File::open(path)?;
    read_expiry(&mut file)
}

/// Result of a [`Cache::clear_report`] operation.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ClearReport {
    /// Number of cache entries successfully removed.
    pub removed: usize,
    /// Paths that could not be removed (permissions, in-use, etc.).
    pub failed: Vec<PathBuf>,
}

/// Acquire an advisory lock on the cache directory (blocking).
///
/// Returns `None` if the lock file cannot be opened or locked; callers
/// treat a failed lock as non-fatal so the cache remains usable on
/// filesystems that do not support advisory locking.
fn acquire_cache_lock(dir: &Path) -> Option<File> {
    let lock_path = dir.join(".cache.lock");
    let mut options = File::options();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let file = options.open(lock_path).ok()?;
    file.lock().ok()?;
    Some(file)
}

fn is_v2_cache_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("v2-") && name.ends_with(".cache"))
}

fn write_entry(path: &Path, header: &[u8], parts: &[&[u8]]) -> std::io::Result<()> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(".librebar-cache-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        builder.permissions(std::fs::Permissions::from_mode(0o600));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut file = builder.tempfile_in(parent)?;
    file.write_all(header)?;
    for part in parts {
        file.write_all(part)?;
    }
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// Get the default cache directory for an application.
///
/// - macOS: `~/Library/Caches/{app}/librebar/`
/// - Linux: `$XDG_CACHE_HOME/{app}/librebar/`
pub fn default_cache_dir(app_name: &str) -> Option<PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Some(proj_dirs.cache_dir().join("librebar"))
}

#[cfg(all(test, any(feature = "http-cache", feature = "update")))]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn blocking_io_does_not_stall_current_thread_runtime() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (timer_tx, timer_rx) = mpsc::sync_channel(1);

        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let blocking = super::run_io(move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                });
                let timer = async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    timer_tx.send(()).unwrap();
                };

                let (result, ()) = tokio::join!(blocking, timer);
                result.unwrap();
            });
        });

        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let timer_ran = timer_rx.recv_timeout(Duration::from_millis(250)).is_ok();
        release_tx.send(()).unwrap();
        worker.join().unwrap();

        assert!(timer_ran, "cache I/O stalled the current-thread runtime");
    }
}
