//! Exclusive operation locking via file locks.
//!
//! Provides a simple advisory lock backed by a file descriptor. Two instances
//! of the same application cannot hold the lock simultaneously — ideal for
//! preventing concurrent runs of background daemons, update checkers, etc.
//!
//! # Example
//!
//! ```no_run
//! use librebar::lockfile::Lockfile;
//!
//! let lock = Lockfile::default_for("my-app")?;
//! let _guard = lock.try_acquire()?;
//! // Exclusive section — guard released when dropped.
//! # Ok::<(), librebar::Error>(())
//! ```

use std::fs::File;
use std::path::{Path, PathBuf};

use fs4::fs_std::FileExt;

use crate::{Error, Result};

// ─── Platform lock directory ─────────────────────────────────────────

/// Returns the platform-appropriate directory for lock files.
///
/// - macOS / other: `$TMPDIR/{app_name}/`
/// - Linux: `$XDG_RUNTIME_DIR/{app_name}/` falling back to `/tmp/{app_name}/`
pub fn default_lock_dir(app_name: &str) -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let base = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        base.join(app_name)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let base = std::env::var("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        base.join(app_name)
    }
}

// ─── Lockfile ────────────────────────────────────────────────────────

/// A handle to a named lock file.
///
/// Use [`Lockfile::try_acquire`] to obtain an exclusive [`LockGuard`].
/// The lock is released automatically when the guard is dropped.
#[derive(Debug, Clone)]
pub struct Lockfile {
    path: PathBuf,
}

impl Lockfile {
    /// Create a `Lockfile` targeting a specific directory.
    ///
    /// The lock file will be named `{app_name}.lock` inside `dir`.
    pub fn new(app_name: &str, dir: &Path) -> Self {
        Self {
            path: dir.join(format!("{app_name}.lock")),
        }
    }

    /// Create a `Lockfile` in the default platform lock directory.
    ///
    /// The directory is created if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the lock directory cannot be created.
    pub fn default_for(app_name: &str) -> Result<Self> {
        let dir = default_lock_dir(app_name);
        std::fs::create_dir_all(&dir)?;
        Ok(Self::new(app_name, &dir))
    }

    /// Returns the path to the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Try to acquire exclusive access.
    ///
    /// Returns a [`LockGuard`] on success. While the guard is alive no other
    /// process using this crate can acquire the same lock.
    ///
    /// # Errors
    ///
    /// - [`Error::Io`] if the lock file cannot be created or opened.
    /// - [`Error::Lock`] if another process already holds the lock.
    pub fn try_acquire(&self) -> Result<LockGuard> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)?;

        file.try_lock_exclusive().map_err(|e| {
            Error::Lock(std::io::Error::new(
                e.kind(),
                format!("another instance holds the lock: {}", self.path.display()),
            ))
        })?;

        tracing::debug!(path = %self.path.display(), "lock acquired");

        Ok(LockGuard {
            _file: file,
            path: self.path.clone(),
        })
    }
}

// ─── LockGuard ───────────────────────────────────────────────────────

/// RAII guard that holds an exclusive lock on a [`Lockfile`].
///
/// The lock is released when this value is dropped — the OS releases
/// the file lock when the file descriptor is closed.
#[derive(Debug)]
pub struct LockGuard {
    /// Held open to maintain the OS-level file lock.
    _file: File,
    path: PathBuf,
}

impl LockGuard {
    /// Returns the path to the lock file held by this guard.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        tracing::debug!(path = %self.path.display(), "lock released");
    }
}
