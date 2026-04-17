//! Minimal librebar example — CLI + config + logging.
//!
//! The smallest realistic librebar app. Defines a `Config`, parses CLI flags
//! (including the standard `-q` / `-v` / `--color` / `-C`), discovers a config
//! file, initializes structured JSONL logging, and handles two subcommands.
//!
//! # Run
//!
//! ```sh
//! # From the project root — config is discovered from the current directory:
//! cargo run --example minimal --features "cli,config,logging" -- hello
//! cargo run --example minimal --features "cli,config,logging" -- -v info
//!
//! # Point at the sample config explicitly:
//! cargo run --example minimal --features "cli,config,logging" -- -C examples info
//! ```
//!
//! # What it demonstrates
//!
//! - `librebar::init()` wiring CLI, config, and logging in one builder chain
//! - `librebar::cli::CommonArgs` flattened into a clap derive struct
//! - A `Config` struct loaded by discovery (TOML in this case)
//! - `tracing::info!` and `tracing::debug!` events written to JSONL on disk
//! - `app.config_sources()` reporting which files contributed
#![allow(missing_docs)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    /// Log level used as the baseline when no `-q`/`-v` flag is passed.
    log_level: librebar::config::LogLevel,
    /// Greeting prefix used by the `hello` subcommand.
    greeting: String,
    /// Name used by the `hello` subcommand.
    name: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: librebar::config::LogLevel::Info,
            greeting: "hello".to_string(),
            name: "world".to_string(),
        }
    }
}

#[derive(Parser)]
#[command(name = "minimal", about = "Smallest idiomatic librebar app")]
struct Cli {
    #[command(flatten)]
    common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Greet using values from config.
    Hello,
    /// Report loaded config and log destination.
    Info,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.common.apply_color();
    cli.common.apply_chdir()?;

    // A real app would pass `env!("CARGO_PKG_NAME")`. In a cargo example,
    // that resolves to the hosting crate's name (librebar), not "minimal",
    // so the string is hardcoded here to match the sample config filename.
    let app = librebar::init("minimal")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_cli(cli.common)
        .config::<Config>()
        .logging()
        .start()?;

    let config = app.config();

    match cli.command.unwrap_or(Command::Info) {
        Command::Hello => {
            tracing::info!(
                greeting = %config.greeting,
                name = %config.name,
                "greeted",
            );
            println!("{}, {}!", config.greeting, config.name);
        }
        Command::Info => {
            tracing::debug!("reporting app state");
            println!("app:     {} v{}", app.app_name(), app.version());
            println!("sources: {:?}", app.config_sources());
            println!(
                "log dir: {:?}",
                librebar::logging::platform_log_dir(app.app_name())
            );
            println!(
                "config:  greeting={:?} name={:?}",
                config.greeting, config.name
            );
        }
    }

    Ok(())
}
