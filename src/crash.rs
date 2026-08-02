//! Panic hook with structured crash dumps.
//!
//! Installs a custom panic hook that captures the panic message, backtrace,
//! location, and OS info, writes a structured crash dump to the XDG cache
//! directory, and chains to the previous hook to preserve default behavior.
//! Dumps may contain sensitive panic payloads, backtraces, and source paths;
//! files are owner-only on Unix and only the ten newest dumps are retained.
//!
//! # Usage
//!
//! ```no_run
//! # fn main() -> librebar::Result<()> {
//! let app = librebar::init(env!("CARGO_PKG_NAME"))
//!     .crash_handler()
//!     .start()?;
//! # let _ = app;
//! # Ok(())
//! # }
//! ```
//!
//! Or install the hook directly (escape hatch):
//!
//! ```no_run
//! librebar::crash::install("myapp", env!("CARGO_PKG_VERSION"));
//! ```

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

const MAX_CRASH_DUMPS: usize = 10;

// ─── Public API ─────────────────────────────────────────────────────

/// Structured crash information captured at panic time.
#[derive(Debug, serde::Serialize)]
#[non_exhaustive]
pub struct CrashInfo {
    /// The panic message (from the panic payload).
    pub message: String,
    /// Source location, e.g. `"src/main.rs:42"`.
    pub location: Option<String>,
    /// Application name.
    pub app_name: String,
    /// Application version.
    pub version: String,
    /// RFC 3339 UTC timestamp.
    pub timestamp: String,
    /// Operating system (e.g., `"macos"`, `"linux"`).
    pub os: String,
    /// Captured backtrace.
    pub backtrace: String,
}

impl CrashInfo {
    /// Create crash information with the current timestamp and operating system.
    pub fn new(
        message: impl Into<String>,
        app_name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            location: None,
            app_name: app_name.into(),
            version: version.into(),
            timestamp: crate::time::format_timestamp(),
            os: std::env::consts::OS.to_string(),
            backtrace: String::new(),
        }
    }

    /// Set the source location.
    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Override the timestamp, primarily when importing existing crash data.
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.timestamp = timestamp.into();
        self
    }

    /// Override the operating-system identifier.
    #[must_use]
    pub fn with_os(mut self, os: impl Into<String>) -> Self {
        self.os = os.into();
        self
    }

    /// Attach a captured backtrace.
    #[must_use]
    pub fn with_backtrace(mut self, backtrace: impl Into<String>) -> Self {
        self.backtrace = backtrace.into();
        self
    }

    /// Format into a human-readable crash report string.
    pub fn format(&self) -> String {
        let location = self.location.as_deref().unwrap_or("<unknown location>");

        let mut report = format!(
            "=== Crash Report ===\n\
             App:       {} {}\n\
             Timestamp: {}\n\
             OS:        {}\n\
             Location:  {}\n\
             Message:   {}\n",
            self.app_name, self.version, self.timestamp, self.os, location, self.message,
        );

        if !self.backtrace.is_empty() {
            report.push_str("\n--- Backtrace ---\n");
            report.push_str(&self.backtrace);
            report.push('\n');
        }

        report.push_str("=== End Crash Report ===\n");
        report
    }
}

/// Errors during crash dump writing.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CrashDumpError {
    /// Failed to create the crash dump directory.
    #[error("failed to create crash dump directory")]
    CreateDir(#[source] std::io::Error),
    /// Failed to open the crash dump file.
    #[error("failed to open crash dump file")]
    OpenFile(#[source] std::io::Error),
    /// Failed to serialize crash information.
    #[error("failed to serialize crash info")]
    Serialize(#[source] serde_json::Error),
    /// Failed to prune old crash dumps.
    #[error("failed to prune crash dumps")]
    Prune(#[source] std::io::Error),
}

/// Install a custom panic hook that captures crash info and writes a dump.
///
/// Chains with the previous panic hook so default behavior (e.g., printing
/// the panic message to stderr) is preserved.
pub fn install(app_name: &str, version: &str) {
    let app_name = app_name.to_string();
    let version = version.to_string();

    let prev_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        let message = extract_panic_message(panic_info);
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()));
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();

        let mut info =
            CrashInfo::new(message, app_name.clone(), version.clone()).with_backtrace(backtrace);
        if let Some(location) = location {
            info = info.with_location(location);
        }

        let dump_dir = crash_dump_dir(&app_name);
        let dump_path = write_crash_dump_to(&info, &dump_dir);
        write_crash_notice(std::io::stderr().lock(), &app_name, dump_path.as_deref());

        prev_hook(panic_info);
    }));
}

/// Write a crash dump to a file in `dir`, returning typed errors on failure.
///
/// The file is named with a timestamp and `.crash` extension. Files are
/// owner-only on Unix, existing same-timestamp files are not replaced, and
/// only the ten newest crash dumps in `dir` are retained.
pub fn try_write_crash_dump_to(
    info: &CrashInfo,
    dir: &Path,
) -> std::result::Result<PathBuf, CrashDumpError> {
    std::fs::create_dir_all(dir).map_err(CrashDumpError::CreateDir)?;

    let ts = info.timestamp.replace([':', '.'], "-");
    let filename = format!("{}-{}.crash", info.app_name, ts);
    let path = dir.join(&filename);

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }

    let mut file = options.open(&path).map_err(CrashDumpError::OpenFile)?;
    if let Err(error) = serde_json::to_writer(&mut file, info) {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(CrashDumpError::Serialize(error));
    }
    drop(file);

    prune_crash_dumps(dir).map_err(CrashDumpError::Prune)?;
    Ok(path)
}

/// Write a crash dump to a file in `dir`.
///
/// Convenience wrapper around [`try_write_crash_dump_to`] that discards the error.
/// Returns the path to the written file, or `None` if writing failed.
pub fn write_crash_dump_to(info: &CrashInfo, dir: &Path) -> Option<PathBuf> {
    try_write_crash_dump_to(info, dir).ok()
}

/// Return the platform-appropriate crash dump directory for an app.
///
/// - macOS: `~/Library/Caches/{app}/crashes/`
/// - Linux: `$XDG_CACHE_HOME/{app}/crashes/` (default `~/.cache/{app}/crashes/`)
/// - Fallback: `$TMPDIR/{app}/crashes/` (or `/tmp/{app}/crashes/`)
pub fn crash_dump_dir(app_name: &str) -> PathBuf {
    if let Some(dir) = platform_cache_dir(app_name) {
        return dir;
    }

    // Fallback: use TMPDIR or /tmp
    let tmp = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    tmp.join(app_name).join("crashes")
}

// ─── Internal ───────────────────────────────────────────────────────

fn write_crash_notice(mut writer: impl std::io::Write, app_name: &str, path: Option<&Path>) {
    let result = match path {
        Some(path) => writeln!(
            writer,
            "\n{app_name} crashed. Crash report written to: {}\n",
            path.display()
        ),
        None => writeln!(
            writer,
            "\n{app_name} crashed. (Could not write crash report.)\n"
        ),
    };
    let _ = result;
}

fn platform_cache_dir(app_name: &str) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Caches")
                .join(app_name)
                .join("crashes"),
        )
    } else {
        // Linux / other Unix: use XDG_CACHE_HOME or ~/.cache
        let cache_base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
        Some(cache_base.join(app_name).join("crashes"))
    }
}

fn prune_crash_dumps(dir: &Path) -> std::io::Result<()> {
    let mut dumps = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry.path().extension() == Some(std::ffi::OsStr::new("crash"))
        {
            dumps.push(entry.path());
        }
    }

    dumps.sort();
    let remove_count = dumps.len().saturating_sub(MAX_CRASH_DUMPS);
    for path in dumps.into_iter().take(remove_count) {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    Ok(())
}

fn extract_panic_message(panic_info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = panic_info.payload().downcast_ref::<String>() {
        return s.clone();
    }
    "<unknown panic payload>".to_string()
}

#[cfg(test)]
mod tests {
    use std::io::{Error, ErrorKind, Write};

    use super::*;

    struct BrokenWriter;

    impl Write for BrokenWriter {
        fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
            Err(Error::new(ErrorKind::BrokenPipe, "stderr is closed"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(Error::new(ErrorKind::BrokenPipe, "stderr is closed"))
        }
    }

    #[test]
    fn crash_notice_ignores_writer_failures() {
        write_crash_notice(
            &mut BrokenWriter,
            "test-app",
            Some(Path::new("/tmp/test-app.crash")),
        );
        write_crash_notice(&mut BrokenWriter, "test-app", None);
    }
}
