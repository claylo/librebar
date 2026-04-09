#![allow(missing_docs)]
#![cfg(feature = "http")]

use rebar::http::{HttpClient, HttpClientConfig};
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
