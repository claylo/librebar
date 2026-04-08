#![cfg(feature = "config")]
#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use serde_json::json;

// ─── deep_merge tests ───────────────────────────────────────────────

#[test]
fn merge_scalar_override() {
    let mut base = json!({"level": "info"});
    rebar::config::deep_merge(&mut base, json!({"level": "debug"}));
    assert_eq!(base["level"], "debug");
}

#[test]
fn merge_nested_objects() {
    let mut base = json!({"logging": {"level": "info", "dir": "/var/log"}});
    rebar::config::deep_merge(&mut base, json!({"logging": {"level": "debug"}}));
    assert_eq!(base["logging"]["level"], "debug");
    assert_eq!(base["logging"]["dir"], "/var/log"); // preserved
}

#[test]
fn merge_array_replaces() {
    let mut base = json!({"tags": ["a", "b"]});
    rebar::config::deep_merge(&mut base, json!({"tags": ["c"]}));
    assert_eq!(base["tags"], json!(["c"]));
}

#[test]
fn merge_adds_new_keys() {
    let mut base = json!({"a": 1});
    rebar::config::deep_merge(&mut base, json!({"b": 2}));
    assert_eq!(base, json!({"a": 1, "b": 2}));
}

#[test]
fn merge_null_overlay_replaces() {
    let mut base = json!({"a": 1});
    rebar::config::deep_merge(&mut base, json!({"a": null}));
    assert!(base["a"].is_null());
}

// ─── file parsing tests ─────────────────────────────────────────────

#[test]
fn parse_toml_to_value() {
    let content = r#"
        log_level = "debug"
        [nested]
        key = "value"
    "#;
    let value = rebar::config::parse_toml(content).unwrap();
    assert_eq!(value["log_level"], "debug");
    assert_eq!(value["nested"]["key"], "value");
}

#[test]
fn parse_yaml_to_value() {
    let content = "log_level: debug\nnested:\n  key: value\n";
    let value = rebar::config::parse_yaml(content).unwrap();
    assert_eq!(value["log_level"], "debug");
    assert_eq!(value["nested"]["key"], "value");
}

#[test]
fn parse_json_to_value() {
    let content = r#"{"log_level": "debug", "nested": {"key": "value"}}"#;
    let value = rebar::config::parse_json(content).unwrap();
    assert_eq!(value["log_level"], "debug");
    assert_eq!(value["nested"]["key"], "value");
}

// ─── deserialization into typed config ──────────────────────────────

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
struct TestConfig {
    log_level: rebar::config::LogLevel,
    log_dir: Option<camino::Utf8PathBuf>,
    custom_field: Option<String>,
}

#[test]
fn merge_and_deserialize() {
    let base = r#"log_level = "info""#;
    let overlay = r#"custom_field = "hello""#;

    let mut merged = rebar::config::parse_toml(base).unwrap();
    rebar::config::deep_merge(&mut merged, rebar::config::parse_toml(overlay).unwrap());

    let config: TestConfig = serde_json::from_value(merged).unwrap();
    assert_eq!(config.log_level, rebar::config::LogLevel::Info);
    assert_eq!(config.custom_field.as_deref(), Some("hello"));
}

#[test]
fn log_level_default_is_info() {
    assert_eq!(
        rebar::config::LogLevel::default(),
        rebar::config::LogLevel::Info
    );
}

#[test]
fn log_level_as_str() {
    assert_eq!(rebar::config::LogLevel::Debug.as_str(), "debug");
    assert_eq!(rebar::config::LogLevel::Info.as_str(), "info");
    assert_eq!(rebar::config::LogLevel::Warn.as_str(), "warn");
    assert_eq!(rebar::config::LogLevel::Error.as_str(), "error");
}
