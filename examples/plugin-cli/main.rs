//! Plugin-CLI example — git-style external subcommand dispatch.
//!
//! The main binary parses its own built-in subcommands (`info`, `which`) and
//! forwards anything else to `{app}-{subcommand}` on PATH, the same pattern
//! `git`, `cargo`, and `kubectl` use. The paired `hello-greet/main.rs`
//! ships as `plugin-cli-hello-greet`, a standalone binary that knows nothing
//! about librebar — that's the point: plugins are just PATH binaries.
//!
//! # Run
//!
//! Build both binaries in one pass:
//!
//! ```sh
//! cargo build --examples --features "cli,config,logging,dispatch"
//! ```
//!
//! Then run the main binary with the target/debug/examples directory
//! prepended to PATH so the plugin resolves:
//!
//! ```sh
//! PATH="$(pwd)/target/debug/examples:$PATH" \
//!     ./target/debug/examples/plugin-cli -C examples/plugin-cli hello-greet --name Clay
//! # → hello, Clay!
//!
//! # Built-in: report app state.
//! ./target/debug/examples/plugin-cli -C examples/plugin-cli info
//!
//! # Built-in: resolve a plugin without running it.
//! PATH="$(pwd)/target/debug/examples:$PATH" \
//!     ./target/debug/examples/plugin-cli which hello-greet
//!
//! # Unknown subcommand — reports the expected binary name.
//! ./target/debug/examples/plugin-cli nope
//! ```
//!
//! # What it demonstrates
//!
//! - `#[command(external_subcommand)]` on the main `Subcommand` enum, which
//!   makes clap hand unknown subcommands to us as `Vec<OsString>` instead of
//!   erroring — exactly what git-style dispatch needs.
//! - `librebar::dispatch::resolve` for "is this plugin on PATH?" lookups
//!   (the `which` subcommand) and `librebar::dispatch::run` for the full
//!   spawn-and-wait (the fallback arm).
//! - `librebar::dispatch::subcommand_binary` for telling the user exactly
//!   what binary name is expected, which is the single most helpful thing
//!   an error message can do here.
//! - Exit-code passthrough: when the plugin exits non-zero we exit with the
//!   same code, so shell pipelines and CI behave as if the plugin ran directly.
#![allow(missing_docs)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    /// Log level used as the baseline when no `-q`/`-v` flag is passed.
    log_level: librebar::config::LogLevel,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: librebar::config::LogLevel::Info,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "plugin-cli",
    about = "Main CLI that dispatches unknown subcommands to `plugin-cli-*` binaries on PATH"
)]
struct Cli {
    #[command(flatten)]
    common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Report app state and plugin-resolution info.
    Info,
    /// Resolve `plugin-cli-<name>` on PATH without running it.
    Which {
        /// Name of the subcommand to look up.
        name: String,
    },
    /// Any other subcommand is dispatched as `plugin-cli-<name>` on PATH.
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.common.apply_color();
    cli.common.apply_chdir()?;

    // Hardcoded: env!("CARGO_PKG_NAME") resolves to the hosting crate
    // (librebar) in a cargo example, not "plugin-cli".
    let app = librebar::init("plugin-cli")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_cli(cli.common)
        .config::<Config>()
        .logging()
        .start()?;

    match cli.command.unwrap_or(Command::Info) {
        Command::Info => print_info(&app),
        Command::Which { name } => print_which(&app, &name),
        Command::External(args) => dispatch_external(&app, args),
    }
}

fn print_info(app: &librebar::App<Config>) -> Result<()> {
    println!("app:      {} v{}", app.app_name(), app.version());
    println!("sources:  {:?}", app.config_sources());
    println!(
        "log dir:  {:?}",
        librebar::logging::platform_log_dir(app.app_name())
    );
    println!("built-ins: info, which");
    println!("plugins:  any `plugin-cli-<name>` binary on PATH (try `which hello-greet`)");
    Ok(())
}

fn print_which(app: &librebar::App<Config>, name: &str) -> Result<()> {
    let binary = librebar::dispatch::subcommand_binary(app.app_name(), name);
    match librebar::dispatch::resolve(app.app_name(), name) {
        Some(path) => {
            println!("{binary} -> {}", path.display());
            Ok(())
        }
        None => {
            anyhow::bail!("{binary} not found on PATH");
        }
    }
}

fn dispatch_external(app: &librebar::App<Config>, args: Vec<OsString>) -> Result<()> {
    // clap guarantees at least the subcommand name when `external_subcommand`
    // fires, so `split_first` is safe — but we handle the empty case anyway.
    let (sub, rest) = args
        .split_first()
        .context("external subcommand requires a name")?;
    let sub_str = sub.to_string_lossy();

    tracing::info!(subcommand = %sub_str, "dispatching to external plugin");

    match librebar::dispatch::run(app.app_name(), &sub_str, rest)? {
        Some(status) => {
            let code = status.code().unwrap_or(1);
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
        None => {
            let expected = librebar::dispatch::subcommand_binary(app.app_name(), &sub_str);
            anyhow::bail!(
                "unknown subcommand: {sub_str}. Install `{expected}` on PATH to provide it."
            );
        }
    }
}
