//! Updater example — GitHub releases check with 24-hour cache and env suppression.
//!
//! Exercises librebar's `update` feature end-to-end: real HTTPS call to the
//! GitHub releases API, disk-backed 24h cache so repeat runs don't hammer the
//! API, and the `{APP}_NO_UPDATE_CHECK=1` opt-out every good CLI ships.
//!
//! # Run
//!
//! ```sh
//! # Check for updates (first call hits network, subsequent calls hit cache):
//! cargo run --example updater \
//!     --features "cli,config,logging,http,cache,update" \
//!     -- -C examples check
//!
//! # Report what the checker is configured to do without running it:
//! cargo run --example updater \
//!     --features "cli,config,logging,http,cache,update" \
//!     -- -C examples info
//!
//! # Drop the cache so the next `check` hits the network again:
//! cargo run --example updater \
//!     --features "cli,config,logging,http,cache,update" \
//!     -- -C examples clear-cache
//!
//! # Suppress the check entirely — useful in CI or offline:
//! UPDATER_NO_UPDATE_CHECK=1 cargo run --example updater \
//!     --features "cli,config,logging,http,cache,update" \
//!     -- -C examples check
//! ```
//!
//! # What it demonstrates
//!
//! - `librebar::update::UpdateChecker` wired against a real public repo. The
//!   target repo and the version we compare against are read from config, so
//!   you can point it at any GitHub project and see the cache + comparison
//!   flow without rebuilding.
//! - The 24h cache is transparent inside `check()`; the `info` subcommand
//!   peeks at the cache directly so you can see the stored tag after the
//!   first network round-trip, and `clear-cache` wipes it.
//! - `check()` is `async` because librebar's HTTPS client is `hyper`-based.
//!   A `current_thread` tokio runtime is enough — no threads are spawned.
//!
//! # Rate limits
//!
//! Unauthenticated GitHub API calls get 60/hour per IP. The 24h cache makes
//! that a non-issue under normal use; if you're iterating on this example,
//! use `clear-cache` sparingly or set `UPDATER_NO_UPDATE_CHECK=1` between
//! runs.
#![allow(missing_docs)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

const DEFAULT_REPO: &str = "BurntSushi/ripgrep";
// Low sentinel so the fetched latest-tag always compares as newer — the
// example is about the cache and env wiring, not realistic version tracking.
const DEFAULT_PRETEND_VERSION: &str = "0.0.1";
const CACHE_KEY: &str = "latest-version";

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    /// Log level used as the baseline when no `-q`/`-v` flag is passed.
    log_level: librebar::config::LogLevel,
    /// GitHub `owner/repo` to check for releases.
    repo: String,
    /// Version the checker pretends to be running. Kept artificially low
    /// in the sample config so `check` reliably reports an available update.
    pretend_version: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: librebar::config::LogLevel::Info,
            repo: DEFAULT_REPO.to_string(),
            pretend_version: DEFAULT_PRETEND_VERSION.to_string(),
        }
    }
}

#[derive(Parser)]
#[command(
    name = "updater",
    about = "GitHub release check with 24h cache and env suppression"
)]
struct Cli {
    #[command(flatten)]
    common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Check GitHub releases; returns a cached result if one is fresh.
    Check,
    /// Report configured repo, pretended version, cache status, and env gate.
    Info,
    /// Wipe the on-disk cache so the next `check` hits the network.
    ClearCache,
}

// Current-thread runtime: one HTTP request plus JSON decode doesn't need
// the multi-threaded scheduler, and librebar's tokio features don't enable
// `rt-multi-thread` anyway.
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.common.apply_color();
    cli.common.apply_chdir()?;

    // Hardcoded because env!("CARGO_PKG_NAME") inside a cargo example
    // resolves to the hosting crate (librebar), not the example.
    let app = librebar::init("updater")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_cli(cli.common)
        .config::<Config>()
        .logging()
        .start()?;

    match cli.command.unwrap_or(Command::Info) {
        Command::Check => run_check(&app).await,
        Command::Info => print_info(&app),
        Command::ClearCache => clear_cache(&app),
    }
}

async fn run_check(app: &librebar::App<Config>) -> Result<()> {
    let config = app.config();
    let checker =
        librebar::update::UpdateChecker::new(app.app_name(), &config.pretend_version, &config.repo);

    if checker.is_suppressed() {
        println!(
            "update check suppressed by {}_NO_UPDATE_CHECK",
            app.app_name().to_uppercase()
        );
        return Ok(());
    }

    println!(
        "checking {} for releases (pretending to run v{})...",
        config.repo, config.pretend_version
    );
    match checker.check().await {
        Some(info) => {
            tracing::info!(
                latest = %info.latest,
                current = %info.current,
                "update available",
            );
            println!("{}", info.message());
        }
        None => {
            tracing::info!("no update available or check failed");
            println!("no newer release found (or the check failed — run with -v for details)");
        }
    }
    Ok(())
}

fn print_info(app: &librebar::App<Config>) -> Result<()> {
    let config = app.config();
    let suppress_var = format!("{}_NO_UPDATE_CHECK", app.app_name().to_uppercase());
    let suppressed = std::env::var(&suppress_var)
        .ok()
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));

    let cache = librebar::cache::Cache::default_for(app.app_name())
        .context("cache directory is not available on this platform")?;
    let cache_entry = cache.get(CACHE_KEY)?;

    println!("app:            {} v{}", app.app_name(), app.version());
    println!("sources:        {:?}", app.config_sources());
    println!("repo:           {}", config.repo);
    println!("pretending:     v{}", config.pretend_version);
    println!("cache dir:      {}", cache.dir().display());
    match cache_entry {
        Some(bytes) => {
            let latest = String::from_utf8_lossy(&bytes);
            println!("cache:          fresh — latest-version={latest}");
        }
        None => println!("cache:          empty (next check will hit the network)"),
    }
    println!(
        "{}: {}",
        suppress_var,
        if suppressed {
            "set (check will be skipped)"
        } else {
            "unset"
        }
    );
    Ok(())
}

fn clear_cache(app: &librebar::App<Config>) -> Result<()> {
    let cache = librebar::cache::Cache::default_for(app.app_name())
        .context("cache directory is not available on this platform")?;
    cache.clear()?;
    println!("cache cleared: {}", cache.dir().display());
    Ok(())
}
