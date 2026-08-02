#![allow(missing_docs)]
#![cfg(feature = "crash")]

use librebar::crash;
use std::fs;
use tempfile::TempDir;

fn crash_info(
    message: impl Into<String>,
    location: Option<&str>,
    timestamp: impl Into<String>,
    os: impl Into<String>,
    backtrace: impl Into<String>,
) -> crash::CrashInfo {
    let mut info = crash::CrashInfo::new(message, "test-app", "0.1.0")
        .with_timestamp(timestamp)
        .with_os(os)
        .with_backtrace(backtrace);
    if let Some(location) = location {
        info = info.with_location(location);
    }
    info
}

#[test]
fn crash_info_constructor_sets_and_overrides_fields() {
    let info = crash::CrashInfo::new("test panic", "test-app", "0.1.0")
        .with_location("src/main.rs:42")
        .with_timestamp("2026-04-08T12:00:00.000Z")
        .with_os("macos")
        .with_backtrace("   0: test::frame");

    assert_eq!(info.message, "test panic");
    assert_eq!(info.location.as_deref(), Some("src/main.rs:42"));
    assert_eq!(info.app_name, "test-app");
    assert_eq!(info.version, "0.1.0");
    assert_eq!(info.timestamp, "2026-04-08T12:00:00.000Z");
    assert_eq!(info.os, "macos");
    assert_eq!(info.backtrace, "   0: test::frame");
}

#[test]
fn crash_info_format_contains_required_fields() {
    let info = crash_info(
        "test panic",
        Some("src/main.rs:42"),
        "2026-04-08T12:00:00.000Z",
        "macos",
        "   0: test::frame",
    );

    let formatted = info.format();
    assert!(formatted.contains("test panic"));
    assert!(formatted.contains("test-app"));
    assert!(formatted.contains("0.1.0"));
    assert!(formatted.contains("src/main.rs:42"));
    assert!(formatted.contains("macos"));
}

#[test]
fn write_crash_dump_creates_file() {
    let tmp = TempDir::new().unwrap();
    let info = crash_info(
        "test panic",
        Some("src/main.rs:42"),
        "2026-04-08T12:00:00.000Z",
        std::env::consts::OS,
        "",
    );

    let path = crash::write_crash_dump_to(&info, tmp.path());
    assert!(path.is_some(), "should write crash file");

    let path = path.unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("test panic"));
    assert!(content.contains("test-app"));
}

#[test]
fn crash_dump_is_structured_json() {
    let tmp = TempDir::new().unwrap();
    let info = crash_info(
        "first line\nsecond line",
        Some("src/main.rs:42"),
        "2026-04-08T12:00:00.000Z",
        "macos",
        "0: test::frame",
    );

    let path = crash::write_crash_dump_to(&info, tmp.path()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();

    assert_eq!(value["message"], "first line\nsecond line");
    assert_eq!(value["location"], "src/main.rs:42");
    assert_eq!(value["app_name"], "test-app");
    assert_eq!(value["version"], "0.1.0");
    assert_eq!(value["timestamp"], "2026-04-08T12:00:00.000Z");
    assert_eq!(value["os"], "macos");
    assert_eq!(value["backtrace"], "0: test::frame");
}

#[cfg(unix)]
#[test]
fn crash_dump_is_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let tmp = TempDir::new().unwrap();
    let info = crash_info(
        "sensitive panic",
        Some("src/main.rs:42"),
        "2026-04-08T12:00:00.000Z",
        std::env::consts::OS,
        "private source paths",
    );

    let path = crash::write_crash_dump_to(&info, tmp.path()).unwrap();
    let mode = fs::metadata(path).unwrap().permissions().mode();

    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn crash_dump_does_not_replace_a_same_timestamp_file() {
    let tmp = TempDir::new().unwrap();
    let info = crash_info(
        "first panic",
        None,
        "2026-04-08T12:00:00.000Z",
        std::env::consts::OS,
        "",
    );

    let path = crash::write_crash_dump_to(&info, tmp.path()).unwrap();
    fs::write(&path, "original dump").unwrap();

    assert!(crash::write_crash_dump_to(&info, tmp.path()).is_none());
    assert_eq!(fs::read_to_string(path).unwrap(), "original dump");
}

#[test]
fn crash_dump_retention_keeps_the_ten_newest_files() {
    let tmp = TempDir::new().unwrap();
    let mut paths = Vec::new();

    for second in 0..12 {
        let info = crash_info(
            format!("panic {second}"),
            None,
            format!("2026-04-08T12:00:{second:02}.000Z"),
            std::env::consts::OS,
            "",
        );
        paths.push(crash::write_crash_dump_to(&info, tmp.path()).unwrap());
    }

    let retained = fs::read_dir(tmp.path()).unwrap().count();
    assert_eq!(retained, 10);
    assert!(!paths[0].exists());
    assert!(!paths[1].exists());
    assert!(paths[11].exists());
}

#[test]
fn try_write_crash_dump_returns_typed_errors() {
    let info = crash_info(
        "test panic",
        None,
        "2026-04-08T12:00:00.000Z",
        std::env::consts::OS,
        "",
    );

    // Attempting to write to a non-writable path yields a CreateDir error
    let result = crash::try_write_crash_dump_to(&info, std::path::Path::new("/dev/null/crashes"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("create crash dump directory") || msg.contains("open crash dump file"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn try_write_crash_dump_succeeds() {
    let tmp = TempDir::new().unwrap();
    let info = crash_info(
        "test panic",
        None,
        "2026-04-08T12:00:01.000Z",
        std::env::consts::OS,
        "",
    );

    let path = crash::try_write_crash_dump_to(&info, tmp.path()).unwrap();
    assert!(path.exists());
}

#[test]
fn crash_dir_contains_app_name() {
    let dir = crash::crash_dump_dir("test-app");
    let path = dir.to_string_lossy();
    assert!(
        path.contains("test-app"),
        "crash dir should contain app name: {path}"
    );
}
