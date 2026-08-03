#![allow(missing_docs)]
#![cfg(feature = "logging")]

use librebar::logging;

// ─── env_filter tests ───────────────────────────────────────────────

#[test]
fn env_filter_quiet_overrides() {
    let filter: logging::tracing_subscriber::filter::EnvFilter =
        logging::env_filter(true, 0, "info");
    assert_eq!(filter.to_string(), "error");
}

#[test]
fn env_filter_verbose_debug() {
    let filter = logging::env_filter(false, 1, "info");
    assert_eq!(filter.to_string(), "debug");
}

#[test]
fn env_filter_verbose_trace() {
    let filter = logging::env_filter(false, 2, "info");
    assert_eq!(filter.to_string(), "trace");
}

#[test]
fn env_filter_default_level() {
    let filter = logging::env_filter(false, 0, "warn");
    assert_eq!(filter.to_string(), "warn");
}

// ─── log target resolution tests ────────────────────────────────────

#[test]
fn log_target_from_path_uses_parent() {
    let temp_dir = std::env::temp_dir().join("librebar-test-log-path");
    let file_path = temp_dir.join("custom.jsonl");

    let target = logging::resolve_log_target_with("demo", Some(file_path), None, None).unwrap();
    assert_eq!(target.dir, temp_dir);
    assert_eq!(target.file_name, "custom.jsonl");
}

#[test]
fn log_target_from_dir_appends_service() {
    let temp_dir = std::env::temp_dir().join("librebar-test-log-dir");
    let target =
        logging::resolve_log_target_with("demo", None, Some(temp_dir.clone()), None).unwrap();
    assert_eq!(target.dir, temp_dir);
    assert_eq!(target.file_name, "demo.jsonl");
}

#[test]
fn log_target_probe_uses_stable_file_name() {
    let temp = tempfile::TempDir::new().unwrap();
    let service = "librebar-probe-test";

    logging::resolve_log_target_with(service, None, Some(temp.path().into()), None).unwrap();

    let mut files = std::fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files, [format!("{service}.jsonl")]);
}

#[test]
fn log_target_path_overrides_dir() {
    let temp_dir = std::env::temp_dir().join("librebar-test-log-override");
    let file_path = temp_dir.join("override.jsonl");

    let target =
        logging::resolve_log_target_with("demo", Some(file_path), Some(std::env::temp_dir()), None)
            .unwrap();
    assert_eq!(target.dir, temp_dir);
    assert_eq!(target.file_name, "override.jsonl");
}

#[cfg(unix)]
#[test]
fn initialized_log_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = tempfile::TempDir::new().unwrap();
    let service = "librebar-private-log-test";
    let log_path = temp.path().join(format!("{service}.jsonl"));
    std::fs::write(&log_path, b"existing log\n").unwrap();
    std::fs::set_permissions(&log_path, std::fs::Permissions::from_mode(0o644)).unwrap();

    let config =
        logging::LoggingConfig::from_app_name(service).with_log_dir(Some(temp.path().into()));
    let guard = logging::init(&config, logging::env_filter(false, 0, "info")).unwrap();
    tracing::info!("permission check");
    drop(guard);

    let mode = std::fs::metadata(log_path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

// ─── timestamp tests ────────────────────────────────────────────────

#[test]
fn format_timestamp_produces_rfc3339() {
    let ts = logging::format_timestamp();
    assert!(ts.ends_with('Z'), "should end with Z: {ts}");
    assert_eq!(ts.len(), 24, "should be 24 chars: {ts}");
    assert_eq!(&ts[10..11], "T", "date-time separator");
}

// ─── platform log dir tests ─────────────────────────────────────────

#[test]
fn platform_log_dir_contains_service_name() {
    let dir = logging::platform_log_dir("test-svc").expect("should return Some");
    let path = dir.to_str().expect("valid UTF-8");
    assert!(
        path.contains("test-svc"),
        "should contain service name: {path}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn platform_log_dir_uses_library_logs_on_macos() {
    let dir = logging::platform_log_dir("test-svc").unwrap();
    let path = dir.to_str().unwrap();
    assert!(
        path.contains("Library/Logs"),
        "macOS should use ~/Library/Logs/: {path}"
    );
}

// ─── typed error resolution tests ───────────────────────────────────

#[test]
fn try_resolve_log_target_returns_typed_error_for_path_without_file_name() {
    let result = logging::try_resolve_log_target_with(
        "demo",
        Some(std::path::PathBuf::from("/")),
        None,
        None,
    );
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("file name"),
        "expected NoFileName, got: {error}"
    );
}

#[test]
fn try_resolve_log_target_succeeds_with_dir_override() {
    let temp = tempfile::TempDir::new().unwrap();
    let target =
        logging::try_resolve_log_target_with("demo", None, Some(temp.path().into()), None).unwrap();
    assert_eq!(target.file_name, "demo.jsonl");
}

// ─── rotated log retention ──────────────────────────────────────────

use std::time::{Duration, SystemTime};

/// Set a file's mtime to `age` in the past, so retention can be exercised
/// without waiting for real time to pass.
fn backdate(path: &std::path::Path, age: Duration) {
    let when = SystemTime::now() - age;
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

fn touch(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"x").unwrap();
    path
}

/// The default is seven days.
#[test]
fn default_retention_is_seven_days() {
    assert_eq!(
        logging::DEFAULT_LOG_RETENTION,
        Duration::from_secs(7 * 24 * 60 * 60)
    );
    let cfg = logging::LoggingConfig::from_app_name("myapp");
    assert_eq!(cfg.retention, Some(logging::DEFAULT_LOG_RETENTION));
}

/// Rotated logs past the cutoff go; recent ones and the live file stay.
///
/// The names here are the documented rotation scheme —
/// `{app}.{date}.jsonl` becomes `{app}.{date}.jsonl.zst` after compression.
#[test]
fn prune_removes_only_expired_rotated_logs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    let live = touch(path, "myapp.jsonl");
    let recent = touch(path, "myapp.2026-08-02.jsonl.zst");
    let old_zst = touch(path, "myapp.2026-07-01.jsonl.zst");
    let old_plain = touch(path, "myapp.2026-07-02.jsonl");

    // The live file is backdated too: age must not save it from the name check.
    backdate(&live, Duration::from_secs(40 * 24 * 60 * 60));
    backdate(&recent, Duration::from_secs(2 * 24 * 60 * 60));
    backdate(&old_zst, Duration::from_secs(30 * 24 * 60 * 60));
    backdate(&old_plain, Duration::from_secs(30 * 24 * 60 * 60));

    let removed =
        logging::prune_rotated_logs(path, "myapp.jsonl", logging::DEFAULT_LOG_RETENTION).unwrap();

    assert_eq!(removed, 2);
    assert!(live.exists(), "the live log must never be pruned");
    assert!(recent.exists(), "a log inside the window must survive");
    assert!(!old_zst.exists());
    assert!(!old_plain.exists());
}

/// Pruning only touches files it could have produced.
#[test]
fn prune_ignores_unrelated_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    let strangers = [
        touch(path, "notes.txt"),
        touch(path, "otherapp.2026-07-01.jsonl.zst"),
        touch(path, "myapp.log"),
        touch(path, "myapp.2026-07-01.txt"),
    ];
    for stranger in &strangers {
        backdate(stranger, Duration::from_secs(90 * 24 * 60 * 60));
    }

    let removed =
        logging::prune_rotated_logs(path, "myapp.jsonl", logging::DEFAULT_LOG_RETENTION).unwrap();

    assert_eq!(removed, 0);
    for stranger in &strangers {
        assert!(
            stranger.exists(),
            "{} should be untouched",
            stranger.display()
        );
    }
}

/// Retention is configurable, and a short window prunes more aggressively.
#[test]
fn prune_honors_a_custom_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    let two_days_old = touch(path, "myapp.2026-08-01.jsonl.zst");
    backdate(&two_days_old, Duration::from_secs(2 * 24 * 60 * 60));

    let kept =
        logging::prune_rotated_logs(path, "myapp.jsonl", Duration::from_secs(7 * 24 * 60 * 60))
            .unwrap();
    assert_eq!(kept, 0, "inside a 7-day window");

    let pruned =
        logging::prune_rotated_logs(path, "myapp.jsonl", Duration::from_secs(24 * 60 * 60))
            .unwrap();
    assert_eq!(pruned, 1, "outside a 1-day window");
}

/// `with_retention(None)` opts out entirely.
#[test]
fn retention_can_be_disabled() {
    let cfg = logging::LoggingConfig::from_app_name("myapp").with_retention(None);
    assert_eq!(cfg.retention, None);
}

/// `with_retention_days` is the config-friendly spelling.
#[test]
fn retention_days_convenience() {
    let cfg = logging::LoggingConfig::from_app_name("myapp").with_retention_days(3);
    assert_eq!(cfg.retention, Some(Duration::from_secs(3 * 24 * 60 * 60)));
}

/// Zero days disables pruning; it does not mean "delete everything."
///
/// Null cannot serve as the sentinel: an `Option<u64>` the user never set
/// serializes to null too, so null-means-never would defeat the default.
/// Reading zero literally would put the cutoff at now and wipe every rotated
/// log, which is what a typo in this field would otherwise cost.
#[test]
fn zero_retention_days_disables_pruning() {
    let cfg = logging::LoggingConfig::from_app_name("myapp").with_retention_days(0);
    assert_eq!(cfg.retention, None);
}

/// Even reached directly, a zero window refuses to delete anything.
#[test]
fn prune_refuses_a_zero_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    let ancient = touch(path, "myapp.2020-01-01.jsonl.zst");
    backdate(&ancient, Duration::from_secs(3650 * 24 * 60 * 60));

    let removed = logging::prune_rotated_logs(path, "myapp.jsonl", Duration::ZERO).unwrap();

    assert_eq!(removed, 0);
    assert!(ancient.exists(), "a zero window must not delete anything");
}
