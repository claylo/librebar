//! Panic hook with structured crash dumps.
//!
//! Installs a custom panic hook that captures the panic message, backtrace,
//! location, and OS info, writes a structured crash dump to the XDG cache
//! directory, and chains to the previous hook to preserve default behavior.
//!
//! # Usage
//!
//! ```ignore
//! let app = librebar::init(env!("CARGO_PKG_NAME"))
//!     .crash_handler()
//!     .start()?;
//! ```
//!
//! Or install the hook directly (escape hatch):
//!
//! ```ignore
//! librebar::crash::install("myapp", env!("CARGO_PKG_VERSION"));
//! ```

use std::path::{Path, PathBuf};

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
            timestamp: format_timestamp(),
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
/// The file is named with a timestamp and `.crash` extension.
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
    std::fs::write(&path, content).ok()?;
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

fn extract_panic_message(panic_info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = panic_info.payload().downcast_ref::<String>() {
        return s.clone();
    }
    "<unknown panic payload>".to_string()
}

/// Format timestamp as RFC 3339 UTC using std::time only.
///
/// Duplicated from logging.rs — crash must remain standalone with no feature
/// dependency on logging.
fn format_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = now.as_secs();
    let nanos = now.subsec_nanos();

    let days_since_epoch = secs / 86400;
    let secs_of_day = secs % 86400;
    let hours = secs_of_day / 3600;
    let minutes = (secs_of_day % 3600) / 60;
    let seconds = secs_of_day % 60;

    let (year, month, day) = days_to_ymd(days_since_epoch as i64);

    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z",
        millis = nanos / 1_000_000
    )
}

/// Convert days since Unix epoch to (year, month, day).
///
/// Uses Howard Hinnant's civil calendar algorithm.
const fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}
