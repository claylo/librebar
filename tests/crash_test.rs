#![allow(missing_docs)]
#![cfg(feature = "crash")]

use rebar::crash;
use std::fs;
use tempfile::TempDir;

#[test]
fn crash_info_format_contains_required_fields() {
    let info = crash::CrashInfo {
        message: "test panic".to_string(),
        location: Some("src/main.rs:42".to_string()),
        app_name: "test-app".to_string(),
        version: "0.1.0".to_string(),
        timestamp: "2026-04-08T12:00:00.000Z".to_string(),
        os: "macos".to_string(),
        backtrace: "   0: test::frame".to_string(),
    };

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
    let info = crash::CrashInfo {
        message: "test panic".to_string(),
        location: Some("src/main.rs:42".to_string()),
        app_name: "test-app".to_string(),
        version: "0.1.0".to_string(),
        timestamp: "2026-04-08T12:00:00.000Z".to_string(),
        os: std::env::consts::OS.to_string(),
        backtrace: String::new(),
    };

    let path = crash::write_crash_dump_to(&info, tmp.path());
    assert!(path.is_some(), "should write crash file");

    let path = path.unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("test panic"));
    assert!(content.contains("test-app"));
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
