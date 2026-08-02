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
