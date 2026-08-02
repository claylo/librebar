#![allow(missing_docs)]
#![cfg(feature = "cache")]

use librebar::cache::Cache;
use std::time::Duration;
use tempfile::TempDir;

use base64::Engine as _;

fn cache_entry_path(dir: &std::path::Path, key: &str) -> std::path::PathBuf {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.as_bytes());
    dir.join(format!("v2-{encoded}.cache"))
}

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
fn cache_file_stores_the_raw_binary_value() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    let value = (0_u8..=255).cycle().take(4096).collect::<Vec<_>>();

    cache
        .set("binary", &value, Duration::from_secs(60))
        .unwrap();

    let stored = std::fs::read(cache_entry_path(tmp.path(), "binary")).unwrap();
    assert_eq!(stored.len(), value.len() + 16);
    assert_eq!(&stored[16..], value);
    assert_eq!(cache.get("binary").unwrap().unwrap(), value);
}

#[test]
fn malformed_cache_header_is_a_format_error() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    std::fs::write(
        cache_entry_path(tmp.path(), "broken"),
        b"not a cache header",
    )
    .unwrap();

    let error = cache.get("broken").unwrap_err();

    assert!(
        error.to_string().contains("invalid cache entry format"),
        "{error}"
    );
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
fn expired_entry_returns_none_without_unlinking() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    // TTL of 0 means already expired
    cache.set("key1", b"value1", Duration::ZERO).unwrap();
    let path = cache_entry_path(tmp.path(), "key1");
    assert!(path.exists());

    let result = cache.get("key1").unwrap();

    assert!(result.is_none());
    assert!(path.exists());
}

#[test]
fn prune_removes_only_expired_v2_entries() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    let unrelated = tmp.path().join("notes.txt");
    let malformed = tmp.path().join("v2-malformed.cache");

    cache.set("expired", b"old", Duration::ZERO).unwrap();
    cache
        .set("live", b"current", Duration::from_secs(60))
        .unwrap();
    std::fs::write(&unrelated, b"keep").unwrap();
    std::fs::write(&malformed, b"not a cache header").unwrap();

    assert_eq!(cache.prune().unwrap(), 1);
    assert!(!cache_entry_path(tmp.path(), "expired").exists());
    assert!(cache_entry_path(tmp.path(), "live").exists());
    assert!(unrelated.exists());
    assert!(malformed.exists());
}

#[test]
fn prune_treats_a_missing_directory_as_empty() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("missing");
    let cache = Cache::new(&missing);

    assert_eq!(cache.prune().unwrap(), 0);
    assert!(!missing.exists());
}

#[test]
fn first_write_through_a_new_handle_prunes_expired_entries() {
    let tmp = TempDir::new().unwrap();
    let expired_path = cache_entry_path(tmp.path(), "expired");

    let original = Cache::new(tmp.path());
    original.set("expired", b"old", Duration::ZERO).unwrap();
    assert!(expired_path.exists());

    let restarted = Cache::new(tmp.path());
    restarted
        .set("fresh", b"current", Duration::from_secs(60))
        .unwrap();

    assert!(!expired_path.exists());
    assert_eq!(
        restarted.get("fresh").unwrap().as_deref(),
        Some(b"current".as_ref())
    );
}

#[test]
fn cloned_handles_share_the_automatic_prune_cadence() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache
        .set("seed", b"current", Duration::from_secs(60))
        .unwrap();
    cache.set("expired", b"old", Duration::ZERO).unwrap();
    let expired_path = cache_entry_path(tmp.path(), "expired");
    assert!(expired_path.exists());

    let cloned = cache.clone();
    cloned
        .set("next", b"current", Duration::from_secs(60))
        .unwrap();

    assert!(expired_path.exists());
    assert_eq!(cache.prune().unwrap(), 1);
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
fn clear_report_returns_counts() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache.set("key1", b"val1", Duration::from_secs(60)).unwrap();
    cache.set("key2", b"val2", Duration::from_secs(60)).unwrap();

    let report = cache.clear_report().unwrap();

    assert_eq!(report.removed, 2);
    assert!(report.failed.is_empty());
    assert!(cache.get("key1").unwrap().is_none());
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

    let entry = cache_entry_path(tmp.path(), "key");
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

    // The cache dir also holds .cache.lock; read_dir order is
    // filesystem-dependent, so select the entry file by name.
    let entry = std::fs::read_dir(tmp.path())
        .unwrap()
        .map(|entry| entry.unwrap())
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("v2-") && name.ends_with(".cache"))
        })
        .expect("cache entry file present");
    assert_eq!(
        entry.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn cache_set_refuses_to_replace_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::new(tmp.path());
    std::fs::create_dir(cache_entry_path(tmp.path(), "key")).unwrap();

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
