#![allow(missing_docs)]
#![cfg(feature = "cache")]

use rebar::cache::Cache;
use std::time::Duration;
use tempfile::TempDir;

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
    let dir = rebar::cache::default_cache_dir("test-app");
    assert!(dir.is_some());
    let dir = dir.unwrap();
    let path = dir.to_string_lossy();
    assert!(
        path.contains("test-app"),
        "cache dir should contain app name: {path}"
    );
}
