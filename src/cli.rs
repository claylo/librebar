//! CLI argument types shared across librebar-based applications.
//!
//! Provides [`CommonArgs`] for standard flags and typed output/color choices.
//! Consumers embed these into their own clap-derived structs via
//! `#[command(flatten)]`.
//!
//! After parsing, call [`CommonArgs::apply`] once. It performs every startup
//! side effect these flags imply, in the order they have to happen in, and
//! reports whether the process should keep going.
//!
//! Prefer [`parse`] over `clap::Parser::parse`: it adds machine-readable CLI
//! Spec introspection and stable shell completions before normal application
//! startup. [`generate_manpages`] renders the same augmented command tree for
//! release packaging.

use clap::Args;
use std::io::IsTerminal;
use std::path::PathBuf;

/// Re-export of [`clap`], Librebar's CLI extension API.
pub use clap;

mod artifacts;
mod parse;
mod schema;

pub use artifacts::{ArtifactError, generate_manpages, render_manpage};
pub use parse::{ParseOutcome, command, parse, parse_with, try_parse_from};
pub use schema::{
    CLI_SPEC_VERSION, CommandExample, CommandMetadata, ErrorMetadata, OutcomeMetadata, OutputField,
    SchemaDocument, SchemaError, SchemaMetadata, Stability, schema_for,
};

/// Color output preference.
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum ColorChoice {
    /// Detect terminal capabilities automatically.
    #[default]
    Auto,
    /// Always emit colors.
    Always,
    /// Never emit colors.
    Never,
}

impl ColorChoice {
    /// Configure global color output based on this choice.
    ///
    /// Call this once at startup to set the color mode for owo-colors.
    pub fn apply(self) {
        match self {
            Self::Auto => {} // owo-colors auto-detects by default
            Self::Always => owo_colors::set_override(true),
            Self::Never => owo_colors::set_override(false),
        }
    }
}

/// Output format requested by the user.
///
/// [`Self::Auto`] preserves the distinction between an explicit format and
/// terminal detection. Applications should render using the resolved value
/// returned by [`CommonArgs::output_format`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    /// Use text on a terminal and JSON when stdout is redirected.
    #[default]
    Auto,
    /// Human-readable text.
    Text,
    /// Machine-readable JSON.
    Json,
}

impl OutputFormat {
    /// Resolve this request for a known stdout terminal state.
    pub const fn resolve_for(self, stdout_is_terminal: bool) -> ResolvedOutputFormat {
        match self {
            Self::Auto if stdout_is_terminal => ResolvedOutputFormat::Text,
            Self::Auto | Self::Json => ResolvedOutputFormat::Json,
            Self::Text => ResolvedOutputFormat::Text,
        }
    }
}

/// Concrete output format after terminal detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedOutputFormat {
    /// Human-readable text.
    Text,
    /// Machine-readable JSON.
    Json,
}

/// Common CLI arguments shared across all librebar-based applications.
///
/// Embed in your app's CLI struct with `#[command(flatten)]`:
///
/// ```
/// use clap::{Parser, Subcommand};
///
/// #[derive(Parser)]
/// struct MyCli {
///     #[command(flatten)]
///     pub common: librebar::cli::CommonArgs,
///     #[command(subcommand)]
///     pub command: Option<MyCommands>,
/// }
///
/// #[derive(Subcommand)]
/// enum MyCommands { Run }
/// ```
///
/// # Reserved short flags
///
/// `-C`, `-q` and `-v` are declared `global = true`, so they propagate to every
/// subcommand and are unavailable to yours. Redeclaring one is a clap conflict,
/// which surfaces as a panic on first run rather than a compile error — design
/// your subcommand flags around these three.
///
/// Every flag here is global, so all of them are accepted after a subcommand
/// name as well as before it. `myapp sub --version-only` prints the version and
/// exits without running `sub`, the same way `-C` applies wherever it appears.
///
/// The rustdoc above is for readers of this crate, not for users of yours.
/// `#[command(about = None, long_about = None)]` keeps clap from adopting it:
/// a flattened struct's doc comment otherwise becomes the *consuming* binary's
/// help description, so `myapp --help` would open by explaining librebar.
#[derive(Args, Clone, Debug)]
#[command(about = None, long_about = None)]
pub struct CommonArgs {
    /// Print only the version number (for scripting).
    #[arg(long, global = true)]
    pub version_only: bool,

    /// Run as if started in DIR.
    #[arg(short = 'C', long, global = true)]
    pub chdir: Option<PathBuf>,

    /// Read configuration from FILE instead of discovering it.
    #[arg(short = 'c', long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Only print errors (suppresses warnings/info).
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// More detail (repeatable; e.g. -vv).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Colorize output.
    #[arg(long, global = true, value_enum, default_value_t)]
    pub color: ColorChoice,

    /// Output format; auto uses text on a terminal and JSON when redirected.
    #[arg(long, global = true, value_enum, default_value_t)]
    pub format: OutputFormat,

    /// Compatibility spelling for --format json.
    #[arg(long = "json", global = true, hide = true, conflicts_with = "format")]
    pub(crate) legacy_json: bool,
}

/// Whether the process should continue after [`CommonArgs::apply`].
///
/// Some flags are complete requests in themselves — `--version-only` asks a
/// question and wants nothing else to happen. `apply` answers them and hands
/// back [`Startup::Exit`] so the caller can return before doing real work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "an ignored `Startup::Exit` means the process keeps running after \
              a flag like --version-only was already handled"]
pub enum Startup {
    /// No terminal flag was passed; proceed with normal startup.
    Continue,
    /// A flag was handled in full. Exit successfully without further work.
    Exit,
}

impl Startup {
    /// Returns `true` if the caller should exit immediately.
    pub const fn is_exit(self) -> bool {
        matches!(self, Self::Exit)
    }

    /// Returns `true` if the caller should proceed with startup.
    pub const fn is_continue(self) -> bool {
        matches!(self, Self::Continue)
    }
}

impl CommonArgs {
    /// Resolve the requested output format using stdout's terminal state.
    pub fn output_format(&self) -> ResolvedOutputFormat {
        self.output_format_for(std::io::stdout().is_terminal())
    }

    /// Resolve the requested output format for an explicit terminal state.
    ///
    /// This is useful for deterministic rendering tests. Normal applications
    /// should call [`output_format`](Self::output_format).
    pub const fn output_format_for(&self, stdout_is_terminal: bool) -> ResolvedOutputFormat {
        if self.legacy_json {
            ResolvedOutputFormat::Json
        } else {
            self.format.resolve_for(stdout_is_terminal)
        }
    }

    /// The `--config` path as UTF-8, ready for the config loader.
    ///
    /// Returns `Ok(None)` when the flag was not supplied, which callers should
    /// treat as "discover config normally":
    ///
    /// ```no_run
    /// # use clap::Parser;
    /// # #[derive(Parser)]
    /// # struct Cli {
    /// #     #[command(flatten)]
    /// #     common: librebar::cli::CommonArgs,
    /// # }
    /// # fn main() -> librebar::Result<()> {
    /// # let cli = Cli::parse();
    /// let mut loader = librebar::config::ConfigLoader::new("myapp");
    /// if let Some(path) = cli.common.config_path()? {
    ///     loader = loader.with_file(&path);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::PathNotUtf8`](crate::Error::PathNotUtf8) if the path
    /// is not valid UTF-8. librebar's config API is `camino`-based, so a path
    /// it cannot represent has to be rejected rather than silently skipped.
    #[cfg(feature = "config")]
    pub fn config_path(&self) -> crate::Result<Option<camino::Utf8PathBuf>> {
        self.config
            .as_ref()
            .map(|path| {
                camino::Utf8PathBuf::from_path_buf(path.clone()).map_err(|path| {
                    crate::Error::PathNotUtf8 {
                        path: path.to_string_lossy().into_owned(),
                    }
                })
            })
            .transpose()
    }

    /// Perform every startup side effect these flags imply.
    ///
    /// This is the one call an application needs after parsing. It applies
    /// color settings, answers `--version-only`, and honors `-C/--chdir`, in
    /// that order — `--version-only` is a scripting query, so it is answered
    /// before touching the filesystem, and the directory change has to land
    /// before config discovery walks up from the current directory.
    ///
    /// `version` is the *application's* version, normally
    /// `env!("CARGO_PKG_VERSION")`. librebar cannot read it for you: the same
    /// macro expanded inside this crate would yield librebar's own version.
    /// Pass the same string to [`Builder::with_version`](crate::Builder::with_version).
    ///
    /// ```no_run
    /// use clap::Parser;
    ///
    /// #[derive(Parser)]
    /// struct Cli {
    ///     #[command(flatten)]
    ///     common: librebar::cli::CommonArgs,
    /// }
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let cli = Cli::parse();
    /// if cli.common.apply(env!("CARGO_PKG_VERSION"))?.is_exit() {
    ///     return Ok(());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The granular [`apply_color`](Self::apply_color) and
    /// [`apply_chdir`](Self::apply_chdir) remain available for applications
    /// that need a different order or handle `--version-only` themselves.
    ///
    /// # Errors
    ///
    /// Returns an error if `--version-only` cannot write to stdout, or if
    /// `--chdir` names a directory that does not exist or is not accessible.
    pub fn apply(&self, version: &str) -> std::io::Result<Startup> {
        self.apply_with_writer(version, std::io::stdout().lock())
    }

    fn apply_with_writer(
        &self,
        version: &str,
        mut stdout: impl std::io::Write,
    ) -> std::io::Result<Startup> {
        // Colors first, so anything printed afterwards is styled correctly.
        self.apply_color();

        // Answered before `apply_chdir` so that `--version-only` cannot fail
        // on an unrelated bad `-C`.
        if self.version_only {
            writeln!(stdout, "{version}")?;
            return Ok(Startup::Exit);
        }

        self.apply_chdir()?;

        Ok(Startup::Continue)
    }

    /// Apply color settings globally. Call once at startup.
    ///
    /// Most applications should call [`apply`](Self::apply) instead, which
    /// does this and the rest of the startup sequence together.
    pub fn apply_color(&self) {
        self.color.apply();
    }

    /// Change the working directory if `--chdir` was specified.
    ///
    /// Most applications should call [`apply`](Self::apply) instead, which
    /// does this and the rest of the startup sequence together.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory does not exist or is not accessible.
    /// The message names the flag and the path, since a bare "No such file or
    /// directory" gives the user nothing to act on. The
    /// [`kind`](std::io::Error::kind) is preserved.
    pub fn apply_chdir(&self) -> std::io::Result<()> {
        if let Some(ref dir) = self.chdir {
            std::env::set_current_dir(dir).map_err(|e| {
                std::io::Error::new(e.kind(), format!("--chdir {}: {e}", dir.display()))
            })?;
        }
        Ok(())
    }
}

/// Build a clap `Command` with the compact `-h`/`--help` flag (HelpShort).
///
/// Usage: call this on the result of `YourCli::command()` before parsing:
///
/// ```no_run
/// use clap::{CommandFactory, FromArgMatches, Parser};
///
/// #[derive(Parser)]
/// struct MyCli {
///     #[arg(long)]
///     name: Option<String>,
/// }
///
/// # fn main() -> Result<(), clap::Error> {
/// let cmd = librebar::cli::with_help_short(MyCli::command());
/// let cli = MyCli::from_arg_matches(&cmd.get_matches())?;
/// # let _ = cli;
/// # Ok(())
/// # }
/// ```
pub fn with_help_short(cmd: clap::Command) -> clap::Command {
    cmd.arg(
        clap::Arg::new("help")
            .short('h')
            .long("help")
            .help("Print help")
            .global(true)
            .action(clap::ArgAction::HelpShort),
    )
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{ColorChoice, CommonArgs, OutputFormat};

    struct BrokenWriter;

    impl io::Write for BrokenWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed stdout"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed stdout"))
        }
    }

    #[test]
    fn version_only_propagates_stdout_errors() {
        let args = CommonArgs {
            version_only: true,
            chdir: None,
            config: None,
            quiet: false,
            verbose: 0,
            color: ColorChoice::Never,
            format: OutputFormat::Text,
            legacy_json: false,
        };

        let error = args
            .apply_with_writer("1.2.3", BrokenWriter)
            .expect_err("a failed version write should propagate");

        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    }
}
