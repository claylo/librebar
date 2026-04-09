//! Git-style external command dispatch.
//!
//! Resolves `{app}-{subcommand}` binaries on PATH and executes them,
//! enabling a plugin model where external tools extend the main CLI.
//!
//! # Example
//!
//! ```ignore
//! // In the match arm for unknown subcommands:
//! match rebar::dispatch::run("myapp", "deploy", &args)? {
//!     Some(status) => std::process::exit(status.code().unwrap_or(1)),
//!     None => eprintln!("unknown command: deploy"),
//! }
//! ```

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use crate::error::{Error, Result};

/// Construct the expected binary name for a subcommand.
///
/// Returns `"{app_name}-{subcommand}"`.
pub fn subcommand_binary(app_name: &str, subcommand: &str) -> String {
    format!("{app_name}-{subcommand}")
}

/// Resolve the full path to a subcommand binary on PATH.
///
/// Returns `None` if the binary is not found.
pub fn resolve(app_name: &str, subcommand: &str) -> Option<PathBuf> {
    let binary = subcommand_binary(app_name, subcommand);
    which::which(&binary).ok()
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
    let Some(binary_path) = resolve(app_name, subcommand) else {
        return Ok(None);
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
