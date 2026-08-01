#![allow(missing_docs)]
#![cfg(feature = "crash")]

use librebar::crash;
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

#[cfg(unix)]
#[test]
fn crash_dump_is_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let tmp = TempDir::new().unwrap();
    let info = crash::CrashInfo {
        message: "sensitive panic".to_string(),
        location: Some("src/main.rs:42".to_string()),
        app_name: "test-app".to_string(),
        version: "0.1.0".to_string(),
        timestamp: "2026-04-08T12:00:00.000Z".to_string(),
        os: std::env::consts::OS.to_string(),
        backtrace: "private source paths".to_string(),
    };

    let path = crash::write_crash_dump_to(&info, tmp.path()).unwrap();
    let mode = fs::metadata(path).unwrap().permissions().mode();

    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn crash_dump_does_not_replace_a_same_timestamp_file() {
    let tmp = TempDir::new().unwrap();
    let info = crash::CrashInfo {
        message: "first panic".to_string(),
        location: None,
        app_name: "test-app".to_string(),
        version: "0.1.0".to_string(),
        timestamp: "2026-04-08T12:00:00.000Z".to_string(),
        os: std::env::consts::OS.to_string(),
        backtrace: String::new(),
    };

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
        let info = crash::CrashInfo {
            message: format!("panic {second}"),
            location: None,
            app_name: "test-app".to_string(),
            version: "0.1.0".to_string(),
            timestamp: format!("2026-04-08T12:00:{second:02}.000Z"),
            os: std::env::consts::OS.to_string(),
            backtrace: String::new(),
        };
        paths.push(crash::write_crash_dump_to(&info, tmp.path()).unwrap());
    }

    let retained = fs::read_dir(tmp.path()).unwrap().count();
    assert_eq!(retained, 10);
    assert!(!paths[0].exists());
    assert!(!paths[1].exists());
    assert!(paths[11].exists());
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
