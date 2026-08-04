#![allow(missing_docs)]
#![cfg(all(feature = "cli", feature = "config", feature = "logging"))]

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use tempfile::TempDir;

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct TestConfig {
    log_level: librebar::config::LogLevel,
    custom: Option<String>,
}

#[derive(Parser)]
#[command(name = "test-app")]
struct TestCli {
    #[command(flatten)]
    pub common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    pub command: Option<TestCommands>,
}

#[derive(clap::Subcommand)]
enum TestCommands {
    Run,
}

#[test]
fn builder_without_config() {
    let cli = TestCli::parse_from(["test-app", "run"]);

    let app: librebar::App = librebar::init("test-app")
        .with_cli(cli.common)
        .start()
        .unwrap();

    assert!(!app.cli().quiet);
}

#[test]
fn builder_with_config_file() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    fs::write(&config_path, r#"custom = "hello""#).unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let cli = TestCli::parse_from(["test-app", "run"]);

    let app: librebar::App<TestConfig> = librebar::init("test-app")
        .with_cli(cli.common)
        .config_from_file::<TestConfig>(&config_path)
        .start()
        .unwrap();

    assert_eq!(app.config().custom.as_deref(), Some("hello"));
}

#[test]
fn builder_with_preloaded_config() {
    let config = TestConfig {
        log_level: librebar::config::LogLevel::Debug,
        custom: Some("preloaded".to_string()),
    };
    let cli = TestCli::parse_from(["test-app", "run"]);

    let app = librebar::init("test-app")
        .with_cli(cli.common)
        .with_config(config)
        .start()
        .unwrap();

    assert_eq!(app.config().custom.as_deref(), Some("preloaded"));
}

#[test]
fn configured_builder_applies_programmatic_override_last() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    fs::write(&config_path, r#"custom = "file""#).unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let app = librebar::init("librebar-builder-override-test")
        .config_from_file::<TestConfig>(&config_path)
        .with_config_override("custom", "cli")
        .start()
        .unwrap();

    assert_eq!(app.config().custom.as_deref(), Some("cli"));
    assert_eq!(app.config_sources().override_paths, ["custom"]);
}

#[test]
fn preloaded_config_accepts_programmatic_override() {
    use librebar::config::ConfigOrigin;

    let config = TestConfig {
        log_level: librebar::config::LogLevel::Info,
        custom: Some("preloaded".to_string()),
    };

    let app = librebar::init("librebar-preloaded-override-test")
        .with_config(config)
        .with_config_override("custom", "cli")
        .start()
        .unwrap();

    assert_eq!(app.config().custom.as_deref(), Some("cli"));
    assert_eq!(app.config_sources().override_paths, ["custom"]);
    assert_eq!(
        app.config_sources().origin("log_level"),
        Some(&ConfigOrigin::Preloaded)
    );
    assert_eq!(
        app.config_sources().origin("custom"),
        Some(&ConfigOrigin::Override {
            path: "custom".to_string(),
        })
    );
}

#[test]
fn app_cli_accessors() {
    let cli = TestCli::parse_from(["test-app", "--quiet", "run"]);

    let app: librebar::App = librebar::init("test-app")
        .with_cli(cli.common)
        .start()
        .unwrap();

    assert!(app.cli().quiet);
}

#[test]
fn app_name_accessor() {
    let cli = TestCli::parse_from(["test-app", "run"]);

    let app: librebar::App = librebar::init("test-app")
        .with_cli(cli.common)
        .start()
        .unwrap();

    assert_eq!(app.app_name(), "test-app");
}

#[test]
fn builder_config_sources_empty_without_files() {
    let config = TestConfig::default();
    let cli = TestCli::parse_from(["test-app", "run"]);

    let app = librebar::init("test-app")
        .with_cli(cli.common)
        .with_config(config)
        .start()
        .unwrap();

    assert!(app.config_sources().primary_file().is_none());
}

/// Verbosity has to be settable without a `CommonArgs`.
///
/// The level reached the filter only through config `log_level` or a
/// `CommonArgs` handed to `with_cli`. An application adopting logging before
/// its CLI had no way to set it, and could not build a `CommonArgs` by hand to
/// bridge the gap — one field is `pub(crate)`, so it has to be parsed.
#[test]
fn with_log_level_sets_the_filter_default() {
    let temp = TempDir::new().unwrap();
    let app: librebar::App = librebar::init("librebar-log-level-test")
        .logging()
        .with_log_dir(temp.path().to_path_buf())
        .with_log_level("debug")
        .start()
        .unwrap();

    tracing::debug!("recorded at debug");
    drop(app);

    let log = fs::read_to_string(temp.path().join("librebar-log-level-test.jsonl")).unwrap();
    assert!(log.contains("recorded at debug"), "{log}");
}

/// An explicit setting outranks the config file, matching `with_log_dir`.
#[test]
fn with_log_level_overrides_the_config_field() {
    let temp = TempDir::new().unwrap();
    let config = TestConfig {
        log_level: librebar::config::LogLevel::Error,
        custom: None,
    };

    let app = librebar::init("librebar-log-level-precedence-test")
        .with_config(config)
        .logging()
        .with_log_dir(temp.path().to_path_buf())
        .with_log_level("debug")
        .start()
        .unwrap();

    tracing::debug!("explicit wins");
    drop(app);

    let log =
        fs::read_to_string(temp.path().join("librebar-log-level-precedence-test.jsonl")).unwrap();
    assert!(log.contains("explicit wins"), "{log}");
}
