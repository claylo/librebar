//! CLI argument types shared across librebar-based applications.
//!
//! Provides [`CommonArgs`] for standard flags (quiet, verbose, json, color, chdir)
//! and [`ColorChoice`] for terminal color configuration. Consumers embed these
//! into their own clap-derived structs via `#[command(flatten)]`.
//!
//! After parsing, call [`CommonArgs::apply`] once. It performs every startup
//! side effect these flags imply, in the order they have to happen in, and
//! reports whether the process should keep going.

use clap::Parser;
use std::path::PathBuf;

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
/// `--version-only` is deliberately *not* global: it is a top-level query, so
/// `myapp --version-only` works and `myapp sub --version-only` does not.
///
/// The rustdoc above is for readers of this crate, not for users of yours.
/// `#[command(about = None, long_about = None)]` keeps clap from adopting it:
/// a flattened struct's doc comment otherwise becomes the *consuming* binary's
/// help description, so `myapp --help` would open by explaining librebar.
#[derive(Parser, Debug)]
#[command(about = None, long_about = None)]
pub struct CommonArgs {
    /// Print only the version number (for scripting).
    #[arg(long)]
    pub version_only: bool,

    /// Run as if started in DIR.
    #[arg(short = 'C', long, global = true)]
    pub chdir: Option<PathBuf>,

    /// Only print errors (suppresses warnings/info).
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// More detail (repeatable; e.g. -vv).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Colorize output.
    #[arg(long, global = true, value_enum, default_value_t)]
    pub color: ColorChoice,

    /// Output as JSON (for scripting).
    #[arg(long, global = true)]
    pub json: bool,
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
    /// Returns an error if `--chdir` names a directory that does not exist or
    /// is not accessible.
    pub fn apply(&self, version: &str) -> std::io::Result<Startup> {
        // Colors first, so anything printed afterwards is styled correctly.
        self.apply_color();

        // Answered before `apply_chdir` so that `--version-only` cannot fail
        // on an unrelated bad `-C`.
        if self.version_only {
            println!("{version}");
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
