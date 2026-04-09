#![allow(missing_docs)]
#![cfg(feature = "lockfile")]

use rebar::lockfile::Lockfile;
use tempfile::TempDir;

#[test]
fn acquire_lock_succeeds() {
    let tmp = TempDir::new().unwrap();
    let lock = Lockfile::new("test-app", tmp.path());
    assert!(lock.try_acquire().is_ok());
}

#[test]
fn lock_creates_file() {
    let tmp = TempDir::new().unwrap();
    let lock = Lockfile::new("test-app", tmp.path());
    let _guard = lock.try_acquire().unwrap();
    let lock_path = tmp.path().join("test-app.lock");
    assert!(lock_path.exists());
}

#[test]
fn lock_released_on_guard_drop() {
    let tmp = TempDir::new().unwrap();
    let lock = Lockfile::new("test-app", tmp.path());
    {
        let _guard = lock.try_acquire().unwrap();
    }
    // Guard dropped — should be able to re-acquire
    let lock2 = Lockfile::new("test-app", tmp.path());
    assert!(lock2.try_acquire().is_ok());
}

#[test]
fn lock_dir_default_contains_app_name() {
    let dir = rebar::lockfile::default_lock_dir("test-app");
    let path = dir.to_string_lossy();
    assert!(
        path.contains("test-app"),
        "lock dir should contain app name: {path}"
    );
}
