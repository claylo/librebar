//! Doctor + debug-bundle example — structured health checks and a shareable archive.
//!
//! Exercises librebar's `diagnostics` feature end-to-end: define two
//! realistic checks (config discovery, log-dir writability), run them via
//! `DoctorRunner`, and — on demand — bundle the results plus a config
//! snapshot into a tar.gz that a user can attach to a bug report.
//!
//! # Run
//!
//! ```sh
//! # Run the health checks and print a report:
//! cargo run --example doctor-bundle \
//!     --features "cli,config,logging,diagnostics" \
//!     -- -C examples doctor
//!
//! # Build a debug bundle (tar.gz) in the configured bundle directory:
//! cargo run --example doctor-bundle \
//!     --features "cli,config,logging,diagnostics" \
//!     -- -C examples bundle
//!
//! # Show configured paths and what would go where:
//! cargo run --example doctor-bundle \
//!     --features "cli,config,logging,diagnostics" \
//!     -- -C examples info
//! ```
//!
//! # What it demonstrates
//!
//! - Implementing `DoctorCheck` on struct types that *carry state captured
//!   at app startup* — `ConfigCheck` holds the `ConfigSources` we loaded,
//!   `LogDirCheck` holds the app name so it can resolve the platform dir.
//!   Checks are stateless at run-time; the pattern is to gather what you
//!   need up front and hand it to the check.
//! - Three `CheckStatus` values (`Ok`, `Warn`, `Error`) expressing different
//!   severities — the log-dir check emits `Warn` when the directory doesn't
//!   exist yet (librebar creates it on first write) vs `Error` when the
//!   platform can't resolve one at all.
//! - `DebugBundle` assembled from three kinds of input: `add_doctor_results`
//!   (the formatted report), `add_text` (a JSON snapshot of `ConfigSources`
//!   so a reader knows *which* files were active), and the archive writer
//!   via `.finish()` — returning the path for the user to attach.
#![allow(missing_docs)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use librebar::diagnostics::{CheckResult, CheckStatus, DebugBundle, DoctorCheck, DoctorRunner};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    /// Log level used as the baseline when no `-q`/`-v` flag is passed.
    log_level: librebar::config::LogLevel,
    /// Directory where `bundle` writes the tar.gz. Defaults to the
    /// platform log directory if unset.
    bundle_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: librebar::config::LogLevel::Info,
            bundle_dir: None,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "doctor-bundle",
    about = "Health checks and shareable debug bundles"
)]
struct Cli {
    #[command(flatten)]
    common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run health checks and print a report.
    Doctor,
    /// Run checks and write a tar.gz containing the report + config snapshot.
    Bundle,
    /// Show app state, resolved paths, and what bundle would include.
    Info,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.common.apply_color();
    cli.common.apply_chdir()?;

    // Hardcoded: env!("CARGO_PKG_NAME") inside a cargo example resolves
    // to the hosting crate (librebar), not "doctor-bundle".
    let app = librebar::init("doctor-bundle")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_cli(cli.common)
        .config::<Config>()
        .logging()
        .start()?;

    match cli.command.unwrap_or(Command::Info) {
        Command::Doctor => run_doctor(&app),
        Command::Bundle => run_bundle(&app),
        Command::Info => print_info(&app),
    }
}

fn build_runner(app: &librebar::App<Config>) -> DoctorRunner {
    let mut runner = DoctorRunner::new();
    runner.add(Box::new(ConfigCheck {
        sources: app.config_sources().clone(),
    }));
    runner.add(Box::new(LogDirCheck {
        app_name: app.app_name().to_string(),
    }));
    runner
}

fn run_doctor(app: &librebar::App<Config>) -> Result<()> {
    let runner = build_runner(app);
    let results = runner.run_all();
    print!("{}", DoctorRunner::format_report(&results));

    let summary = DoctorRunner::summarize(&results);
    if summary.failed > 0 {
        tracing::warn!(failed = summary.failed, "doctor reported failures");
        std::process::exit(1);
    }
    Ok(())
}

fn run_bundle(app: &librebar::App<Config>) -> Result<()> {
    let runner = build_runner(app);
    let results = runner.run_all();

    let dir = resolve_bundle_dir(app)?;
    let sources_json =
        serde_json::to_string_pretty(app.config_sources()).context("serializing config sources")?;

    let mut bundle = DebugBundle::new(app.app_name(), &dir);
    bundle
        .add_doctor_results(&results)
        .add_text("config-sources.json", &sources_json);
    let path = bundle.finish()?;

    tracing::info!(path = %path.display(), "bundle written");
    println!("bundle written: {}", path.display());
    Ok(())
}

fn print_info(app: &librebar::App<Config>) -> Result<()> {
    let config = app.config();
    let bundle_dir = resolve_bundle_dir(app)?;

    println!("app:         {} v{}", app.app_name(), app.version());
    println!("sources:     {:?}", app.config_sources());
    println!(
        "log dir:     {:?}",
        librebar::logging::platform_log_dir(app.app_name())
    );
    println!("bundle dir:  {}", bundle_dir.display());
    if config.bundle_dir.is_none() {
        println!("             (defaulted from log dir — set `bundle_dir` in config to override)");
    }
    println!("checks:      config-discovered, log-dir-writable");
    println!("bundle contents: doctor-report.txt, config-sources.json");
    Ok(())
}

fn resolve_bundle_dir(app: &librebar::App<Config>) -> Result<PathBuf> {
    if let Some(ref dir) = app.config().bundle_dir {
        return Ok(dir.clone());
    }
    librebar::logging::platform_log_dir(app.app_name())
        .context("no platform log dir available; set `bundle_dir` in config")
}

// ─── Checks ─────────────────────────────────────────────────────────

struct ConfigCheck {
    sources: librebar::config::ConfigSources,
}

impl DoctorCheck for ConfigCheck {
    fn name(&self) -> &str {
        "config-discovered"
    }

    fn category(&self) -> &str {
        "configuration"
    }

    fn run(&self) -> CheckResult {
        if self.sources.project_file.is_some() || self.sources.user_file.is_some() {
            CheckResult {
                status: CheckStatus::Ok,
                message: "config file discovered via project/user search".into(),
            }
        } else if !self.sources.explicit_files.is_empty() {
            CheckResult {
                status: CheckStatus::Ok,
                message: "config loaded from explicit file(s)".into(),
            }
        } else {
            CheckResult {
                status: CheckStatus::Warn,
                message: "no config file found; running with defaults".into(),
            }
        }
    }
}

struct LogDirCheck {
    app_name: String,
}

impl DoctorCheck for LogDirCheck {
    fn name(&self) -> &str {
        "log-dir-writable"
    }

    fn category(&self) -> &str {
        "observability"
    }

    fn run(&self) -> CheckResult {
        match librebar::logging::platform_log_dir(&self.app_name) {
            Some(dir) if dir.exists() => CheckResult {
                status: CheckStatus::Ok,
                message: format!("log dir exists at {}", dir.display()),
            },
            Some(dir) => CheckResult {
                status: CheckStatus::Warn,
                message: format!("log dir not yet created: {}", dir.display()),
            },
            None => CheckResult {
                status: CheckStatus::Error,
                message: "platform log directory could not be resolved".into(),
            },
        }
    }
}
