#![allow(missing_docs)]
#![cfg(feature = "config")]

use std::fs;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tempfile::TempDir;

// ─── deep_merge tests ───────────────────────────────────────────────

#[test]
fn merge_scalar_override() {
    let mut base = json!({"level": "info"});
    rebar::config::deep_merge(&mut base, json!({"level": "debug"})).unwrap();
    assert_eq!(base["level"], "debug");
}

#[test]
fn merge_nested_objects() {
    let mut base = json!({"logging": {"level": "info", "dir": "/var/log"}});
    rebar::config::deep_merge(&mut base, json!({"logging": {"level": "debug"}})).unwrap();
    assert_eq!(base["logging"]["level"], "debug");
    assert_eq!(base["logging"]["dir"], "/var/log"); // preserved
}

#[test]
fn merge_array_replaces() {
    let mut base = json!({"tags": ["a", "b"]});
    rebar::config::deep_merge(&mut base, json!({"tags": ["c"]})).unwrap();
    assert_eq!(base["tags"], json!(["c"]));
}

#[test]
fn merge_adds_new_keys() {
    let mut base = json!({"a": 1});
    rebar::config::deep_merge(&mut base, json!({"b": 2})).unwrap();
    assert_eq!(base, json!({"a": 1, "b": 2}));
}

#[test]
fn merge_null_overlay_replaces() {
    let mut base = json!({"a": 1});
    rebar::config::deep_merge(&mut base, json!({"a": null})).unwrap();
    assert!(base["a"].is_null());
}

#[test]
fn merge_rejects_excessive_depth() {
    // Both sides must be deeply-nested objects with matching keys:
    // merge_inner only increments depth through the (Object, Object) match arm.
    // If the base key is absent, entry().or_insert(Null) short-circuits via the
    // default `*base = overlay` branch and the depth guard never fires.
    let mut base = json!("bottom");
    let mut overlay = json!("bottom");
    for _ in 0..=64 {
        base = json!({ "k": base });
        overlay = json!({ "k": overlay });
    }

    let err = rebar::config::deep_merge(&mut base, overlay).unwrap_err();
    assert!(matches!(err, rebar::Error::ConfigMergeDepth));
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
    rebar::config::deep_merge(&mut merged, rebar::config::parse_toml(overlay).unwrap()).unwrap();

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

// ─── ConfigLoader discovery tests ───────────────────────────────────

#[test]
fn loader_defaults_when_no_files() {
    let loader = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_boundary_marker();

    let (config, sources): (TestConfig, _) = loader.load().unwrap();
    assert_eq!(config.log_level, rebar::config::LogLevel::Info);
    assert!(sources.primary_file().is_none());
}

#[test]
fn loader_explicit_file_overrides_default() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    fs::write(&config_path, r#"log_level = "debug""#).unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let (config, sources): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .with_file(&config_path)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Debug);
    assert!(sources.primary_file().is_some());
}

#[test]
fn loader_later_file_overrides_earlier() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base.toml");
    fs::write(&base, r#"log_level = "warn""#).unwrap();
    let over = tmp.path().join("override.toml");
    fs::write(&over, r#"log_level = "error""#).unwrap();

    let base = camino::Utf8PathBuf::try_from(base).unwrap();
    let over = camino::Utf8PathBuf::try_from(over).unwrap();

    let (config, _): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .with_file(&base)
        .with_file(&over)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Error);
}

#[test]
fn loader_discovers_dotfile() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("project");
    let sub_dir = project_dir.join("src").join("deep");
    fs::create_dir_all(&sub_dir).unwrap();

    fs::write(project_dir.join(".test-app.toml"), r#"log_level = "debug""#).unwrap();

    let sub_dir = camino::Utf8PathBuf::try_from(sub_dir).unwrap();

    let (config, sources): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_boundary_marker()
        .with_project_search(&sub_dir)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Debug);
    assert!(sources.project_file.is_some());
}

#[test]
fn loader_dotconfig_dir_takes_precedence() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("project");
    let dotconfig_dir = project_dir.join(".config");
    fs::create_dir_all(&dotconfig_dir).unwrap();

    fs::write(
        dotconfig_dir.join("test-app.toml"),
        r#"log_level = "debug""#,
    )
    .unwrap();
    fs::write(project_dir.join(".test-app.toml"), r#"log_level = "warn""#).unwrap();

    let project_dir = camino::Utf8PathBuf::try_from(project_dir).unwrap();

    let (config, sources): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_boundary_marker()
        .with_project_search(&project_dir)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Debug);
    let found = sources.project_file.unwrap();
    assert!(found.as_str().contains(".config/"));
}

#[test]
fn loader_boundary_marker_stops_search() {
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("parent");
    let child = parent.join("child");
    let work = child.join("work");
    fs::create_dir_all(&work).unwrap();

    fs::write(parent.join(".test-app.toml"), r#"log_level = "warn""#).unwrap();
    fs::create_dir(child.join(".git")).unwrap();

    let work = camino::Utf8PathBuf::try_from(work).unwrap();

    let (config, sources): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .with_boundary_marker(".git")
        .with_project_search(&work)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Info); // default, not parent's warn
    assert!(sources.project_file.is_none());
}

#[test]
fn loader_load_or_error_fails_when_no_config() {
    let result = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_boundary_marker()
        .load_or_error::<TestConfig>();

    assert!(matches!(result, Err(rebar::Error::ConfigNotFound)));
}

#[test]
fn loader_yaml_file() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.yaml");
    fs::write(&config_path, "log_level: debug\ncustom_field: hello\n").unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let (config, _): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .with_file(&config_path)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Debug);
    assert_eq!(config.custom_field.as_deref(), Some("hello"));
}
