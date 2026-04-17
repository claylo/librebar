#![allow(missing_docs)]
#![cfg(feature = "cli")]

use clap::Parser;

/// Test harness that embeds librebar's CommonArgs the way a consumer would.
#[derive(Parser, Debug)]
#[command(name = "test-app")]
struct TestCli {
    #[command(flatten)]
    pub common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    pub command: Option<TestCommands>,
}

#[derive(clap::Subcommand, Debug)]
enum TestCommands {
    Info,
}

#[test]
fn common_args_defaults() {
    let cli = TestCli::parse_from(["test-app", "info"]);
    assert!(!cli.common.quiet);
    assert_eq!(cli.common.verbose, 0);
    assert!(!cli.common.json);
    assert!(!cli.common.version_only);
    assert!(cli.common.chdir.is_none());
}

#[test]
fn common_args_quiet_flag() {
    let cli = TestCli::parse_from(["test-app", "--quiet", "info"]);
    assert!(cli.common.quiet);
}

#[test]
fn common_args_verbose_stacks() {
    let cli = TestCli::parse_from(["test-app", "-vv", "info"]);
    assert_eq!(cli.common.verbose, 2);
}

#[test]
fn common_args_json_flag() {
    let cli = TestCli::parse_from(["test-app", "--json", "info"]);
    assert!(cli.common.json);
}

#[test]
fn common_args_chdir() {
    let cli = TestCli::parse_from(["test-app", "-C", "/tmp", "info"]);
    assert_eq!(
        cli.common.chdir.as_deref(),
        Some(std::path::Path::new("/tmp"))
    );
}

#[test]
fn color_choice_default_is_auto() {
    let cli = TestCli::parse_from(["test-app", "info"]);
    assert!(matches!(cli.common.color, librebar::cli::ColorChoice::Auto));
}

#[test]
fn color_choice_never() {
    let cli = TestCli::parse_from(["test-app", "--color", "never", "info"]);
    assert!(matches!(
        cli.common.color,
        librebar::cli::ColorChoice::Never
    ));
}
