#![allow(missing_docs, unsafe_code)]
#![cfg(feature = "update")]

use librebar::update::{UpdateChecker, UpdateInfo};
use std::sync::Mutex;

// Process-global env vars are shared across threads. nextest sidesteps this
// by running each test in its own process, but `cargo test` runs them on
// threads within one process — a mutation in one test will race with a read
// in another. This file-level lock serializes the tests that touch env so
// the suite works under either runner.
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn checker_from_app_name() {
    let checker = UpdateChecker::new("test-app", "0.1.0", "owner/repo");
    assert_eq!(checker.app_name(), "test-app");
    assert_eq!(checker.current_version(), "0.1.0");
}

#[test]
fn suppressed_by_env_var() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
    unsafe { std::env::set_var("TEST_APP_NO_UPDATE_CHECK", "1") };
    let checker = UpdateChecker::new("test-app", "0.1.0", "owner/repo");
    assert!(checker.is_suppressed());
    // SAFETY: still holding ENV_LOCK.
    unsafe { std::env::remove_var("TEST_APP_NO_UPDATE_CHECK") };
}

#[test]
fn not_suppressed_by_default() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
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
