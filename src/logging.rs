//! Structured JSONL logging with daily rotation.
//!
//! Provides a custom tracing Layer that writes JSONL to file with daily
//! rotation. All logging goes to files or stderr — never stdout, which
//! is reserved for application output (e.g., MCP server communication).
//!
//! # JSONL output schema
//!
//! Each log line is a JSON object with these fields:
//! - `timestamp` — RFC 3339 UTC (e.g., `2026-04-08T15:30:00.123Z`)
//! - `level` — `trace`, `debug`, `info`, `warn`, or `error`
//! - `target` — Rust module path of the log callsite
//! - `message` — the formatted log message (from `tracing::info!("...")`)
//! - Plus any structured fields from spans and events
//!
//! # Log directory resolution
//!
//! Priority (first writable wins):
//! 1. `{APP}_LOG_PATH` env var — exact file path
//! 2. `{APP}_LOG_DIR` env var — directory, file name derived from service
//! 3. Config `log_dir` — from the application's config struct
//! 4. Platform default — `~/Library/Logs/{app}/` on macOS,
//!    `$XDG_STATE_HOME/{app}/logs/` on Linux
//! 5. `/var/log` on Unix
//! 6. Current working directory (last resort)
//! 7. stderr fallback if nothing is writable

use serde_json::{Map, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::Event;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::Context as LayerContext;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

use crate::error::Result;

const LOG_FILE_SUFFIX: &str = ".jsonl";
const DEFAULT_LOG_DIR_UNIX: &str = "/var/log";

// ─── Public API ─────────────────────────────────────────────────────

/// Configuration for logging setup.
#[derive(Clone, Debug)]
pub struct LoggingConfig {
    /// Service name used in log file names.
    pub service: String,
    /// Env var name for log file path override (e.g., `MYAPP_LOG_PATH`).
    pub env_log_path: String,
    /// Env var name for log directory override (e.g., `MYAPP_LOG_DIR`).
    pub env_log_dir: String,
    /// Directory for JSONL log files (from config). Falls back to platform defaults.
    pub log_dir: Option<PathBuf>,
}

impl LoggingConfig {
    /// Create a logging config from an application name.
    ///
    /// Derives env var names as `{APP}_LOG_PATH` and `{APP}_LOG_DIR`
    /// where `{APP}` is the uppercased, hyphen-to-underscore service name.
    pub fn from_app_name(app_name: &str) -> Self {
        let prefix = app_name.to_uppercase().replace('-', "_");
        Self {
            service: app_name.to_string(),
            env_log_path: format!("{prefix}_LOG_PATH"),
            env_log_dir: format!("{prefix}_LOG_DIR"),
            log_dir: None,
        }
    }

    /// Set the log directory from config.
    pub fn with_log_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.log_dir = dir;
        self
    }
}

/// Guard that must be held for the application lifetime to ensure logs flush.
pub struct LoggingGuard {
    _log_guard: tracing_appender::non_blocking::WorkerGuard,
}

impl LoggingGuard {
    /// Create a guard from a raw worker guard (used by the builder).
    pub(crate) const fn from_guard(guard: tracing_appender::non_blocking::WorkerGuard) -> Self {
        Self { _log_guard: guard }
    }
}

/// Initialize logging and return a guard that must be held.
///
/// This is the standalone escape hatch — it builds the JSON layer and
/// initializes the global subscriber in one call. For composing multiple
/// layers (e.g., logging + OTEL), the builder uses an internal layer
/// construction function instead.
///
/// # Errors
///
/// Falls back to stderr if no writable log directory is found.
pub fn init(cfg: &LoggingConfig, env_filter: EnvFilter) -> Result<LoggingGuard> {
    let (log_layer, log_guard) = build_json_layer(cfg)?;

    tracing_subscriber::registry()
        .with(env_filter)
        .with(log_layer)
        .init();

    tracing::debug!("logging initialized");

    Ok(LoggingGuard {
        _log_guard: log_guard,
    })
}

/// Build the JSON log layer and its writer guard without initializing
/// the global subscriber.
///
/// Used internally by the builder to compose multiple layers (logging + OTEL)
/// on one registry. For standalone use, prefer [`init()`].
pub(crate) fn build_json_layer(
    cfg: &LoggingConfig,
) -> Result<(
    JsonLogLayer<tracing_appender::non_blocking::NonBlocking>,
    tracing_appender::non_blocking::WorkerGuard,
)> {
    let (log_writer, log_guard) = match build_log_writer(
        &cfg.service,
        &cfg.env_log_path,
        &cfg.env_log_dir,
        cfg.log_dir.as_deref(),
    ) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("Warning: {err}. Falling back to stderr logging.");
            tracing_appender::non_blocking(std::io::stderr())
        }
    };

    let log_layer = JsonLogLayer::new(log_writer);
    Ok((log_layer, log_guard))
}

/// Build an `EnvFilter` based on CLI flags and environment.
///
/// Priority: quiet flag > verbose flag > `RUST_LOG` env > default_level.
pub fn env_filter(quiet: bool, verbose: u8, default_level: &str) -> EnvFilter {
    if quiet {
        return EnvFilter::new("error");
    }

    if verbose > 0 {
        let level = match verbose {
            1 => "debug",
            _ => "trace",
        };
        return EnvFilter::new(level);
    }

    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level))
}

// ─── Log Target Resolution ──────────────────────────────────────────

/// Resolved log file location.
#[derive(Clone, Debug)]
pub struct LogTarget {
    /// Directory containing the log file.
    pub dir: PathBuf,
    /// Log file name.
    pub file_name: String,
}

/// Resolve log target with explicit overrides (for testing).
///
/// Priority: path_override > dir_override > config_dir > platform default.
///
/// # Errors
///
/// Returns a descriptive error string if no writable log directory is found.
pub fn resolve_log_target_with(
    service: &str,
    path_override: Option<PathBuf>,
    dir_override: Option<PathBuf>,
    config_dir: Option<PathBuf>,
) -> std::result::Result<LogTarget, String> {
    if let Some(path) = path_override {
        return log_target_from_path(path);
    }

    if let Some(dir) = dir_override {
        return log_target_from_dir(dir, service);
    }

    if let Some(dir) = config_dir {
        return log_target_from_dir(dir, service);
    }

    let mut candidates = Vec::new();

    if let Some(log_dir) = platform_log_dir(service) {
        candidates.push(log_dir);
    }

    if cfg!(unix) {
        candidates.push(PathBuf::from(DEFAULT_LOG_DIR_UNIX));
    }

    if let Ok(dir) = std::env::current_dir() {
        candidates.push(dir);
    }

    let file_name = format!("{service}{LOG_FILE_SUFFIX}");

    for dir in candidates {
        if ensure_writable(&dir, &file_name).is_ok() {
            return Ok(LogTarget { dir, file_name });
        }
    }

    Err("No writable log directory found".to_string())
}

/// Resolve the platform-appropriate log directory for a service.
///
/// - macOS: `~/Library/Logs/{service}/`
/// - Linux/BSD: `$XDG_STATE_HOME/{service}/logs/`
/// - Windows: `{LocalAppData}/{service}/logs/`
pub fn platform_log_dir(service: &str) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs").join(service))
    } else if cfg!(unix) {
        let state_base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })?;
        Some(state_base.join(service).join("logs"))
    } else {
        directories::ProjectDirs::from("", "", service).map(|p| p.data_local_dir().join("logs"))
    }
}

/// Format timestamp as RFC 3339 using std::time (no external time crate).
pub fn format_timestamp() -> String {
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
/// Reference: <https://howardhinnant.github.io/date_algorithms.html#civil_from_days>
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

// ─── Internal ───────────────────────────────────────────────────────

/// Build a non-blocking log writer by resolving the log target from env vars
/// and config, then wrapping a daily-rolling file appender in a non-blocking writer.
fn build_log_writer(
    service: &str,
    env_log_path: &str,
    env_log_dir: &str,
    config_log_dir: Option<&Path>,
) -> std::result::Result<
    (
        tracing_appender::non_blocking::NonBlocking,
        tracing_appender::non_blocking::WorkerGuard,
    ),
    String,
> {
    let path_override = std::env::var_os(env_log_path).map(PathBuf::from);
    let dir_override = std::env::var_os(env_log_dir).map(PathBuf::from);

    let target = resolve_log_target_with(
        service,
        path_override,
        dir_override,
        config_log_dir.map(PathBuf::from),
    )?;

    let appender = tracing_appender::rolling::daily(&target.dir, &target.file_name);
    let (writer, guard) = tracing_appender::non_blocking(appender);

    Ok((writer, guard))
}

fn log_target_from_dir(dir: PathBuf, service: &str) -> std::result::Result<LogTarget, String> {
    let file_name = format!("{service}{LOG_FILE_SUFFIX}");
    ensure_writable(&dir, &file_name)?;
    Ok(LogTarget { dir, file_name })
}

fn log_target_from_path(path: PathBuf) -> std::result::Result<LogTarget, String> {
    let file_name = path
        .file_name()
        .ok_or_else(|| "log path must include a file name".to_string())
        .and_then(|name| {
            name.to_str()
                .map(|v| v.to_string())
                .ok_or_else(|| "log path must be valid UTF-8".to_string())
        })?;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    ensure_writable(dir, &file_name)?;

    Ok(LogTarget {
        dir: dir.to_path_buf(),
        file_name,
    })
}

/// Verify a directory is writable by creating it (if needed) and opening a file for append.
fn ensure_writable(dir: &Path, file_name: &str) -> std::result::Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create log directory {}: {e}", dir.display()))?;

    let path = dir.join(file_name);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Failed to open log file {}: {e}", path.display()))?;

    Ok(())
}

// ─── JSON Log Layer ─────────────────────────────────────────────────

/// Custom tracing Layer that serializes events and span context to JSONL.
///
/// Each event becomes one JSON object per line. Span fields from the
/// current scope are flattened into the event object (root-to-leaf order,
/// later fields win on collision).
pub(crate) struct JsonLogLayer<W> {
    writer: W,
}

impl<W> JsonLogLayer<W> {
    const fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<S, W> tracing_subscriber::Layer<S> for JsonLogLayer<W>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: LayerContext<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = JsonVisitor::default();
            attrs.record(&mut visitor);
            span.extensions_mut().insert(SpanFields {
                values: visitor.values,
            });
        }
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: LayerContext<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = JsonVisitor::default();
            values.record(&mut visitor);
            let mut extensions = span.extensions_mut();
            if let Some(fields) = extensions.get_mut::<SpanFields>() {
                fields.values.extend(visitor.values);
            } else {
                extensions.insert(SpanFields {
                    values: visitor.values,
                });
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: LayerContext<'_, S>) {
        let mut map = Map::new();

        let timestamp = format_timestamp();
        map.insert("timestamp".to_string(), Value::String(timestamp));
        map.insert(
            "level".to_string(),
            Value::String(event.metadata().level().as_str().to_lowercase()),
        );
        map.insert(
            "target".to_string(),
            Value::String(event.metadata().target().to_string()),
        );

        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if let Some(fields) = span.extensions().get::<SpanFields>() {
                    map.extend(fields.values.clone());
                }
            }
        }

        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);
        map.extend(visitor.values);

        if let Ok(mut buf) = serde_json::to_vec(&Value::Object(map)) {
            buf.push(b'\n');
            let mut writer = self.writer.make_writer();
            let _ = writer.write_all(&buf);
        }
    }
}

/// Span extension data: the accumulated key-value fields recorded on a span.
/// Stored in span extensions and cloned into each event emitted within the span.
#[derive(Clone, Debug)]
struct SpanFields {
    values: Map<String, Value>,
}

/// Visitor that collects tracing fields into a JSON map.
/// Used by both span creation (on_new_span) and event recording (on_event).
#[derive(Default)]
struct JsonVisitor {
    values: Map<String, Value>,
}

impl tracing::field::Visit for JsonVisitor {
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.values
            .insert(field.name().to_string(), Value::Bool(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.values
            .insert(field.name().to_string(), Value::Number(value.into()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.values
            .insert(field.name().to_string(), Value::Number(value.into()));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        if let Some(number) = serde_json::Number::from_f64(value) {
            self.values
                .insert(field.name().to_string(), Value::Number(number));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.values
            .insert(field.name().to_string(), Value::String(value.to_string()));
    }

    fn record_error(
        &mut self,
        _field: &tracing::field::Field,
        _value: &(dyn std::error::Error + 'static),
    ) {
        // Errors are captured via record_debug — no separate handling needed.
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.values.insert(
            field.name().to_string(),
            Value::String(format!("{value:?}")),
        );
    }
}
