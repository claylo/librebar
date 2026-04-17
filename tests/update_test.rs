#![allow(missing_docs, unsafe_code)]
#![cfg(feature = "update")]

use librebar::update::{UpdateChecker, UpdateInfo};

#[test]
fn checker_from_app_name() {
    let checker = UpdateChecker::new("test-app", "0.1.0", "owner/repo");
    assert_eq!(checker.app_name(), "test-app");
    assert_eq!(checker.current_version(), "0.1.0");
}

#[test]
fn suppressed_by_env_var() {
    // SAFETY: nextest runs each test in its own process
    unsafe { std::env::set_var("TEST_APP_NO_UPDATE_CHECK", "1") };
    let checker = UpdateChecker::new("test-app", "0.1.0", "owner/repo");
    assert!(checker.is_suppressed());
    unsafe { std::env::remove_var("TEST_APP_NO_UPDATE_CHECK") };
}

#[test]
fn not_suppressed_by_default() {
    // SAFETY: nextest runs each test in its own process
    unsafe { std::env::remove_var("TEST_APP_NO_UPDATE_CHECK") };
    let checker = UpdateChecker::new("test-app", "0.1.0", "owner/repo");
    assert!(!checker.is_suppressed());
}

#[test]
fn version_is_newer() {
    assert!(librebar::update::is_newer("0.1.0", "0.2.0"));
    assert!(librebar::update::is_newer("0.1.0", "1.0.0"));
    assert!(librebar::update::is_newer("1.2.3", "1.2.4"));
    assert!(!librebar::update::is_newer("0.2.0", "0.1.0"));
    assert!(!librebar::update::is_newer("1.0.0", "1.0.0"));
}

#[test]
fn update_info_display() {
    let info = UpdateInfo {
        current: "0.1.0".to_string(),
        latest: "0.2.0".to_string(),
        url: "https://github.com/owner/repo/releases/tag/v0.2.0".to_string(),
    };
    let msg = info.message();
    assert!(msg.contains("0.2.0"));
    assert!(msg.contains("0.1.0"));
}
