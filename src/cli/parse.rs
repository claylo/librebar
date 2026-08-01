//! Pre-startup parsing and librebar-owned terminal commands.

use super::schema::schema_from_command;
use super::{CommandMetadata, SchemaDocument, SchemaMetadata, Stability};
use clap::error::ErrorKind;
use clap::{Arg, ArgAction, Command, CommandFactory, Parser};
use std::ffi::OsString;
use std::io::Write;

const SCHEMA_COMMAND: &str = "schema";
const SCHEMA_PATH_ARG: &str = "command_path";
const COMPLETIONS_COMMAND: &str = "completions";
const COMPLETIONS_SHELL_ARG: &str = "shell";

/// Result of the non-exiting parse path.
#[derive(Debug)]
#[non_exhaustive]
pub enum ParseOutcome<T> {
    /// Arguments belong to the application; continue normal startup.
    Run(T),
    /// Librebar handled `schema` before application startup.
    Schema(SchemaDocument),
    /// Librebar generated a shell completion script before application startup.
    Completions(Vec<u8>),
}

/// Build the application's Clap command with librebar terminal commands.
///
/// # Errors
///
/// Returns an argument conflict when the application already defines a
/// librebar-owned command name.
pub fn command<T: CommandFactory>() -> Result<Command, clap::Error> {
    let mut root = T::command();
    for reserved in [SCHEMA_COMMAND, COMPLETIONS_COMMAND] {
        if root
            .get_subcommands()
            .any(|command| command.get_name() == reserved)
        {
            return Err(root.error(
                ErrorKind::ArgumentConflict,
                format!("the `{reserved}` subcommand is reserved by librebar"),
            ));
        }
    }

    Ok(root
        .subcommand(schema_command())
        .subcommand(completions_command()))
}

/// Parse arguments without exiting the process.
///
/// This is the testable core behind [`parse`] and [`parse_with`]. Terminal
/// commands are returned as variants instead of being printed.
///
/// # Errors
///
/// Returns Clap parse errors, command-name collisions, invalid schema filters,
/// or invalid application schema metadata as a Clap error.
pub fn try_parse_from<T, I, S>(
    args: I,
    metadata: SchemaMetadata,
) -> Result<ParseOutcome<T>, clap::Error>
where
    T: Parser,
    I: IntoIterator<Item = S>,
    S: Into<OsString> + Clone,
{
    let mut root = command::<T>()?;
    let mut matches = root.try_get_matches_from_mut(args)?;

    if let Some(schema_matches) = matches.subcommand_matches(SCHEMA_COMMAND) {
        let filter: Vec<String> = schema_matches
            .get_many::<String>(SCHEMA_PATH_ARG)
            .map(|values| values.cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let metadata = metadata
            .command(
                SCHEMA_COMMAND,
                CommandMetadata::new()
                    .mutating(false)
                    .stability(Stability::Stable),
            )
            .command(
                COMPLETIONS_COMMAND,
                CommandMetadata::new()
                    .mutating(false)
                    .stability(Stability::Stable),
            );
        let document = schema_from_command(root.clone(), &metadata, &filter)
            .map_err(|error| root.error(ErrorKind::InvalidValue, error.to_string()))?;
        return Ok(ParseOutcome::Schema(document));
    }

    if let Some(completion_matches) = matches.subcommand_matches(COMPLETIONS_COMMAND) {
        let shell = *completion_matches
            .get_one::<clap_complete::Shell>(COMPLETIONS_SHELL_ARG)
            .expect("Clap requires the completion shell");
        let bin_name = root.get_name().to_owned();
        let mut bytes = Vec::new();
        clap_complete::generate(shell, &mut root, bin_name, &mut bytes);
        return Ok(ParseOutcome::Completions(bytes));
    }

    T::from_arg_matches_mut(&mut matches).map(ParseOutcome::Run)
}

/// Parse process arguments, handling librebar terminal commands before startup.
///
/// Like [`clap::Parser::parse`], this function exits for help, version, parse
/// errors, and terminal commands. Normal application arguments are returned.
pub fn parse<T: Parser>() -> T {
    parse_with(SchemaMetadata::new())
}

/// Parse process arguments with explicit application schema metadata.
///
/// Like [`parse`], this exits after printing a schema or parse error.
pub fn parse_with<T: Parser>(metadata: SchemaMetadata) -> T {
    match try_parse_from::<T, _, _>(std::env::args_os(), metadata) {
        Ok(ParseOutcome::Run(cli)) => cli,
        Ok(ParseOutcome::Schema(document)) => write_schema_and_exit(&document),
        Ok(ParseOutcome::Completions(bytes)) => write_bytes_and_exit(&bytes, "shell completions"),
        Err(error) => error.exit(),
    }
}

fn schema_command() -> Command {
    Command::new(SCHEMA_COMMAND)
        .about("Print the machine-readable CLI Spec schema")
        .arg(
            Arg::new(SCHEMA_PATH_ARG)
                .help("Optional command path to narrow the schema")
                .num_args(0..)
                .action(ArgAction::Append)
                .value_name("COMMAND_PATH")
                .value_hint(clap::ValueHint::CommandName),
        )
}

fn completions_command() -> Command {
    Command::new(COMPLETIONS_COMMAND)
        .about("Generate a shell completion script")
        .arg(
            Arg::new(COMPLETIONS_SHELL_ARG)
                .help("Shell to generate completions for")
                .required(true)
                .value_parser(clap::builder::EnumValueParser::<clap_complete::Shell>::new())
                .value_name("SHELL"),
        )
}

fn write_schema_and_exit(document: &SchemaDocument) -> ! {
    let result = (|| -> Result<(), std::io::Error> {
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        serde_json::to_writer_pretty(&mut output, document).map_err(std::io::Error::other)?;
        writeln!(output)
    })();

    match result {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("failed to write CLI schema: {error}");
            std::process::exit(1);
        }
    }
}

fn write_bytes_and_exit(bytes: &[u8], label: &str) -> ! {
    let stdout = std::io::stdout();
    let result = stdout.lock().write_all(bytes);
    match result {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("failed to write {label}: {error}");
            std::process::exit(1);
        }
    }
}
