//! Rebar: Rust application foundation crate.
//!
//! Feature-gated modules for CLI, config, logging, and more.
//! Each module is usable independently (escape hatches) or wired
//! together through the builder.
//!
//! # Builder usage
//!
//! ```ignore
//! use clap::Parser;
//!
//! #[derive(Parser)]
//! struct Cli {
//!     #[command(flatten)]
//!     pub common: rebar::cli::CommonArgs,
//!     #[command(subcommand)]
//!     pub command: Option<Commands>,
//! }
//!
//! let cli = Cli::parse();
//!
//! let app = rebar::init(env!("CARGO_PKG_NAME"))
//!     .with_cli(cli.common)
//!     .config::<Config>()
//!     .logging()
//!     .start()?;
//! ```
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

/// Start building a rebar application.
///
/// ```ignore
/// let app = rebar::init(env!("CARGO_PKG_NAME"))
///     .with_cli(cli.common)
///     .config::<Config>()
///     .logging()
///     .start()?;
/// ```
pub fn init(app_name: &str) -> Builder {
    Builder {
        app_name: app_name.to_string(),
        #[cfg(feature = "cli")]
        cli: None,
        #[cfg(feature = "logging")]
        enable_logging: false,
        #[cfg(feature = "otel")]
        enable_otel: false,
        #[cfg(feature = "shutdown")]
        enable_shutdown: false,
        #[cfg(feature = "crash")]
        enable_crash: false,
    }
}

/// Builder for rebar application initialization.
///
/// Wires config discovery, logging setup, and CLI args in the correct
/// initialization order.
pub struct Builder {
    app_name: String,
    #[cfg(feature = "cli")]
    cli: Option<cli::CommonArgs>,
    #[cfg(feature = "logging")]
    enable_logging: bool,
    #[cfg(feature = "otel")]
    enable_otel: bool,
    #[cfg(feature = "shutdown")]
    enable_shutdown: bool,
    #[cfg(feature = "crash")]
    enable_crash: bool,
}

impl Builder {
    /// Provide parsed CLI common arguments.
    #[cfg(feature = "cli")]
    pub fn with_cli(mut self, common: cli::CommonArgs) -> Self {
        self.cli = Some(common);
        self
    }

    /// Enable JSONL logging.
    #[cfg(feature = "logging")]
    pub const fn logging(mut self) -> Self {
        self.enable_logging = true;
        self
    }

    /// Enable OpenTelemetry tracing export.
    #[cfg(feature = "otel")]
    pub const fn otel(mut self) -> Self {
        self.enable_otel = true;
        self
    }

    /// Enable graceful shutdown with signal handling.
    #[cfg(feature = "shutdown")]
    pub const fn shutdown(mut self) -> Self {
        self.enable_shutdown = true;
        self
    }

    /// Install a structured crash handler (panic hook with dump files).
    #[cfg(feature = "crash")]
    pub const fn crash_handler(mut self) -> Self {
        self.enable_crash = true;
        self
    }

    /// Finalize initialization without config.
    ///
    /// Returns `App<()>`. Use [`config_from_file`](Self::config_from_file)
    /// or [`with_config`](Self::with_config) to get `App<C>` with a typed config.
    ///
    /// # Errors
    ///
    /// Returns an error if logging initialization fails.
    pub fn start(self) -> Result<App> {
        // Build layers
        #[cfg(feature = "logging")]
        let (log_layer, log_guard) = if self.enable_logging {
            let log_cfg = logging::LoggingConfig::from_app_name(&self.app_name);
            let (layer, guard) = logging::build_json_layer(&log_cfg)?;
            (Some(layer), Some(logging::LoggingGuard::from_guard(guard)))
        } else {
            (None, None)
        };

        #[cfg(feature = "otel")]
        let (otel_layer, otel_guard) = if self.enable_otel {
            let otel_cfg =
                otel::OtelConfig::from_app_name(&self.app_name, env!("CARGO_PKG_VERSION"));
            otel::build_otel_layer(&otel_cfg)?
        } else {
            (None, None)
        };

        // Compose tracing subscriber
        #[cfg(all(feature = "logging", not(feature = "otel")))]
        if log_layer.is_some() {
            let (quiet, verbose) = self.cli_flags();
            let filter = logging::env_filter(quiet, verbose, "info");
            tracing_subscriber::registry()
                .with(filter)
                .with(log_layer)
                .init();
        }

        #[cfg(all(feature = "logging", feature = "otel"))]
        if log_layer.is_some() || otel_layer.is_some() {
            let (quiet, verbose) = self.cli_flags();
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
            tracing_subscriber::registry().with(layers).init();
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
            crash::install(&self.app_name, env!("CARGO_PKG_VERSION"));
        }

        Ok(App {
            app_name: self.app_name,
            config: (),
            #[cfg(feature = "config")]
            config_sources: config::ConfigSources::default(),
            #[cfg(feature = "cli")]
            cli: self.cli.unwrap_or_else(default_cli),
            #[cfg(feature = "shutdown")]
            shutdown_handle,
            #[cfg(feature = "otel")]
            _otel_guard: otel_guard,
            #[cfg(feature = "logging")]
            _logging_guard: log_guard,
        })
    }

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
            app_name: self.app_name,
            #[cfg(feature = "cli")]
            cli: self.cli,
            #[cfg(feature = "logging")]
            enable_logging: self.enable_logging,
            #[cfg(feature = "otel")]
            enable_otel: self.enable_otel,
            #[cfg(feature = "shutdown")]
            enable_shutdown: self.enable_shutdown,
            #[cfg(feature = "crash")]
            enable_crash: self.enable_crash,
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
            app_name: self.app_name,
            #[cfg(feature = "cli")]
            cli: self.cli,
            #[cfg(feature = "logging")]
            enable_logging: self.enable_logging,
            #[cfg(feature = "otel")]
            enable_otel: self.enable_otel,
            #[cfg(feature = "shutdown")]
            enable_shutdown: self.enable_shutdown,
            #[cfg(feature = "crash")]
            enable_crash: self.enable_crash,
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
            app_name: self.app_name,
            #[cfg(feature = "cli")]
            cli: self.cli,
            #[cfg(feature = "logging")]
            enable_logging: self.enable_logging,
            #[cfg(feature = "otel")]
            enable_otel: self.enable_otel,
            #[cfg(feature = "shutdown")]
            enable_shutdown: self.enable_shutdown,
            #[cfg(feature = "crash")]
            enable_crash: self.enable_crash,
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
    app_name: String,
    #[cfg(feature = "cli")]
    cli: Option<cli::CommonArgs>,
    #[cfg(feature = "logging")]
    enable_logging: bool,
    #[cfg(feature = "otel")]
    enable_otel: bool,
    #[cfg(feature = "shutdown")]
    enable_shutdown: bool,
    #[cfg(feature = "crash")]
    enable_crash: bool,
    config_source: CfgSource<C>,
}

#[cfg(feature = "config")]
impl<C> ConfiguredBuilder<C> {
    /// Provide parsed CLI common arguments.
    #[cfg(feature = "cli")]
    pub fn with_cli(mut self, common: cli::CommonArgs) -> Self {
        self.cli = Some(common);
        self
    }

    /// Enable JSONL logging.
    #[cfg(feature = "logging")]
    pub const fn logging(mut self) -> Self {
        self.enable_logging = true;
        self
    }

    /// Enable OpenTelemetry tracing export.
    #[cfg(feature = "otel")]
    pub const fn otel(mut self) -> Self {
        self.enable_otel = true;
        self
    }

    /// Enable graceful shutdown with signal handling.
    #[cfg(feature = "shutdown")]
    pub const fn shutdown(mut self) -> Self {
        self.enable_shutdown = true;
        self
    }

    /// Install a structured crash handler (panic hook with dump files).
    #[cfg(feature = "crash")]
    pub const fn crash_handler(mut self) -> Self {
        self.enable_crash = true;
        self
    }

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
        // Capture flags before moving fields out of self
        #[cfg(feature = "logging")]
        let cli_flags = self.cli_flags();
        #[cfg(feature = "logging")]
        let do_logging = self.enable_logging;
        #[cfg(feature = "otel")]
        let do_otel = self.enable_otel;
        #[cfg(feature = "shutdown")]
        let do_shutdown = self.enable_shutdown;

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
                config::ConfigLoader::new(&self.app_name)
                    .with_project_search(&cwd)
                    .load::<C>()?
            }
            CfgSource::File(path) => config::ConfigLoader::new(&self.app_name)
                .with_user_config(false)
                .with_file(&path)
                .load::<C>()?,
            CfgSource::Preloaded(config) => (config, config::ConfigSources::default()),
        };

        // Build layers
        #[cfg(feature = "logging")]
        let (log_layer, log_guard) = if do_logging {
            let log_cfg = logging::LoggingConfig::from_app_name(&self.app_name);
            let (layer, guard) = logging::build_json_layer(&log_cfg)?;
            (Some(layer), Some(logging::LoggingGuard::from_guard(guard)))
        } else {
            (None, None)
        };

        #[cfg(feature = "otel")]
        let (otel_layer, otel_guard) = if do_otel {
            let otel_cfg =
                otel::OtelConfig::from_app_name(&self.app_name, env!("CARGO_PKG_VERSION"));
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
                .init();
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
            tracing_subscriber::registry().with(layers).init();
        }

        #[cfg(feature = "shutdown")]
        let shutdown_handle = if do_shutdown {
            let handle = shutdown::ShutdownHandle::new();
            handle.register_signals()?;
            Some(handle)
        } else {
            None
        };

        #[cfg(feature = "crash")]
        if self.enable_crash {
            crash::install(&self.app_name, env!("CARGO_PKG_VERSION"));
        }

        Ok(App {
            app_name: self.app_name,
            config,
            config_sources: sources,
            #[cfg(feature = "cli")]
            cli: self.cli.unwrap_or_else(default_cli),
            #[cfg(feature = "shutdown")]
            shutdown_handle,
            #[cfg(feature = "otel")]
            _otel_guard: otel_guard,
            #[cfg(feature = "logging")]
            _logging_guard: log_guard,
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
