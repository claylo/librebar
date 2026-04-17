#![allow(missing_docs, unsafe_code)]
#![cfg(feature = "otel")]

use librebar::otel::OtelConfig;
use std::sync::{Mutex, MutexGuard};

// Process-global env vars are shared across threads. nextest sidesteps this
// by running each test in its own process, but `cargo test` runs them on
// threads within one process — a mutation in one test will race with a read
// in another. This file-level lock serializes the tests that touch env so
// the suite works under either runner.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Clear OTEL env vars so tests run deterministically regardless of the
/// host environment. Returns a guard the caller must hold for the duration
/// of the test body — dropping it before the test ends reopens the race.
#[must_use = "hold the returned guard for the whole test"]
fn clear_otel_env() -> MutexGuard<'static, ()> {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: ENV_LOCK serializes env-touching tests in this file.
    unsafe {
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        std::env::remove_var("OTEL_EXPORTER_OTLP_PROTOCOL");
    }
    guard
}

#[test]
fn otel_config_from_app_name() {
    let _guard = clear_otel_env();
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0");
    assert_eq!(cfg.service, "test-app");
    assert_eq!(cfg.version, "0.1.0");
    assert_eq!(cfg.env, "dev");
    assert!(cfg.endpoint.is_none());
}

#[test]
fn otel_config_env_var_names() {
    let _guard = clear_otel_env();
    let cfg = OtelConfig::from_app_name("my-tool", "1.0.0");
    assert_eq!(cfg.env_var_endpoint, "OTEL_EXPORTER_OTLP_ENDPOINT");
    assert_eq!(cfg.env_var_protocol, "OTEL_EXPORTER_OTLP_PROTOCOL");
    assert_eq!(cfg.env_var_env, "MY_TOOL_ENV");
}

#[test]
fn otel_config_with_endpoint() {
    let _guard = clear_otel_env();
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0")
        .with_endpoint(Some("http://localhost:4318".to_string()));
    assert_eq!(cfg.endpoint.as_deref(), Some("http://localhost:4318"));
}

#[test]
fn build_layer_returns_none_without_endpoint() {
    let _guard = clear_otel_env();
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0");
    let result = librebar::otel::build_otel_layer(&cfg);
    assert!(result.is_ok());
    let (layer, guard) = result.unwrap();
    assert!(layer.is_none(), "no endpoint means no layer");
    assert!(guard.is_none(), "no endpoint means no guard");
}
