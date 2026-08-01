#![allow(missing_docs)]
#![cfg(feature = "cache")]

use librebar::cache::Cache;
use std::time::Duration;
use tempfile::TempDir;

use base64::Engine as _;

#[test]
fn store_and_retrieve() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache
        .set("key1", b"value1", Duration::from_secs(60))
        .unwrap();
    let result = cache.get("key1").unwrap();
    assert_eq!(result.as_deref(), Some(b"value1".as_ref()));
}

#[test]
fn structured_keys_do_not_collide() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    let ttl = Duration::from_secs(60);

    cache.set("foo/bar", b"slash", ttl).unwrap();
    cache.set("foo:bar", b"colon", ttl).unwrap();
    cache.set("foo.bar", b"dot", ttl).unwrap();

    assert_eq!(
        cache.get("foo/bar").unwrap().as_deref(),
        Some(b"slash".as_ref())
    );
    assert_eq!(
        cache.get("foo:bar").unwrap().as_deref(),
        Some(b"colon".as_ref())
    );
    assert_eq!(
        cache.get("foo.bar").unwrap().as_deref(),
        Some(b"dot".as_ref())
    );
}

#[test]
fn missing_key_returns_none() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    let result = cache.get("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn expired_entry_returns_none() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    // TTL of 0 means already expired
    cache.set("key1", b"value1", Duration::ZERO).unwrap();
    let result = cache.get("key1").unwrap();
    assert!(result.is_none());
}

#[test]
fn remove_deletes_entry() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache
        .set("key1", b"value1", Duration::from_secs(60))
        .unwrap();
    cache.remove("key1").unwrap();
    let result = cache.get("key1").unwrap();
    assert!(result.is_none());
}

#[test]
fn clear_removes_all() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache.set("key1", b"val1", Duration::from_secs(60)).unwrap();
    cache.set("key2", b"val2", Duration::from_secs(60)).unwrap();
    cache.clear().unwrap();
    assert!(cache.get("key1").unwrap().is_none());
    assert!(cache.get("key2").unwrap().is_none());
}

#[test]
fn default_cache_dir_contains_app_name() {
    let dir = librebar::cache::default_cache_dir("test-app");
    assert!(dir.is_some());
    let dir = dir.unwrap();
    let path = dir.to_string_lossy();
    assert!(
        path.contains("test-app"),
        "cache dir should contain app name: {path}"
    );
}

#[cfg(unix)]
#[test]
fn cache_set_replaces_symlink_without_following_it() {
    use std::os::unix::fs::symlink;

    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::new(tmp.path());
    let target = tmp.path().join("target");
    std::fs::write(&target, b"keep me").unwrap();

    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"key");
    let entry = tmp.path().join(format!("v1-{encoded}.json"));
    symlink(&target, &entry).unwrap();

    cache
        .set("key", b"replacement", Duration::from_secs(60))
        .unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
    assert!(
        !std::fs::symlink_metadata(entry)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn cache_set_forces_private_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::new(tmp.path());
    cache
        .set("secret", b"value", Duration::from_secs(60))
        .unwrap();

    let entry = std::fs::read_dir(tmp.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(
        entry.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn cache_set_refuses_to_replace_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::new(tmp.path());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"key");
    std::fs::create_dir(tmp.path().join(format!("v1-{encoded}.json"))).unwrap();

    cache
        .set("key", b"value", Duration::from_secs(60))
        .expect_err("a directory destination must be rejected");
}

#[test]
fn cache_set_saturates_extreme_expiry_instead_of_overflowing() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::new(tmp.path());

    cache.set("long-lived", b"value", Duration::MAX).unwrap();

    assert_eq!(cache.get("long-lived").unwrap().unwrap(), b"value");
}
