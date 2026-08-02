//! Git-style external command dispatch.
//!
//! Resolves `{app}-{subcommand}` binaries on PATH and executes them,
//! enabling a plugin model where external tools extend the main CLI.
//!
//! # Example
//!
//! ```no_run
//! # fn main() -> librebar::Result<()> {
//! // Typical use: in the match arm for an unknown subcommand.
//! let args: Vec<String> = std::env::args().skip(2).collect();
//! match librebar::dispatch::run("myapp", "deploy", &args)? {
//!     Some(status) => std::process::exit(status.code().unwrap_or(1)),
//!     None => eprintln!("unknown command: deploy"),
//! }
//! # Ok(())
//! # }
//! ```

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use crate::error::{Error, Result};

/// Errors during subcommand resolution.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DispatchError {
    /// PATH environment variable is not set.
    #[error("PATH is not set")]
    PathNotSet,
    /// No absolute PATH entries contain the binary.
    #[error("no absolute PATH entries available")]
    PathJoinFailed,
    /// Could not determine the current directory.
    #[error("could not determine current directory")]
    CurrentDirFailed(#[source] std::io::Error),
    /// Binary was not found on any absolute PATH entry.
    #[error("{binary} not found on PATH")]
    NotFound {
        /// The binary name that was searched for.
        binary: String,
    },
}

/// Construct the expected binary name for a subcommand.
///
/// Returns `"{app_name}-{subcommand}"`.
pub fn subcommand_binary(app_name: &str, subcommand: &str) -> String {
    format!("{app_name}-{subcommand}")
}

/// Resolve the full path to a subcommand binary on PATH, returning typed errors.
///
/// Empty and relative PATH entries are ignored so dispatch never resolves a
/// plugin from the process working directory.
///
/// # Errors
///
/// Returns a [`DispatchError`] variant describing why resolution failed.
pub fn try_resolve(app_name: &str, subcommand: &str) -> std::result::Result<PathBuf, DispatchError> {
    let binary = subcommand_binary(app_name, subcommand);
    let path = std::env::var_os("PATH").ok_or(DispatchError::PathNotSet)?;
    let absolute_paths: Vec<_> = std::env::split_paths(&path)
        .filter(|entry| entry.is_absolute())
        .collect();
    if absolute_paths.is_empty() {
        return Err(DispatchError::PathJoinFailed);
    }

    let path = std::env::join_paths(absolute_paths).map_err(|_| DispatchError::PathJoinFailed)?;
    let cwd = std::env::current_dir().map_err(DispatchError::CurrentDirFailed)?;
    which::which_in(&binary, Some(path), cwd)
        .ok()
        .filter(|resolved| resolved.is_absolute())
        .ok_or(DispatchError::NotFound { binary })
}

/// Resolve the full path to a subcommand binary on PATH.
///
/// Empty and relative PATH entries are ignored so dispatch never resolves a
/// plugin from the process working directory. Returns `None` if the binary is
/// not found on an absolute PATH entry.
pub fn resolve(app_name: &str, subcommand: &str) -> Option<PathBuf> {
    try_resolve(app_name, subcommand).ok()
}

/// Run an external subcommand, passing through arguments.
///
/// Returns `Ok(Some(ExitStatus))` if the binary was found and executed.
/// Returns `Ok(None)` if the binary was not found on PATH.
///
/// # Errors
///
/// Returns [`Error::Dispatch`] if the binary exists but fails to execute
/// (permission denied, invalid binary, etc.).
#[tracing::instrument(skip(args), fields(app = %app_name, subcommand = %subcommand))]
pub fn run<I, S>(app_name: &str, subcommand: &str, args: I) -> Result<Option<ExitStatus>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let binary_path = match try_resolve(app_name, subcommand) {
        Ok(path) => path,
        Err(DispatchError::NotFound { .. }) => return Ok(None),
        Err(error) => {
            return Err(Error::Dispatch(std::io::Error::other(error)))
        }
    };

    tracing::debug!(binary = %binary_path.display(), "dispatching to external command");

    let status = Command::new(&binary_path)
        .args(args)
        .status()
        .map_err(|e| {
            Error::Dispatch(std::io::Error::new(
                e.kind(),
                format!("failed to execute {}: {e}", binary_path.display()),
            ))
        })?;

    tracing::debug!(exit_code = ?status.code(), "external command finished");
    Ok(Some(status))
}
