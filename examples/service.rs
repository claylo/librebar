//! Long-running service example — async runtime, graceful shutdown, crash dumps, optional OTEL.
//!
//! This is the "service" shape: an app that runs until an operator signals it
//! to stop. It wires the full builder chain — CLI, config, JSONL logging,
//! OpenTelemetry, shutdown token, and a crash-dumping panic hook — then drives
//! a periodic work loop that awaits the shutdown token alongside a `tokio::time::interval`.
//!
//! # Run
//!
//! ```sh
//! # Run from repo root (discovers examples/service.toml via -C):
//! cargo run --example service \
//!     --features "cli,config,logging,shutdown,crash,otel" \
//!     -- -C examples run
//!
//! # Report what was loaded and how observability is wired:
//! cargo run --example service \
//!     --features "cli,config,logging,shutdown,crash,otel" \
//!     -- -C examples info
//!
//! # Fire a panic to exercise the crash handler. Look for the crash report
//! # path printed on stderr after the panic.
//! cargo run --example service \
//!     --features "cli,config,logging,shutdown,crash,otel" \
//!     -- -C examples crash
//! ```
//!
//! # Observability
//!
//! Tracing events always flow to the JSONL log file under the platform log
//! directory. If `OTEL_EXPORTER_OTLP_ENDPOINT` is set to an OTLP/HTTP collector
//! URL, spans are also exported there (drop the `otel-grpc` feature in for
//! Tonic transport). Leave it unset and the service still runs fine — the
//! OTEL layer simply isn't added.
//!
//! ```sh
//! OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 \
//!     cargo run --example service \
//!     --features "cli,config,logging,shutdown,crash,otel" \
//!     -- -C examples run
//! ```
//!
//! # What it demonstrates
//!
//! - `#[tokio::main(flavor = "current_thread")]` — single-threaded runtime is enough
//!   for signal handling plus one work loop, and it matches librebar's tokio
//!   feature set (no `rt-multi-thread`).
//! - `app.shutdown_token().cancelled().await` as one arm of `tokio::select!`, so
//!   the work loop exits deterministically on SIGINT/SIGTERM.
//! - A post-shutdown grace window for any async cleanup you want to do before
//!   returning from `main`.
//! - `librebar::crash::install()` (wired via `.crash_handler()` on the builder)
//!   writing a dump to the platform cache dir when `main` panics.
#![allow(missing_docs)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    /// Log level used as the baseline when no `-q`/`-v` flag is passed.
    log_level: librebar::config::LogLevel,
    /// Interval between work ticks, in milliseconds.
    tick_interval_ms: u64,
    /// How long to wait after shutdown is signaled before returning from `main`.
    /// Real services use this window to flush buffers, drain queues, etc.
    shutdown_grace_ms: u64,
    /// Startup message printed to stdout when `run` begins.
    greeting: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: librebar::config::LogLevel::Info,
            tick_interval_ms: 1000,
            shutdown_grace_ms: 500,
            greeting: "service up".to_string(),
        }
    }
}

#[derive(Parser)]
#[command(name = "service", about = "Long-running librebar service example")]
struct Cli {
    #[command(flatten)]
    common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the service until SIGINT/SIGTERM.
    Run,
    /// Report loaded config, log destination, and OTEL wiring.
    Info,
    /// Intentionally panic to exercise the crash handler.
    Crash,
}

// Current-thread runtime: librebar's tokio feature set is
// ["rt", "macros", "signal", "sync"] — no rt-multi-thread.
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.common.apply_color();
    cli.common.apply_chdir()?;

    // Hardcoded: env!("CARGO_PKG_NAME") in a cargo example resolves to the
    // hosting crate (librebar), not the example. Hardcoding keeps config
    // discovery pointed at examples/service.toml.
    let app = librebar::init("service")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_cli(cli.common)
        .config::<Config>()
        .logging()
        .otel()
        .shutdown()
        .crash_handler()
        .start()?;

    match cli.command.unwrap_or(Command::Info) {
        Command::Run => run(&app).await,
        Command::Info => {
            print_info(&app);
            Ok(())
        }
        Command::Crash => {
            tracing::error!(phase = "pre-panic", "demonstrating crash handler");
            panic!("intentional panic to demonstrate crash handler");
        }
    }
}

async fn run(app: &librebar::App<Config>) -> Result<()> {
    let config = app.config();
    let mut token = app
        .shutdown_token()
        .expect("shutdown enabled on the builder");
    let mut ticker = tokio::time::interval(Duration::from_millis(config.tick_interval_ms));
    // Swallow the immediate first tick so "tick #1" happens one interval in.
    ticker.tick().await;

    println!(
        "{}: ticking every {}ms; Ctrl-C to stop",
        config.greeting, config.tick_interval_ms
    );
    tracing::info!(
        tick_interval_ms = config.tick_interval_ms,
        shutdown_grace_ms = config.shutdown_grace_ms,
        "service started",
    );

    let mut ticks: u64 = 0;
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                ticks += 1;
                tracing::debug!(tick = ticks, "work tick");
            }
            _ = token.cancelled() => {
                tracing::info!(ticks, "shutdown signal received");
                break;
            }
        }
    }

    // Real services use this window to drain async work, flush buffers, etc.
    // Keep it short in the example so `cargo run` returns promptly.
    tokio::time::sleep(Duration::from_millis(config.shutdown_grace_ms)).await;

    tracing::info!(ticks, "service stopped cleanly");
    println!("stopped cleanly after {ticks} ticks");
    Ok(())
}

fn print_info(app: &librebar::App<Config>) {
    let config = app.config();
    let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|v| !v.is_empty());

    println!("app:      {} v{}", app.app_name(), app.version());
    println!("sources:  {:?}", app.config_sources());
    println!(
        "log dir:  {:?}",
        librebar::logging::platform_log_dir(app.app_name())
    );
    println!(
        "crashes:  {}",
        librebar::crash::crash_dump_dir(app.app_name()).display()
    );
    match otel_endpoint {
        Some(ep) => println!("otel:     exporting to {ep}"),
        None => println!("otel:     disabled (set OTEL_EXPORTER_OTLP_ENDPOINT to enable)"),
    }
    println!(
        "config:   tick={}ms grace={}ms greeting={:?}",
        config.tick_interval_ms, config.shutdown_grace_ms, config.greeting
    );
}
