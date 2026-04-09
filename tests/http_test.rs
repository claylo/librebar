#![allow(missing_docs)]
#![cfg(feature = "http")]

use rebar::http::{HttpClient, HttpClientConfig};
use std::time::Duration;

/// Run with `REBAR_TEST_NETWORK=1 cargo nextest run` to enable network tests.
fn network_tests_enabled() -> bool {
    std::env::var("REBAR_TEST_NETWORK").is_ok()
}

#[test]
fn client_config_defaults() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0");
    assert_eq!(cfg.user_agent, "test-app/0.1.0");
    assert_eq!(cfg.timeout, Duration::from_secs(30));
}

#[test]
fn client_config_custom_timeout() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0").with_timeout(Duration::from_secs(5));
    assert_eq!(cfg.timeout, Duration::from_secs(5));
}

#[test]
fn client_config_custom_user_agent() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0").with_user_agent("custom/1.0");
    assert_eq!(cfg.user_agent, "custom/1.0");
}

#[test]
fn client_construction() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0");
    let client = HttpClient::new(cfg);
    assert!(client.is_ok());
}

#[tokio::test]
async fn https_get_succeeds() {
    if !network_tests_enabled() {
        return;
    }
    let client = HttpClient::from_app("rebar-test", "0.1.0").unwrap();
    let resp = client
        .get("https://api.github.com/zen")
        .await
        .expect("HTTPS GET should succeed");
    assert!(resp.is_success(), "status: {}", resp.status);
    let body = resp.text().unwrap();
    assert!(
        !body.is_empty(),
        "GitHub zen should return a non-empty string"
    );
}

#[tokio::test]
async fn http_get_succeeds() {
    if !network_tests_enabled() {
        return;
    }
    let client = HttpClient::from_app("rebar-test", "0.1.0").unwrap();
    let resp = client
        .get("http://httpbin.org/get")
        .await
        .expect("HTTP GET should succeed");
    assert!(resp.is_success(), "status: {}", resp.status);
}
