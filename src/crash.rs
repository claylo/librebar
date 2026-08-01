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

use std::path::{Path, PathBuf};
use std::{fs::OpenOptions, io::Write as _};

const MAX_CRASH_DUMPS: usize = 10;

// ─── Public API ─────────────────────────────────────────────────────

/// Structured crash information captured at panic time.
#[derive(Debug)]
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

        let info = CrashInfo {
            message,
            location,
            app_name: app_name.clone(),
            version: version.clone(),
            timestamp: crate::time::format_timestamp(),
            os: std::env::consts::OS.to_string(),
            backtrace,
        };

        let dump_dir = crash_dump_dir(&app_name);
        if let Some(path) = write_crash_dump_to(&info, &dump_dir) {
            eprintln!(
                "\n{} crashed. Crash report written to: {}\n",
                app_name,
                path.display()
            );
        } else {
            eprintln!("\n{} crashed. (Could not write crash report.)\n", app_name);
        }

        prev_hook(panic_info);
    }));
}

/// Write a crash dump to a file in `dir`.
///
/// The file is named with a timestamp and `.crash` extension. Files are
/// owner-only on Unix, existing same-timestamp files are not replaced, and
/// only the ten newest crash dumps in `dir` are retained.
/// Returns the path to the written file, or `None` if writing failed.
pub fn write_crash_dump_to(info: &CrashInfo, dir: &Path) -> Option<PathBuf> {
    if std::fs::create_dir_all(dir).is_err() {
        return None;
    }

    // Use timestamp chars that are safe in filenames
    let ts = info.timestamp.replace([':', '.'], "-");
    let filename = format!("{}-{}.crash", info.app_name, ts);
    let path = dir.join(&filename);

    let content = info.format();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }

    let mut file = options.open(&path).ok()?;
    if file.write_all(content.as_bytes()).is_err() {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return None;
    }
    drop(file);

    if prune_crash_dumps(dir).is_err() {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(path)
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
