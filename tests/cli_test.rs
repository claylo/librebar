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

// ─── CommonArgs::apply ──────────────────────────────────────────────
//
// These deliberately avoid exercising a *successful* `--chdir`: that mutates
// process-global state and would race the rest of the suite. The failure path
// leaves the cwd untouched, so it is safe to run in parallel.

#[test]
fn apply_continues_when_no_terminal_flag_is_set() {
    let cli = TestCli::parse_from(["test-app", "info"]);
    let startup = cli.common.apply("1.2.3").expect("apply should succeed");

    assert_eq!(startup, librebar::cli::Startup::Continue);
    assert!(startup.is_continue());
    assert!(!startup.is_exit());
}

#[test]
fn apply_exits_on_version_only() {
    let cli = TestCli::parse_from(["test-app", "--version-only"]);
    let startup = cli.common.apply("1.2.3").expect("apply should succeed");

    assert_eq!(startup, librebar::cli::Startup::Exit);
    assert!(startup.is_exit());
}

#[test]
fn apply_reports_a_bad_chdir() {
    let cli = TestCli::parse_from([
        "test-app",
        "-C",
        "/librebar-nonexistent-directory-for-tests",
        "info",
    ]);

    let err = cli
        .common
        .apply("1.2.3")
        .expect_err("a nonexistent --chdir target should error");

    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    // The bare OS message names neither the flag nor the path, which leaves
    // the user nothing to act on.
    let msg = err.to_string();
    assert!(
        msg.contains("--chdir"),
        "message should name the flag: {msg}"
    );
    assert!(
        msg.contains("/librebar-nonexistent-directory-for-tests"),
        "message should name the path: {msg}"
    );
}

#[test]
fn version_only_is_answered_before_chdir() {
    // `--version-only` is a scripting query and must not fail because of an
    // unrelated bad `-C`. This pins the ordering inside `apply`.
    let cli = TestCli::parse_from([
        "test-app",
        "--version-only",
        "-C",
        "/librebar-nonexistent-directory-for-tests",
    ]);

    let startup = cli
        .common
        .apply("1.2.3")
        .expect("--version-only should not touch the filesystem");
    assert_eq!(startup, librebar::cli::Startup::Exit);
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
