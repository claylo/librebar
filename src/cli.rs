//! CLI argument types shared across librebar-based applications.
//!
//! Provides [`CommonArgs`] for standard flags (quiet, verbose, json, color, chdir)
//! and [`ColorChoice`] for terminal color configuration. Consumers embed these
//! into their own clap-derived structs via `#[command(flatten)]`.

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
#[derive(Parser, Debug)]
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

impl CommonArgs {
    /// Apply color settings globally. Call once at startup.
    pub fn apply_color(&self) {
        self.color.apply();
    }

    /// Change the working directory if `--chdir` was specified.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory does not exist or is not accessible.
    pub fn apply_chdir(&self) -> std::io::Result<()> {
        if let Some(ref dir) = self.chdir {
            std::env::set_current_dir(dir)?;
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
