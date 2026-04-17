//! Liblibrebar: opinionated application foundation for Rust CLIs and services.
//!
//! Feature-gated modules for CLI, config, logging, and more.
//! Each module is usable independently (escape hatches) or wired
//! together through the builder. Enable only what you need.
//!
//! # Features
//!
//! Every module is behind a Cargo feature flag. No features are enabled
//! by default — you opt in to exactly what your application needs.
//!
//! ## Core application features
//!
//! | Feature | Module | Use when your app needs... |
//! |---------|--------|---------------------------|
//! | `cli` | [`cli`] | Clap-based CLI with `--quiet`, `--verbose`, `--color`, `--json` flags |
//! | `config` | [`config`] | Multi-format config discovery (TOML/YAML/JSON) with layered merge |
//! | `logging` | [`logging`] | Structured JSONL file logging with rotation |
//! | `shutdown` | [`shutdown`] | Graceful shutdown with SIGINT/SIGTERM handling |
//! | `crash` | [`crash`] | Structured JSON crash dumps on panic |
//!
//! ## Networking and data
//!
//! | Feature | Module | Use when your app needs... |
//! |---------|--------|---------------------------|
//! | `http` | [`http`] | HTTPS client with tracing, timeouts, user-agent (rustls + Mozilla CA roots) |
//! | `cache` | [`cache`] | File-based key-value cache with TTL (XDG cache directory) |
//! | `update` | [`update`] | "Update available" notifications via GitHub releases API |
//!
//! ## Integration features
//!
//! | Feature | Module | Use when your app needs... |
//! |---------|--------|---------------------------|
//! | `otel` | [`otel`] | OpenTelemetry tracing export (OTLP/HTTP) |
//! | `otel-grpc` | [`otel`] | OpenTelemetry via gRPC (adds Tonic transport) |
//! | `mcp` | [`mcp`] | Model Context Protocol server support |
//!
//! ## Operational features
//!
//! | Feature | Module | Use when your app needs... |
//! |---------|--------|---------------------------|
//! | `lockfile` | [`lockfile`] | Exclusive file locks to prevent concurrent instances |
//! | `dispatch` | [`dispatch`] | Git-style `{app}-{subcommand}` plugin lookup on PATH |
//! | `diagnostics` | [`diagnostics`] | `doctor` command framework + `.tar.gz` debug bundles |
//!
//! ## Benchmarking (dev-only)
//!
//! | Feature | Module | Use when your project needs... |
//! |---------|--------|-------------------------------|
//! | `bench` | [`bench`](mod@bench) | Wall-clock benchmarks via [divan](https://crates.io/crates/divan) (any platform) |
//! | `bench-gungraun` | [`bench`](mod@bench) | Instruction-count benchmarks via [gungraun](https://crates.io/crates/gungraun) / Valgrind (Linux/Intel) |
//!
//! ## Feature implications
//!
//! Some features automatically enable their dependencies:
//!
//! - `update` implies `http` + `cache` (needs both for network checks and 24h caching)
//! - `dispatch` implies `cli` (subcommand dispatch extends the CLI)
//! - `diagnostics` implies `config` + `logging` (bundles need config sources and log paths)
//! - `otel` implies `logging` (OTEL layer composes with the tracing subscriber)
//! - `otel-grpc` implies `otel`
//!
//! ## Typical feature sets
//!
//! ```toml
//! # Minimal CLI tool
//! librebar = { version = "0.1", features = ["cli", "config", "logging"] }
//!
//! # CLI tool with update checks
//! librebar = { version = "0.1", features = ["cli", "config", "logging", "shutdown", "update"] }
//!
//! # Long-running service with observability
//! librebar = { version = "0.1", features = ["cli", "config", "logging", "shutdown", "otel", "crash"] }
//!
//! # Plugin-extensible CLI (git-style subcommands)
//! librebar = { version = "0.1", features = ["cli", "config", "logging", "dispatch"] }
//! ```
//!
//! # Builder usage
//!
//! The builder wires enabled features together in the correct init order:
//!
//! ```ignore
//! use clap::Parser;
//!
//! #[derive(Parser)]
//! struct Cli {
//!     #[command(flatten)]
//!     pub common: librebar::cli::CommonArgs,
//!     #[command(subcommand)]
//!     pub command: Option<Commands>,
//! }
//!
//! let cli = Cli::parse();
//!
//! let app = librebar::init(env!("CARGO_PKG_NAME"))
//!     .with_version(env!("CARGO_PKG_VERSION"))
//!     .with_cli(cli.common)
//!     .config::<Config>()
//!     .logging()
//!     .shutdown()
//!     .crash_handler()
//!     .start()?;
//! ```
//!
//! Modules not wired through the builder (lockfile, http, cache, update,
//! dispatch, diagnostics, bench) are used directly via their public APIs.
//!
//! # Type-state pattern
//!
//! The builder uses a type-state transition to carry the config type:
//! - [`init()`] returns [`Builder`]
//! - [`Builder::config`] / [`Builder::config_from_file`] / [`Builder::with_config`]
//!   transition to [`ConfiguredBuilder<C>`]
//! - Each builder has its own [`start()`](Builder::start) returning the
//!   appropriate [`App`] type (`App<()>` or `App<C>`)
//!
//! # Initialization order
//!
//! [`start()`](Builder::start) initializes subsystems in this order:
//! 1. Load config (if requested via `.config::<C>()` or `.config_from_file()`)
//! 2. Initialize logging (reads verbosity from CLI flags if provided)
//! 3. Return [`App<C>`] holding all initialized state and guards
#![deny(unsafe_code)]

pub mod error;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "config")]
pub mod config;

#[cfg(feature = "logging")]
pub mod logging;

#[cfg(feature = "otel")]
pub mod otel;

#[cfg(feature = "shutdown")]
pub mod shutdown;

#[cfg(feature = "crash")]
pub mod crash;

#[cfg(feature = "mcp")]
pub mod mcp;

#[cfg(feature = "lockfile")]
pub mod lockfile;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "cache")]
pub mod cache;

#[cfg(feature = "update")]
pub mod update;

#[cfg(feature = "dispatch")]
pub mod dispatch;

#[cfg(feature = "diagnostics")]
pub mod diagnostics;

#[cfg(any(feature = "bench", feature = "bench-gungraun"))]
pub mod bench;

#[cfg(feature = "logging")]
use tracing_subscriber::layer::SubscriberExt;
#[cfg(feature = "logging")]
use tracing_subscriber::util::SubscriberInitExt;

pub use error::{Error, Result};

// ─── App ────────────────────────────────────────────────────────────

/// The initialized application state.
///
/// Holds config, CLI args, and guards for logging/tracing.
/// `C` is the user's config type (defaults to `()` when config is not used).
pub struct App<C = ()> {
    app_name: String,
    version: String,
    config: C,
    #[cfg(feature = "config")]
    config_sources: config::ConfigSources,
    #[cfg(feature = "cli")]
    cli: cli::CommonArgs,
    #[cfg(feature = "shutdown")]
    shutdown_handle: Option<shutdown::ShutdownHandle>,
    #[cfg(feature = "otel")]
    _otel_guard: Option<otel::OtelGuard>,
    #[cfg(feature = "logging")]
    _logging_guard: Option<logging::LoggingGuard>,
}

impl<C> App<C> {
    /// Returns a reference to the loaded configuration.
    pub const fn config(&self) -> &C {
        &self.config
    }

    /// Returns the application name.
    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    /// Returns the application version.
    pub fn version(&self) -> &str {
        &self.version
    }
}

#[cfg(feature = "config")]
impl<C> App<C> {
    /// Returns metadata about which config files were loaded.
    pub const fn config_sources(&self) -> &config::ConfigSources {
        &self.config_sources
    }
}

#[cfg(feature = "cli")]
impl<C> App<C> {
    /// Returns the parsed common CLI arguments.
    pub const fn cli(&self) -> &cli::CommonArgs {
        &self.cli
    }
}

#[cfg(feature = "shutdown")]
impl<C> App<C> {
    /// Get a shutdown token for waiting on graceful shutdown.
    ///
    /// Returns `None` if `.shutdown()` was not called on the builder.
    pub fn shutdown_token(&self) -> Option<shutdown::ShutdownToken> {
        self.shutdown_handle.as_ref().map(|h| h.token())
    }

    /// Trigger shutdown programmatically.
    pub fn shutdown(&self) {
        if let Some(ref handle) = self.shutdown_handle {
            handle.shutdown();
        }
    }
}

// ─── Builder ────────────────────────────────────────────────────────

/// Shared state for subsystem toggles.
///
/// Used internally by [`Builder`] and [`ConfiguredBuilder`]. Both outer
/// types delegate their builder methods here via [`builder_methods!`].
struct BuilderInner {
    app_name: String,
    version: Option<String>,
    #[cfg(feature = "cli")]
    cli: Option<cli::CommonArgs>,
    #[cfg(feature = "logging")]
    enable_logging: bool,
    #[cfg(feature = "logging")]
    log_dir: Option<std::path::PathBuf>,
    #[cfg(feature = "otel")]
    enable_otel: bool,
    #[cfg(feature = "shutdown")]
    enable_shutdown: bool,
    #[cfg(feature = "crash")]
    enable_crash: bool,
}

/// Intermediate result from [`BuilderInner::init_subsystems`].
struct SubsystemInit {
    app_name: String,
    version: String,
    #[cfg(feature = "cli")]
    cli: cli::CommonArgs,
    #[cfg(feature = "shutdown")]
    shutdown_handle: Option<shutdown::ShutdownHandle>,
    #[cfg(feature = "otel")]
    otel_guard: Option<otel::OtelGuard>,
    #[cfg(feature = "logging")]
    logging_guard: Option<logging::LoggingGuard>,
}

impl BuilderInner {
    #[cfg(all(feature = "logging", feature = "cli"))]
    fn cli_flags(&self) -> (bool, u8) {
        self.cli
            .as_ref()
            .map_or((false, 0), |c| (c.quiet, c.verbose))
    }

    #[cfg(all(feature = "logging", not(feature = "cli")))]
    fn cli_flags(&self) -> (bool, u8) {
        (false, 0)
    }

    /// Initialize tracing, shutdown, and crash subsystems.
    ///
    /// Consumes the inner builder and returns the initialized
    /// subsystem handles, ready to be assembled into an [`App`].
    fn init_subsystems(self) -> Result<SubsystemInit> {
        #[cfg(feature = "logging")]
        let cli_flags = self.cli_flags();
        #[cfg(feature = "logging")]
        let do_logging = self.enable_logging;
        let app_version = self
            .version
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

        // Build layers
        #[cfg(feature = "logging")]
        let (log_layer, log_guard) = if do_logging {
            let log_cfg =
                logging::LoggingConfig::from_app_name(&self.app_name).with_log_dir(self.log_dir);
            let (layer, guard) = logging::build_json_layer(&log_cfg)?;
            (Some(layer), Some(logging::LoggingGuard::from_guard(guard)))
        } else {
            (None, None)
        };

        #[cfg(feature = "otel")]
        let (otel_layer, otel_guard) = if self.enable_otel {
            let otel_cfg = otel::OtelConfig::from_app_name(&self.app_name, &app_version);
            otel::build_otel_layer(&otel_cfg)?
        } else {
            (None, None)
        };

        // Compose tracing subscriber
        #[cfg(all(feature = "logging", not(feature = "otel")))]
        if log_layer.is_some() {
            let (quiet, verbose) = cli_flags;
            let filter = logging::env_filter(quiet, verbose, "info");
            tracing_subscriber::registry()
                .with(filter)
                .with(log_layer)
                .try_init()
                .map_err(Error::TracingInit)?;
        }

        #[cfg(all(feature = "logging", feature = "otel"))]
        if log_layer.is_some() || otel_layer.is_some() {
            let (quiet, verbose) = cli_flags;
            let filter = logging::env_filter(quiet, verbose, "info");
            let mut layers: Vec<
                Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>,
            > = Vec::new();
            layers.push(Box::new(filter));
            if let Some(l) = log_layer {
                layers.push(Box::new(l));
            }
            if let Some(l) = otel_layer {
                layers.push(l);
            }
            tracing_subscriber::registry()
                .with(layers)
                .try_init()
                .map_err(Error::TracingInit)?;
        }

        #[cfg(feature = "shutdown")]
        let shutdown_handle = if self.enable_shutdown {
            let handle = shutdown::ShutdownHandle::new();
            handle.register_signals()?;
            Some(handle)
        } else {
            None
        };

        #[cfg(feature = "crash")]
        if self.enable_crash {
            crash::install(&self.app_name, &app_version);
        }

        Ok(SubsystemInit {
            app_name: self.app_name,
            version: app_version,
            #[cfg(feature = "cli")]
            cli: self.cli.unwrap_or_else(default_cli),
            #[cfg(feature = "shutdown")]
            shutdown_handle,
            #[cfg(feature = "otel")]
            otel_guard,
            #[cfg(feature = "logging")]
            logging_guard: log_guard,
        })
    }
}

/// Shared builder methods for subsystem toggles.
///
/// Both [`Builder`] and [`ConfiguredBuilder`] delegate to the same
/// [`BuilderInner`] fields. This macro generates the forwarding
/// methods so the documentation and behavior stay in sync.
macro_rules! builder_methods {
    () => {
        /// Provide parsed CLI common arguments.
        #[cfg(feature = "cli")]
        pub fn with_cli(mut self, common: cli::CommonArgs) -> Self {
            self.inner.cli = Some(common);
            self
        }

        /// Enable JSONL logging.
        #[cfg(feature = "logging")]
        pub const fn logging(mut self) -> Self {
            self.inner.enable_logging = true;
            self
        }

        /// Set the log directory explicitly.
        #[cfg(feature = "logging")]
        pub fn with_log_dir(mut self, dir: std::path::PathBuf) -> Self {
            self.inner.log_dir = Some(dir);
            self
        }

        /// Enable OpenTelemetry tracing export.
        #[cfg(feature = "otel")]
        pub const fn otel(mut self) -> Self {
            self.inner.enable_otel = true;
            self
        }

        /// Enable graceful shutdown with signal handling.
        #[cfg(feature = "shutdown")]
        pub const fn shutdown(mut self) -> Self {
            self.inner.enable_shutdown = true;
            self
        }

        /// Install a structured crash handler (panic hook with dump files).
        #[cfg(feature = "crash")]
        pub const fn crash_handler(mut self) -> Self {
            self.inner.enable_crash = true;
            self
        }

        /// Set the application version for crash dumps and OTEL resource
        /// attributes.
        ///
        /// If not set, crash and OTEL use the librebar crate version.
        pub fn with_version(mut self, version: &str) -> Self {
            self.inner.version = Some(version.to_string());
            self
        }
    };
}

/// Start building a librebar application.
///
/// ```ignore
/// let app = librebar::init(env!("CARGO_PKG_NAME"))
///     .with_cli(cli.common)
///     .config::<Config>()
///     .logging()
///     .start()?;
/// ```
pub fn init(app_name: &str) -> Builder {
    Builder {
        inner: BuilderInner {
            app_name: app_name.to_string(),
            version: None,
            #[cfg(feature = "cli")]
            cli: None,
            #[cfg(feature = "logging")]
            enable_logging: false,
            #[cfg(feature = "logging")]
            log_dir: None,
            #[cfg(feature = "otel")]
            enable_otel: false,
            #[cfg(feature = "shutdown")]
            enable_shutdown: false,
            #[cfg(feature = "crash")]
            enable_crash: false,
        },
    }
}

/// Builder for librebar application initialization.
///
/// Wires config discovery, logging setup, and CLI args in the correct
/// initialization order.
pub struct Builder {
    inner: BuilderInner,
}

impl Builder {
    builder_methods!();

    /// Finalize initialization without config.
    ///
    /// Returns `App<()>`. Use [`config_from_file`](Self::config_from_file)
    /// or [`with_config`](Self::with_config) to get `App<C>` with a typed config.
    ///
    /// # Errors
    ///
    /// Returns an error if logging initialization fails.
    pub fn start(self) -> Result<App> {
        let sub = self.inner.init_subsystems()?;
        Ok(App {
            app_name: sub.app_name,
            version: sub.version,
            config: (),
            #[cfg(feature = "config")]
            config_sources: config::ConfigSources::default(),
            #[cfg(feature = "cli")]
            cli: sub.cli,
            #[cfg(feature = "shutdown")]
            shutdown_handle: sub.shutdown_handle,
            #[cfg(feature = "otel")]
            _otel_guard: sub.otel_guard,
            #[cfg(feature = "logging")]
            _logging_guard: sub.logging_guard,
        })
    }
}

// ─── Config builder transitions ─────────────────────────────────────

#[cfg(feature = "config")]
impl Builder {
    /// Load config from a specific file.
    ///
    /// Transitions the builder to [`ConfiguredBuilder<C>`] which holds
    /// the config type information.
    pub fn config_from_file<C>(self, path: &camino::Utf8Path) -> ConfiguredBuilder<C>
    where
        C: serde::de::DeserializeOwned + Default + serde::Serialize,
    {
        ConfiguredBuilder {
            inner: self.inner,
            config_source: CfgSource::File(path.to_path_buf()),
        }
    }

    /// Enable config discovery from standard locations.
    ///
    /// Transitions the builder to [`ConfiguredBuilder<C>`].
    pub fn config<C>(self) -> ConfiguredBuilder<C>
    where
        C: serde::de::DeserializeOwned + Default + serde::Serialize,
    {
        ConfiguredBuilder {
            inner: self.inner,
            config_source: CfgSource::Discover,
        }
    }

    /// Provide a pre-loaded config (escape hatch).
    ///
    /// Transitions the builder to [`ConfiguredBuilder<C>`].
    pub fn with_config<C>(self, config: C) -> ConfiguredBuilder<C>
    where
        C: serde::Serialize,
    {
        ConfiguredBuilder {
            inner: self.inner,
            config_source: CfgSource::Preloaded(config),
        }
    }
}

// ─── ConfiguredBuilder ──────────────────────────────────────────────

/// How config should be loaded when `start()` is called.
///
/// - `Discover`: walk up from cwd looking for config files, merge with user config
/// - `File`: load from a specific path (skips user config)
/// - `Preloaded`: use a config value provided directly (no file I/O)
#[cfg(feature = "config")]
enum CfgSource<C> {
    Discover,
    File(camino::Utf8PathBuf),
    Preloaded(C),
}

/// Builder with config type information.
///
/// Created by [`Builder::config`], [`Builder::config_from_file`],
/// or [`Builder::with_config`]. Call [`.start()`](Self::start) to finalize.
#[cfg(feature = "config")]
pub struct ConfiguredBuilder<C> {
    inner: BuilderInner,
    config_source: CfgSource<C>,
}

#[cfg(feature = "config")]
impl<C> ConfiguredBuilder<C> {
    builder_methods!();
}

#[cfg(feature = "config")]
impl<C> ConfiguredBuilder<C>
where
    C: serde::de::DeserializeOwned + Default + serde::Serialize,
{
    /// Finalize initialization with config.
    ///
    /// # Errors
    ///
    /// Returns an error if config loading or logging initialization fails.
    pub fn start(self) -> Result<App<C>> {
        let (config, sources) = match self.config_source {
            CfgSource::Discover => {
                let cwd = std::env::current_dir().map_err(crate::Error::Io)?;
                let cwd = camino::Utf8PathBuf::try_from(cwd).map_err(|e| {
                    crate::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "current directory is not valid UTF-8: {}",
                            e.into_path_buf().display()
                        ),
                    ))
                })?;
                config::ConfigLoader::new(&self.inner.app_name)
                    .with_project_search(&cwd)
                    .load::<C>()?
            }
            CfgSource::File(path) => config::ConfigLoader::new(&self.inner.app_name)
                .with_user_config(false)
                .with_file(&path)
                .load::<C>()?,
            CfgSource::Preloaded(config) => (config, config::ConfigSources::default()),
        };

        let sub = self.inner.init_subsystems()?;
        Ok(App {
            app_name: sub.app_name,
            version: sub.version,
            config,
            config_sources: sources,
            #[cfg(feature = "cli")]
            cli: sub.cli,
            #[cfg(feature = "shutdown")]
            shutdown_handle: sub.shutdown_handle,
            #[cfg(feature = "otel")]
            _otel_guard: sub.otel_guard,
            #[cfg(feature = "logging")]
            _logging_guard: sub.logging_guard,
        })
    }
}

// ─── Helpers ────────────────────────────────────────────────────────

#[cfg(feature = "cli")]
const fn default_cli() -> cli::CommonArgs {
    cli::CommonArgs {
        version_only: false,
        chdir: None,
        quiet: false,
        verbose: 0,
        color: cli::ColorChoice::Auto,
        json: false,
    }
}
