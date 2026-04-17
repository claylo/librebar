#![allow(missing_docs)]
#![cfg(feature = "http")]

// Tests marked `#[ignore]` hit the public internet. The default `just test`
// run skips them (nextest reports them as "skipped" rather than claiming a
// silent pass). Run them explicitly with:
//
//     cargo nextest run --all-features --run-ignored only
//
// or, under the stock runner:
//
//     cargo test --all-features -- --ignored

use librebar::http::{HttpClient, HttpClientConfig};
use std::time::Duration;

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
#[ignore = "hits api.github.com; run with --run-ignored or `-- --ignored`"]
async fn https_get_succeeds() {
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
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
#[ignore = "hits httpbin.org; run with --run-ignored or `-- --ignored`"]
async fn http_get_succeeds() {
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let resp = client
        .get("http://httpbin.org/get")
        .await
        .expect("HTTP GET should succeed");
    assert!(resp.is_success(), "status: {}", resp.status);
}
