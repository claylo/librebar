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

const fn assert_args<T: clap::Args>() {}

#[test]
fn common_args_is_cloneable_flattened_args() {
    assert_args::<librebar::cli::CommonArgs>();
    let cli = TestCli::parse_from(["test-app", "info"]);
    let copy = cli.common.clone();
    assert_eq!(copy.verbose, cli.common.verbose);
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
fn every_common_flag_propagates_to_subcommands() {
    // All of CommonArgs is `global`. A flag that works at the top level but
    // errors after a subcommand name is the kind of inconsistency a user only
    // discovers by tripping over it.
    let cli = TestCli::parse_from([
        "test-app",
        "info",
        "--version-only",
        "--quiet",
        "-vv",
        "--json",
        "--color",
        "never",
        "-C",
        "/tmp",
    ]);

    assert!(cli.common.version_only);
    assert!(cli.common.quiet);
    assert_eq!(cli.common.verbose, 2);
    assert!(cli.common.json);
    assert!(matches!(
        cli.common.color,
        librebar::cli::ColorChoice::Never
    ));
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

// NOTE: these harnesses deliberately use `//` rather than `///`. A doc comment
// here would itself become the command's about/long_about and mask the very
// leak these tests pin.

// A consumer that describes itself with `about` but never sets `long_about` —
// the overwhelmingly common shape, and the one where a leak hides in `--help`
// while `-h` looks fine.
#[derive(Parser, Debug)]
#[command(name = "about-app", about = "Do the thing")]
struct AboutCli {
    #[command(flatten)]
    #[allow(dead_code)]
    pub common: librebar::cli::CommonArgs,
}

// A consumer that describes itself not at all.
#[derive(Parser, Debug)]
#[command(name = "bare-app")]
struct BareCli {
    #[command(flatten)]
    #[allow(dead_code)]
    pub common: librebar::cli::CommonArgs,
}

#[test]
fn flattening_common_args_does_not_describe_the_consumer() {
    use clap::CommandFactory;

    // `Args::augment_args` applies a flattened struct's doc comment as the
    // parent command's `about`/`long_about`. `CommonArgs` has rustdoc aimed at
    // library readers, so without suppression every consumer's help text
    // describes librebar instead of the consumer.
    let cmd = BareCli::command();
    assert_eq!(
        cmd.get_about(),
        None,
        "flattening CommonArgs must not supply the consumer's short description"
    );
    assert_eq!(
        cmd.get_long_about(),
        None,
        "flattening CommonArgs must not supply the consumer's long description"
    );
}

#[test]
fn a_consumer_that_sets_about_keeps_it_in_long_help() {
    use clap::CommandFactory;

    // The regression: `-h` rendered the consumer's `about` because the derive
    // applies the parent's own attributes last, but `--help` fell through to
    // `long_about`, which only the flattened struct had set.
    let rendered = AboutCli::command().render_long_help().to_string();

    assert!(
        rendered.contains("Do the thing"),
        "--help should describe the consumer: {rendered}"
    );
    assert!(
        !rendered.contains("librebar-based applications"),
        "--help must not leak librebar's rustdoc: {rendered}"
    );
    assert!(
        !rendered.contains("command(flatten)"),
        "--help must not leak the rustdoc example: {rendered}"
    );
}
