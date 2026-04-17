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
