#![allow(missing_docs)]
#![cfg(feature = "logging")]

use rebar::logging;

// ─── env_filter tests ───────────────────────────────────────────────

#[test]
fn env_filter_quiet_overrides() {
    let filter = logging::env_filter(true, 0, "info");
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
    let temp_dir = std::env::temp_dir().join("rebar-test-log-path");
    let file_path = temp_dir.join("custom.jsonl");

    let target = logging::resolve_log_target_with("demo", Some(file_path), None, None).unwrap();
    assert_eq!(target.dir, temp_dir);
    assert_eq!(target.file_name, "custom.jsonl");
}

#[test]
fn log_target_from_dir_appends_service() {
    let temp_dir = std::env::temp_dir().join("rebar-test-log-dir");
    let target =
        logging::resolve_log_target_with("demo", None, Some(temp_dir.clone()), None).unwrap();
    assert_eq!(target.dir, temp_dir);
    assert_eq!(target.file_name, "demo.jsonl");
}

#[test]
fn log_target_path_overrides_dir() {
    let temp_dir = std::env::temp_dir().join("rebar-test-log-override");
    let file_path = temp_dir.join("override.jsonl");

    let target =
        logging::resolve_log_target_with("demo", Some(file_path), Some(std::env::temp_dir()), None)
            .unwrap();
    assert_eq!(target.dir, temp_dir);
    assert_eq!(target.file_name, "override.jsonl");
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
