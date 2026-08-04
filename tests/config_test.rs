#![allow(missing_docs)]
#![cfg(feature = "config")]

use std::error::Error as _;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::sync::{Arc, Mutex};

use librebar::config::serde_json::{self, json};
use librebar::error::BoxError;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

// ─── camino re-export ───────────────────────────────────────────────

/// A consumer config reaching `Utf8PathBuf` through librebar rather than
/// through its own `camino` dependency. `Utf8Path` appears in librebar's
/// public API, so the re-export is what guarantees both sides mean the same
/// type instead of two independently resolved copies.
#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
struct PathConfig {
    log_dir: Option<librebar::camino::Utf8PathBuf>,
}

#[test]
fn camino_reexport_is_usable_in_a_consumer_config() {
    let parsed: PathConfig = serde_json::from_value(json!({"log_dir": "/var/log/app"})).unwrap();

    assert_eq!(
        parsed.log_dir.as_deref(),
        Some(librebar::camino::Utf8Path::new("/var/log/app"))
    );
}

#[test]
fn camino_reexport_is_the_type_librebar_compiled_against() {
    // `config_from_file` takes `&camino::Utf8Path`. Handing it a path built
    // from the re-export is what stops compiling if the two ever diverge.
    let path = librebar::camino::Utf8Path::new("/nonexistent/librebar-test.toml");
    let _builder = librebar::init("test-app").config_from_file::<PathConfig>(path);
}

// ─── deep_merge tests ───────────────────────────────────────────────

#[test]
fn merge_scalar_override() {
    let mut base = json!({"level": "info"});
    librebar::config::deep_merge(&mut base, json!({"level": "debug"})).unwrap();
    assert_eq!(base["level"], "debug");
}

#[test]
fn merge_nested_objects() {
    let mut base = json!({"logging": {"level": "info", "dir": "/var/log"}});
    librebar::config::deep_merge(&mut base, json!({"logging": {"level": "debug"}})).unwrap();
    assert_eq!(base["logging"]["level"], "debug");
    assert_eq!(base["logging"]["dir"], "/var/log"); // preserved
}

#[test]
fn merge_array_replaces() {
    let mut base = json!({"tags": ["a", "b"]});
    librebar::config::deep_merge(&mut base, json!({"tags": ["c"]})).unwrap();
    assert_eq!(base["tags"], json!(["c"]));
}

#[test]
fn merge_adds_new_keys() {
    let mut base = json!({"a": 1});
    librebar::config::deep_merge(&mut base, json!({"b": 2})).unwrap();
    assert_eq!(base, json!({"a": 1, "b": 2}));
}

#[test]
fn merge_null_overlay_replaces() {
    let mut base = json!({"a": 1});
    librebar::config::deep_merge(&mut base, json!({"a": null})).unwrap();
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

    let err = librebar::config::deep_merge(&mut base, overlay).unwrap_err();
    assert!(matches!(err, librebar::Error::ConfigMergeDepth));
}

// ─── file parsing tests ─────────────────────────────────────────────

#[test]
fn parse_toml_to_value() {
    let content = r#"
        log_level = "debug"
        [nested]
        key = "value"
    "#;
    let value = librebar::config::parse_toml(content).unwrap();
    assert_eq!(value["log_level"], "debug");
    assert_eq!(value["nested"]["key"], "value");
}

#[test]
fn parse_yaml_to_value() {
    let content = "log_level: debug\nnested:\n  key: value\n";
    let value = librebar::config::parse_yaml(content).unwrap();
    assert_eq!(value["log_level"], "debug");
    assert_eq!(value["nested"]["key"], "value");
}

#[test]
fn parse_json_to_value() {
    let content = r#"{"log_level": "debug", "nested": {"key": "value"}}"#;
    let value = librebar::config::parse_json(content).unwrap();
    assert_eq!(value["log_level"], "debug");
    assert_eq!(value["nested"]["key"], "value");
}

// ─── deserialization into typed config ──────────────────────────────

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
struct TestConfig {
    log_level: librebar::config::LogLevel,
    log_dir: Option<camino::Utf8PathBuf>,
    custom_field: Option<String>,
}

#[derive(Debug)]
struct FixedEnvironment(Vec<(OsString, OsString)>);

impl FixedEnvironment {
    fn new(values: &[(&str, &str)]) -> Self {
        Self(
            values
                .iter()
                .map(|(key, value)| (OsString::from(key), OsString::from(value)))
                .collect(),
        )
    }
}

impl librebar::config::EnvironmentSource for FixedEnvironment {
    fn vars(&self, prefix: &str) -> Result<Vec<(OsString, OsString)>, BoxError> {
        Ok(self
            .0
            .iter()
            .filter(|(key, _)| key.to_str().is_some_and(|key| key.starts_with(prefix)))
            .cloned()
            .collect())
    }
}

struct CapturingEnvironment {
    requested_prefix: Arc<Mutex<Option<String>>>,
}

impl librebar::config::EnvironmentSource for CapturingEnvironment {
    fn vars(&self, prefix: &str) -> Result<Vec<(OsString, OsString)>, BoxError> {
        *self.requested_prefix.lock().unwrap() = Some(prefix.to_string());
        Ok(vec![(
            OsString::from(format!("{prefix}PORT")),
            OsString::from("9000"),
        )])
    }
}

struct FailingEnvironment;

impl librebar::config::EnvironmentSource for FailingEnvironment {
    fn vars(&self, _prefix: &str) -> Result<Vec<(OsString, OsString)>, BoxError> {
        Err(Box::new(io::Error::other("parameter store unavailable")))
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
struct EnvironmentConfig {
    database_url: String,
    database: DatabaseConfig,
    enabled: bool,
    port: u16,
    ratio: f64,
    tags: Vec<String>,
    metadata: serde_json::Value,
    level: librebar::config::LogLevel,
    build_id: Option<String>,
    optional_port: Option<u16>,
    optional_ratio: Option<f64>,
    optional_flag: Option<bool>,
    optional_tags: Option<Vec<String>>,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            database_url: String::new(),
            database: DatabaseConfig::default(),
            enabled: false,
            port: 8080,
            ratio: 1.0,
            tags: Vec::new(),
            metadata: serde_json::json!({}),
            level: librebar::config::LogLevel::Info,
            build_id: None,
            optional_port: None,
            optional_ratio: None,
            optional_flag: None,
            optional_tags: None,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
struct DatabaseConfig {
    url: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct StrictEnvironmentConfig {
    known: String,
}

fn loader_with(values: &[(&str, &str)]) -> librebar::config::ConfigLoader {
    librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .without_boundary_marker()
        .with_environment_source(FixedEnvironment::new(values))
}

#[test]
fn custom_environment_source_receives_the_normalized_prefix_without_debug() {
    let requested_prefix = Arc::new(Mutex::new(None));
    let loader = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(CapturingEnvironment {
            requested_prefix: Arc::clone(&requested_prefix),
        });

    let debug = format!("{loader:?}");
    assert!(debug.contains("<environment source>"));

    let (config, _): (EnvironmentConfig, _) = loader.load().unwrap();
    assert_eq!(config.port, 9000);
    assert_eq!(requested_prefix.lock().unwrap().as_deref(), Some("MY_APP_"));
}

#[test]
fn environment_source_failures_preserve_the_nested_error() {
    let error = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(FailingEnvironment)
        .load::<EnvironmentConfig>()
        .unwrap_err();

    assert_eq!(error.to_string(), "configuration environment source failed");
    assert_eq!(
        error.source().unwrap().to_string(),
        "parameter store unavailable"
    );
}

#[test]
fn environment_uses_normalized_prefix_and_preserves_single_underscores() {
    let (config, sources): (EnvironmentConfig, _) = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(FixedEnvironment::new(&[(
            "MY_APP_DATABASE_URL",
            "postgres://flat",
        )]))
        .load()
        .unwrap();

    assert_eq!(config.database_url, "postgres://flat");
    assert_eq!(
        sources.environment_variables,
        ["MY_APP_DATABASE_URL".to_string()]
    );
}

#[test]
fn environment_double_underscore_addresses_nested_fields() {
    let (config, _): (EnvironmentConfig, _) =
        loader_with(&[("MY_APP_DATABASE__URL", "postgres://nested")])
            .load()
            .unwrap();

    assert_eq!(config.database.url, "postgres://nested");
}

#[test]
fn environment_parses_values_from_lower_layer_schema() {
    let source = FixedEnvironment::new(&[
        ("MY_APP_ENABLED", "true"),
        ("MY_APP_PORT", "9090"),
        ("MY_APP_RATIO", "1.25"),
        ("MY_APP_TAGS", r#"["worker","blue"]"#),
        ("MY_APP_METADATA", r#"{"region":"iad"}"#),
        ("MY_APP_LEVEL", "trace"),
    ]);

    let (config, _): (EnvironmentConfig, _) = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(source)
        .load()
        .unwrap();

    assert!(config.enabled);
    assert_eq!(config.port, 9090);
    assert_eq!(config.ratio, 1.25);
    assert_eq!(config.tags, ["worker", "blue"]);
    assert_eq!(config.metadata["region"], "iad");
    assert_eq!(config.level, librebar::config::LogLevel::Trace);
}

#[test]
fn environment_booleans_accept_only_lowercase_true_and_false() {
    for accepted in ["true", "false"] {
        loader_with(&[("MY_APP_ENABLED", accepted)])
            .load::<EnvironmentConfig>()
            .unwrap();
    }

    for rejected in ["1", "0", "yes", "no", "TRUE", "FALSE"] {
        let err = loader_with(&[("MY_APP_ENABLED", rejected)])
            .load::<EnvironmentConfig>()
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("MY_APP_ENABLED"), "{message}");
        assert!(message.contains("true` or `false"), "{message}");
    }
}

#[test]
fn empty_environment_string_is_a_value() {
    let (config, _): (EnvironmentConfig, _) =
        loader_with(&[("MY_APP_BUILD_ID", "")]).load().unwrap();
    assert_eq!(config.build_id.as_deref(), Some(""));
}

#[test]
fn empty_non_string_environment_value_is_a_parse_error() {
    let err = loader_with(&[("MY_APP_PORT", "")])
        .load::<EnvironmentConfig>()
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("MY_APP_PORT"), "{message}");
    assert!(message.contains("integer"), "{message}");
}

#[test]
fn null_schema_values_remain_strings() {
    let (config, _): (EnvironmentConfig, _) =
        loader_with(&[("MY_APP_BUILD_ID", "00123")]).load().unwrap();
    assert_eq!(config.build_id.as_deref(), Some("00123"));
}

/// An `Option<T>` field that defaults to `None` still gets its type honored.
///
/// `C::default()` is librebar's only schema, so an unset optional serializes to
/// `null` and carries no type with it. Without recovering that type from `C`
/// itself, every numeric optional is unsettable from the environment — the
/// value arrives as a string and deserialization rejects it.
#[test]
fn optional_numbers_accept_environment_values_without_a_file() {
    let (config, _): (EnvironmentConfig, _) = loader_with(&[
        ("MY_APP_OPTIONAL_PORT", "8000"),
        ("MY_APP_OPTIONAL_RATIO", "0.25"),
    ])
    .load()
    .unwrap();

    assert_eq!(config.optional_port, Some(8000));
    assert_eq!(config.optional_ratio, Some(0.25));
}

#[test]
fn optional_booleans_accept_environment_values_without_a_file() {
    let (config, _): (EnvironmentConfig, _) = loader_with(&[("MY_APP_OPTIONAL_FLAG", "true")])
        .load()
        .unwrap();

    assert_eq!(config.optional_flag, Some(true));
}

#[test]
fn optional_sequences_accept_environment_values_without_a_file() {
    let (config, _): (EnvironmentConfig, _) =
        loader_with(&[("MY_APP_OPTIONAL_TAGS", r#"["worker","blue"]"#)])
            .load()
            .unwrap();

    assert_eq!(
        config.optional_tags.as_deref(),
        Some(["worker".to_string(), "blue".to_string()].as_slice())
    );
}

/// Recovering the type per field, not per document.
///
/// A numeric-looking string bound for an `Option<String>` and a real number
/// bound for an `Option<u16>` arrive through the same layer. Parsing values
/// loosely — figment's approach — would resolve the number and corrupt the
/// string, so the two have to be typed independently.
#[test]
fn a_numeric_looking_string_survives_alongside_a_real_optional_number() {
    let (config, _): (EnvironmentConfig, _) = loader_with(&[
        ("MY_APP_BUILD_ID", "00123"),
        ("MY_APP_OPTIONAL_PORT", "8000"),
    ])
    .load()
    .unwrap();

    assert_eq!(config.build_id.as_deref(), Some("00123"));
    assert_eq!(config.optional_port, Some(8000));
}

/// Once the type is known, a bad value is reported against that type.
///
/// The pre-fix error said only "invalid configuration at optional_port" —
/// accurate, but it left the reader to work out that a string had been handed
/// to a number. Recovering the type is what makes the reason sayable, and it
/// puts an optional field's diagnostics on par with a required one's.
#[test]
fn an_unparseable_optional_number_reports_the_expected_type() {
    let err = loader_with(&[("MY_APP_OPTIONAL_PORT", "not-a-number")])
        .load::<EnvironmentConfig>()
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("MY_APP_OPTIONAL_PORT"), "{message}");
    assert!(message.contains("number"), "{message}");
}

#[test]
fn a_discovered_file_value_supplies_schema_for_an_optional_number() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join(".my-app.toml");
    fs::write(&file, "optional_port = 7000\n").unwrap();
    let project = camino::Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();

    let (config, _): (EnvironmentConfig, _) = loader_with(&[("MY_APP_OPTIONAL_PORT", "8000")])
        .without_boundary_marker()
        .with_project_search(project)
        .load()
        .unwrap();

    assert_eq!(config.optional_port, Some(8000));
}

#[test]
fn unknown_prefixed_variables_are_ignored_by_default() {
    let (config, sources): (EnvironmentConfig, _) =
        loader_with(&[("MY_APP_TYPO_FIELD", "ignored")])
            .load()
            .unwrap();
    assert_eq!(config, EnvironmentConfig::default());
    assert!(sources.environment_variables.is_empty());
}

#[test]
fn unknown_prefixed_variables_can_be_collected() {
    let err = loader_with(&[("MY_APP_TYPO_FIELD", "collected")])
        .with_unknown_environment(librebar::config::UnknownEnvironment::Collect)
        .load::<StrictEnvironmentConfig>()
        .unwrap_err();
    assert!(matches!(
        err,
        librebar::Error::ConfigValue {
            path,
            origin: librebar::config::ConfigOrigin::Environment { variable },
            ..
        } if path == "typo_field" && variable == "MY_APP_TYPO_FIELD"
    ));
}

#[cfg(unix)]
#[test]
fn ignored_unknown_values_are_not_decoded_but_known_values_are() {
    use std::os::unix::ffi::OsStringExt;

    let ignored = FixedEnvironment(vec![(
        OsString::from("MY_APP_TYPO_FIELD"),
        OsString::from_vec(vec![0xff]),
    )]);
    librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(ignored)
        .load::<EnvironmentConfig>()
        .unwrap();

    let known = FixedEnvironment(vec![(
        OsString::from("MY_APP_DATABASE_URL"),
        OsString::from_vec(vec![0xff]),
    )]);
    let err = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(known)
        .load::<EnvironmentConfig>()
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("MY_APP_DATABASE_URL"), "{message}");
    assert!(message.contains("UTF-8"), "{message}");
}

#[test]
fn duplicate_environment_paths_are_rejected() {
    let err = loader_with(&[
        ("MY_APP_DATABASE_URL", "postgres://one"),
        ("MY_APP_DATABASE_URL", "postgres://two"),
    ])
    .load::<EnvironmentConfig>()
    .unwrap_err();
    assert!(matches!(err, librebar::Error::ConfigEnvironment { .. }));
}

#[test]
fn parent_child_environment_conflicts_are_rejected() {
    let err = loader_with(&[
        ("MY_APP_DATABASE", r#"{"url":"postgres://whole"}"#),
        ("MY_APP_DATABASE__URL", "postgres://child"),
    ])
    .load::<EnvironmentConfig>()
    .unwrap_err();
    assert!(matches!(err, librebar::Error::ConfigEnvironment { .. }));
}

#[test]
fn empty_environment_path_segments_are_rejected() {
    let err = loader_with(&[("MY_APP_DATABASE____URL", "postgres://bad")])
        .load::<EnvironmentConfig>()
        .unwrap_err();
    assert!(matches!(err, librebar::Error::ConfigEnvironment { .. }));
}

#[test]
fn over_deep_environment_paths_are_rejected() {
    let variable = format!("MY_APP_{}", vec!["NESTED"; 65].join("__"));
    let source = FixedEnvironment(vec![(OsString::from(variable), OsString::from("value"))]);
    let err = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(source)
        .load::<EnvironmentConfig>()
        .unwrap_err();
    assert!(matches!(err, librebar::Error::ConfigEnvironment { .. }));
}

#[test]
fn merge_and_deserialize() {
    let base = r#"log_level = "info""#;
    let overlay = r#"custom_field = "hello""#;

    let mut merged = librebar::config::parse_toml(base).unwrap();
    librebar::config::deep_merge(&mut merged, librebar::config::parse_toml(overlay).unwrap())
        .unwrap();

    let config: TestConfig = serde_json::from_value(merged).unwrap();
    assert_eq!(config.log_level, librebar::config::LogLevel::Info);
    assert_eq!(config.custom_field.as_deref(), Some("hello"));
}

#[test]
fn log_level_default_is_info() {
    assert_eq!(
        librebar::config::LogLevel::default(),
        librebar::config::LogLevel::Info
    );
}

#[test]
fn log_level_as_str() {
    assert_eq!(librebar::config::LogLevel::Debug.as_str(), "debug");
    assert_eq!(librebar::config::LogLevel::Info.as_str(), "info");
    assert_eq!(librebar::config::LogLevel::Warn.as_str(), "warn");
    assert_eq!(librebar::config::LogLevel::Error.as_str(), "error");
}

#[test]
fn log_level_trace_round_trips() {
    let level: librebar::config::LogLevel = serde_json::from_str(r#""trace""#).unwrap();
    assert_eq!(level, librebar::config::LogLevel::Trace);
    assert_eq!(level.as_str(), "trace");
}

// ─── ConfigLoader discovery tests ───────────────────────────────────

#[test]
fn loader_defaults_when_no_files() {
    let loader = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_environment()
        .without_boundary_marker();

    let (config, sources): (TestConfig, _) = loader.load().unwrap();
    assert_eq!(config.log_level, librebar::config::LogLevel::Info);
    assert!(sources.primary_file().is_none());
}

#[test]
fn loader_explicit_file_overrides_default() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    fs::write(&config_path, r#"log_level = "debug""#).unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let (config, sources): (TestConfig, _) = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_environment()
        .with_file(&config_path)
        .load()
        .unwrap();

    assert_eq!(config.log_level, librebar::config::LogLevel::Debug);
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

    let (config, _): (TestConfig, _) = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_environment()
        .with_file(&base)
        .with_file(&over)
        .load()
        .unwrap();

    assert_eq!(config.log_level, librebar::config::LogLevel::Error);
}

#[test]
fn loader_discovers_dotfile() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("project");
    let sub_dir = project_dir.join("src").join("deep");
    fs::create_dir_all(&sub_dir).unwrap();

    fs::write(project_dir.join(".test-app.toml"), r#"log_level = "debug""#).unwrap();

    let sub_dir = camino::Utf8PathBuf::try_from(sub_dir).unwrap();

    let (config, sources): (TestConfig, _) = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_environment()
        .without_boundary_marker()
        .with_project_search(&sub_dir)
        .load()
        .unwrap();

    assert_eq!(config.log_level, librebar::config::LogLevel::Debug);
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

    let (config, sources): (TestConfig, _) = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_environment()
        .without_boundary_marker()
        .with_project_search(&project_dir)
        .load()
        .unwrap();

    assert_eq!(config.log_level, librebar::config::LogLevel::Debug);
    let found = sources.project_file.unwrap();
    assert!(found.as_str().contains(".config/"));
}

#[test]
fn loader_preserves_extension_before_layout_precedence() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("project");
    let dotconfig_dir = project_dir.join(".config");
    fs::create_dir_all(&dotconfig_dir).unwrap();

    fs::write(dotconfig_dir.join("test-app.yaml"), "log_level: debug\n").unwrap();
    fs::write(project_dir.join(".test-app.toml"), r#"log_level = "warn""#).unwrap();

    let project_dir = camino::Utf8PathBuf::try_from(project_dir).unwrap();

    let (config, sources): (TestConfig, _) = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_environment()
        .without_boundary_marker()
        .with_project_search(&project_dir)
        .load()
        .unwrap();

    assert_eq!(config.log_level, librebar::config::LogLevel::Warn);
    assert_eq!(
        sources.project_file.unwrap(),
        project_dir.join(".test-app.toml")
    );
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

    let (config, sources): (TestConfig, _) = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_environment()
        .with_boundary_marker(".git")
        .with_project_search(&work)
        .load()
        .unwrap();

    assert_eq!(config.log_level, librebar::config::LogLevel::Info); // default, not parent's warn
    assert!(sources.project_file.is_none());
}

#[test]
fn loader_boundary_marker_stops_at_search_root() {
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("parent");
    let project = parent.join("project");
    fs::create_dir_all(project.join(".git")).unwrap();

    fs::write(parent.join(".test-app.toml"), r#"log_level = "warn""#).unwrap();

    let project = camino::Utf8PathBuf::try_from(project).unwrap();

    let (config, sources): (TestConfig, _) = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_environment()
        .with_project_search(&project)
        .load()
        .unwrap();

    assert_eq!(config.log_level, librebar::config::LogLevel::Info);
    assert!(sources.project_file.is_none());
}

#[test]
fn loader_load_or_error_fails_when_no_config() {
    let result = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_boundary_marker()
        .without_environment()
        .load_or_error::<TestConfig>();

    assert!(matches!(result, Err(librebar::Error::ConfigNotFound)));
}

#[test]
fn load_or_error_accepts_environment_as_a_configuration_source() {
    let (config, sources): (EnvironmentConfig, _) =
        loader_with(&[("MY_APP_DATABASE_URL", "postgres://environment")])
            .load_or_error()
            .unwrap();

    assert_eq!(config.database_url, "postgres://environment");
    assert_eq!(
        sources.environment_variables,
        ["MY_APP_DATABASE_URL".to_string()]
    );
}

#[test]
fn explicit_file_overrides_environment() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("config.toml");
    fs::write(&file, "port = 7000\n").unwrap();
    let file = camino::Utf8PathBuf::try_from(file).unwrap();

    let (config, sources): (EnvironmentConfig, _) = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_file(file)
        .with_environment_source(FixedEnvironment::new(&[("MY_APP_PORT", "8000")]))
        .load()
        .unwrap();

    assert_eq!(config.port, 7000);
    assert_eq!(sources.environment_variables, ["MY_APP_PORT".to_string()]);
}

#[test]
fn programmatic_override_beats_explicit_file() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("config.toml");
    fs::write(&file, "port = 7000\n").unwrap();
    let file = camino::Utf8PathBuf::try_from(file).unwrap();

    let (config, sources): (EnvironmentConfig, _) = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_file(file)
        .with_environment_source(FixedEnvironment::new(&[("MY_APP_PORT", "8000")]))
        .with_override("port", 9000_u16)
        .load()
        .unwrap();

    assert_eq!(config.port, 9000);
    assert_eq!(sources.override_paths, ["port".to_string()]);
}

#[test]
fn config_sources_reports_the_winning_origin_for_each_path() {
    use librebar::config::ConfigOrigin;

    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("config.toml");
    fs::write(&file, "port = 7000\nratio = 2.0\n").unwrap();
    let file = camino::Utf8PathBuf::try_from(file).unwrap();

    let (_, sources): (EnvironmentConfig, _) = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_file(&file)
        .with_environment_source(FixedEnvironment::new(&[
            ("MY_APP_PORT", "8000"),
            ("MY_APP_ENABLED", "true"),
        ]))
        .with_override("port", 9000_u16)
        .load()
        .unwrap();

    assert_eq!(
        sources.origin("port"),
        Some(&ConfigOrigin::Override {
            path: "port".to_string(),
        })
    );
    assert_eq!(
        sources.origin("ratio"),
        Some(&ConfigOrigin::ExplicitFile { path: file })
    );
    assert_eq!(
        sources.origin("enabled"),
        Some(&ConfigOrigin::Environment {
            variable: "MY_APP_ENABLED".to_string(),
        })
    );
    assert_eq!(sources.origin("database.url"), Some(&ConfigOrigin::Default));
}

#[test]
fn config_sources_discards_origins_for_values_replaced_by_an_override() {
    use librebar::config::ConfigOrigin;

    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("config.toml");
    fs::write(&file, "[metadata]\nold = 1\n").unwrap();
    let file = camino::Utf8PathBuf::try_from(file).unwrap();

    let (config, sources): (EnvironmentConfig, _) = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_file(file)
        .without_environment()
        .with_override("metadata", "replacement")
        .load()
        .unwrap();

    assert_eq!(config.metadata, "replacement");
    assert_eq!(
        sources.origin("metadata.old"),
        Some(&ConfigOrigin::Override {
            path: "metadata".to_string(),
        })
    );
}

#[test]
fn deserialization_error_identifies_the_winning_file_and_path() {
    use librebar::config::ConfigOrigin;

    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("config.toml");
    fs::write(&file, "database = \"not-an-object\"\n").unwrap();
    let file = camino::Utf8PathBuf::try_from(file).unwrap();

    let error = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_file(&file)
        .with_environment_source(FixedEnvironment::new(&[(
            "MY_APP_DATABASE__URL",
            "postgres://environment",
        )]))
        .load::<EnvironmentConfig>()
        .unwrap_err();

    let librebar::Error::ConfigValue {
        path,
        origin,
        source: _,
    } = &error
    else {
        panic!("unexpected error: {error:?}");
    };
    assert_eq!(path, "database");
    assert_eq!(origin, &ConfigOrigin::ExplicitFile { path: file });
    assert!(error.source().unwrap().is::<serde_json::Error>());
}

#[test]
fn deserialization_error_identifies_the_winning_override() {
    use librebar::config::ConfigOrigin;

    let error = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(FixedEnvironment::new(&[("MY_APP_RATIO", "2.0")]))
        .with_override("ratio", "not-a-number")
        .load::<EnvironmentConfig>()
        .unwrap_err();

    let librebar::Error::ConfigValue { path, origin, .. } = &error else {
        panic!("unexpected error: {error:?}");
    };
    assert_eq!(path, "ratio");
    assert_eq!(
        origin,
        &ConfigOrigin::Override {
            path: "ratio".to_string(),
        }
    );
    assert!(error.source().unwrap().is::<serde_json::Error>());
}

#[test]
fn load_or_error_accepts_programmatic_override_as_a_source() {
    let (config, sources): (EnvironmentConfig, _) = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(FixedEnvironment::new(&[]))
        .with_override("port", 9000_u16)
        .load_or_error()
        .unwrap();

    assert_eq!(config.port, 9000);
    assert_eq!(sources.override_paths, ["port".to_string()]);
}

struct FailingSerialize;

impl Serialize for FailingSerialize {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Err(serde::ser::Error::custom("nope"))
    }
}

#[test]
fn programmatic_override_reports_serialization_error_without_a_value() {
    let err = librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .with_environment_source(FixedEnvironment::new(&[]))
        .with_override("secret", FailingSerialize)
        .load::<EnvironmentConfig>()
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("secret"), "{message}");
    assert!(!message.contains("MY_SECRET_VALUE"), "{message}");
}

#[test]
fn invalid_programmatic_override_paths_are_rejected() {
    for path in ["database..url".to_string(), vec!["nested"; 65].join(".")] {
        let err = librebar::config::ConfigLoader::new("my-app")
            .with_user_config(false)
            .with_environment_source(FixedEnvironment::new(&[]))
            .with_override(path.clone(), "value")
            .load::<EnvironmentConfig>()
            .unwrap_err();
        assert!(err.to_string().contains(&path), "{err}");
    }
}

#[test]
fn loader_yaml_file() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.yaml");
    fs::write(&config_path, "log_level: debug\ncustom_field: hello\n").unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let (config, _): (TestConfig, _) = librebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_environment()
        .with_file(&config_path)
        .load()
        .unwrap();

    assert_eq!(config.log_level, librebar::config::LogLevel::Debug);
    assert_eq!(config.custom_field.as_deref(), Some("hello"));
}

// ─── XDG helpers ────────────────────────────────────────────────────

/// All four XDG accessors resolve, and each is namespaced by app name.
///
/// `user_data_local_dir` was missing from this set until it was added
/// alongside its three siblings; `logging` had been reaching for
/// `data_local_dir` internally the whole time.
#[test]
fn xdg_helpers_resolve_and_namespace_by_app() {
    let dirs = [
        librebar::config::user_config_dir("librebar-xdg-test"),
        librebar::config::user_cache_dir("librebar-xdg-test"),
        librebar::config::user_data_dir("librebar-xdg-test"),
        librebar::config::user_data_local_dir("librebar-xdg-test"),
    ];

    for dir in &dirs {
        let dir = dir
            .as_ref()
            .expect("home directory should resolve in the test environment");
        assert!(
            dir.as_str().contains("librebar-xdg-test"),
            "{dir} should be namespaced by app name"
        );
        assert!(dir.is_absolute(), "{dir} should be absolute");
    }
}

/// The data and machine-local data directories are distinct concepts even
/// where they resolve to the same path.
///
/// They agree on Linux and macOS and diverge on Windows, so this asserts the
/// call works rather than that the paths differ.
#[test]
fn user_data_local_dir_resolves_independently() {
    let data = librebar::config::user_data_dir("librebar-xdg-test");
    let local = librebar::config::user_data_local_dir("librebar-xdg-test");

    assert_eq!(data.is_some(), local.is_some());
}
