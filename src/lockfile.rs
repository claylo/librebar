//! Advisory operation locking via file locks.
//!
//! Provides a simple advisory lock backed by a file descriptor. On a local
//! filesystem that implements the platform's locking semantics, two processes
//! using the same lock path cannot hold the lock simultaneously.
//!
//! # Platform behavior
//!
//! File locking is advisory and filesystem-dependent. Use a local filesystem
//! that supports the operating system's file-locking API. Network, FUSE, and
//! overlay filesystems may reject locking or fail to provide mutual exclusion;
//! callers that pass a directory to [`Lockfile::new`] are responsible for that
//! directory's locking guarantees. The lock is released when the guard's file
//! descriptor closes, so a lock file left on disk is not a stale held lock.
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

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

use crate::{Error, Result};

// ─── Platform lock directory ─────────────────────────────────────────

/// Returns the platform-appropriate directory for lock files.
///
/// - macOS / other: `$TMPDIR/{app_name}/`
/// - Linux: `$XDG_RUNTIME_DIR/{app_name}/`, falling back to
///   `$XDG_STATE_HOME/{app_name}/` or `~/.local/state/{app_name}/`
///
/// # Errors
///
/// Returns [`Error::Io`] on Linux when no per-user runtime or state directory
/// can be resolved. Librebar does not fall back to a shared temporary
/// directory because another local user could pre-create or hold that path.
pub fn default_lock_dir(app_name: &str) -> Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let base_dirs = directories::BaseDirs::new();
        let runtime_dir = base_dirs.as_ref().and_then(|dirs| dirs.runtime_dir());
        let state_dir = base_dirs.as_ref().and_then(|dirs| dirs.state_dir());
        linux_lock_dir(app_name, runtime_dir, state_dir).map_err(Error::from)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let base = std::env::var("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        Ok(base.join(app_name))
    }
}

#[cfg(any(target_os = "linux", test))]
fn linux_lock_dir(
    app_name: &str,
    runtime_dir: Option<&Path>,
    state_dir: Option<&Path>,
) -> std::io::Result<PathBuf> {
    runtime_dir
        .or(state_dir)
        .map(|base| base.join(app_name))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no per-user runtime or state directory available for lockfile",
            )
        })
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
    /// Returns [`Error::Io`] if no secure per-user directory is available or
    /// if the lock directory cannot be created.
    pub fn default_for(app_name: &str) -> Result<Self> {
        let dir = default_lock_dir(app_name)?;
        std::fs::create_dir_all(&dir)?;
        Ok(Self::new(app_name, &dir))
    }

    /// Returns the path to the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Try to acquire exclusive access.
    ///
    /// Returns a [`LockGuard`] on success. On filesystems that implement the
    /// platform's advisory-locking semantics, no other process using the same
    /// lock path can acquire it while the guard is alive.
    ///
    /// # Errors
    ///
    /// - [`Error::Io`] if the lock file cannot be created or opened.
    /// - [`Error::LockContended`] if another process already holds the lock.
    /// - [`Error::Lock`] if the operating system cannot acquire the lock.
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

        file.try_lock()
            .map_err(|error| map_lock_error(error, &self.path))?;

        tracing::debug!(path = %self.path.display(), "lock acquired");

        Ok(LockGuard {
            _file: file,
            path: self.path.clone(),
        })
    }
}

fn map_lock_error(error: TryLockError, path: &Path) -> Error {
    match error {
        TryLockError::WouldBlock => Error::LockContended {
            path: path.to_owned(),
        },
        TryLockError::Error(source) => Error::Lock(source),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;

    #[test]
    fn linux_lock_dir_falls_back_to_per_user_state() {
        let state = Path::new("/home/test/.local/state");

        let path = linux_lock_dir("test-app", None, Some(state)).unwrap();

        assert_eq!(path, state.join("test-app"));
    }

    #[test]
    fn linux_lock_dir_rejects_shared_temporary_fallback() {
        let error = linux_lock_dir("test-app", None, None).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn genuine_lock_errors_preserve_the_io_failure() {
        let error = std::io::Error::new(ErrorKind::PermissionDenied, "locking unavailable");

        let mapped = map_lock_error(
            std::fs::TryLockError::Error(error),
            Path::new("/tmp/example.lock"),
        );

        let Error::Lock(source) = mapped else {
            panic!("expected Error::Lock, got: {mapped:?}");
        };
        assert_eq!(source.kind(), ErrorKind::PermissionDenied);
        assert_eq!(source.to_string(), "locking unavailable");
    }
}
