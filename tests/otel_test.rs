#![allow(missing_docs, unsafe_code)]
#![cfg(feature = "otel")]

use rebar::otel::OtelConfig;

/// Clear OTEL env vars so tests run deterministically regardless of
/// the host environment.
fn clear_otel_env() {
    // Safety: nextest runs each test as a separate process, so this
    // cannot race with other threads reading env vars.
    unsafe {
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        std::env::remove_var("OTEL_EXPORTER_OTLP_PROTOCOL");
    }
}

#[test]
fn otel_config_from_app_name() {
    clear_otel_env();
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0");
    assert_eq!(cfg.service, "test-app");
    assert_eq!(cfg.version, "0.1.0");
    assert_eq!(cfg.env, "dev");
    assert!(cfg.endpoint.is_none());
}

#[test]
fn otel_config_env_var_names() {
    clear_otel_env();
    let cfg = OtelConfig::from_app_name("my-tool", "1.0.0");
    assert_eq!(cfg.env_var_endpoint, "OTEL_EXPORTER_OTLP_ENDPOINT");
    assert_eq!(cfg.env_var_protocol, "OTEL_EXPORTER_OTLP_PROTOCOL");
    assert_eq!(cfg.env_var_env, "MY_TOOL_ENV");
}

#[test]
fn otel_config_with_endpoint() {
    clear_otel_env();
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0")
        .with_endpoint(Some("http://localhost:4318".to_string()));
    assert_eq!(cfg.endpoint.as_deref(), Some("http://localhost:4318"));
}

#[test]
fn build_layer_returns_none_without_endpoint() {
    clear_otel_env();
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0");
    let result = rebar::otel::build_otel_layer(&cfg);
    assert!(result.is_ok());
    let (layer, guard) = result.unwrap();
    assert!(layer.is_none(), "no endpoint means no layer");
    assert!(guard.is_none(), "no endpoint means no guard");
}
