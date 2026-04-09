#![allow(missing_docs)]
#![cfg(feature = "shutdown")]

use rebar::shutdown::ShutdownHandle;

#[test]
fn handle_starts_not_shutting_down() {
    let handle = ShutdownHandle::new();
    assert!(!handle.is_shutting_down());
}

#[test]
fn shutdown_sets_flag() {
    let handle = ShutdownHandle::new();
    handle.shutdown();
    assert!(handle.is_shutting_down());
}

#[test]
fn token_is_cloneable() {
    let handle = ShutdownHandle::new();
    let token1 = handle.token();
    let token2 = token1.clone();
    handle.shutdown();
    assert!(token1.is_shutting_down());
    assert!(token2.is_shutting_down());
}

#[tokio::test]
async fn token_cancelled_resolves_after_shutdown() {
    let handle = ShutdownHandle::new();
    let mut token = handle.token();

    let handle_clone = handle.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        handle_clone.shutdown();
    });

    token.cancelled().await;
    assert!(handle.is_shutting_down());
}

#[tokio::test]
async fn token_cancelled_resolves_immediately_if_already_shutdown() {
    let handle = ShutdownHandle::new();
    handle.shutdown();

    let mut token = handle.token();
    tokio::time::timeout(std::time::Duration::from_millis(100), token.cancelled())
        .await
        .expect("cancelled() should resolve immediately when already shut down");
}

#[test]
fn multiple_shutdown_calls_are_safe() {
    let handle = ShutdownHandle::new();
    handle.shutdown();
    handle.shutdown();
    assert!(handle.is_shutting_down());
}
