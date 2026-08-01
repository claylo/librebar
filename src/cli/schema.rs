//! CLI Spec v0.2 generation from Clap's built command model.

use clap::{Arg, ArgAction, Command, CommandFactory, ValueHint};
use serde::Serialize;
use serde_json::Value;
use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// CLI Spec version emitted by librebar.
pub const CLI_SPEC_VERSION: &str = "0.2";

/// A complete CLI Spec v0.2 document.
#[derive(Debug, Clone, Serialize)]
pub struct SchemaDocument {
    /// CLI Spec schema version.
    pub clispec: &'static str,
    /// Binary invocation name.
    pub name: String,
    /// Application version.
    pub version: String,
    /// Application description from Clap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Arguments accepted throughout the command tree.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub global_args: Vec<ArgumentSchema>,
    /// Flattened command paths.
    pub commands: Vec<CommandSchema>,
    /// Declared structured error kinds.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorMetadata>,
    /// Declared non-error exit outcomes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outcomes: Vec<OutcomeMetadata>,
    /// Default unflagged output behavior.
    pub output: OutputBehavior,
}

/// Default output behavior advertised in a schema document.
#[derive(Debug, Clone, Serialize)]
pub struct OutputBehavior {
    /// Format selected on a terminal.
    pub tty: &'static str,
    /// Format selected when stdout is redirected.
    pub piped: &'static str,
}

/// A command entry generated from Clap and enriched by application metadata.
#[derive(Debug, Clone, Serialize)]
pub struct CommandSchema {
    /// Full space-separated command path.
    pub name: String,
    /// Clap command description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the command modifies state; absent means unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutating: Option<bool>,
    /// Stability of the application contract.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stability: Option<Stability>,
    /// Non-global command arguments.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<ArgumentSchema>,
    /// Structured output fields supplied by the application.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub output_fields: Vec<OutputField>,
    /// Self-contained example supplied by the application.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<CommandExample>,
    /// Command aliases accepted by Clap.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Whether Clap hides this command from help.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// Argument groups declared on this command.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub arg_groups: Vec<ArgumentGroupSchema>,
}

/// Invocation metadata for one Clap argument.
#[derive(Debug, Clone, Serialize)]
pub struct ArgumentSchema {
    /// Flag spelling or positional identifier.
    pub name: String,
    /// CLI Spec transport type.
    #[serde(rename = "type")]
    pub value_type: String,
    /// Whether Clap requires the argument.
    pub required: bool,
    /// Default value, preserving scalar types when Clap exposes them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Enumerated accepted values.
    #[serde(rename = "enum", skip_serializing_if = "Vec::is_empty")]
    pub possible_values: Vec<String>,
    /// Clap help text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional accepted flag spellings.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Shell-completion value hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_hint: Option<&'static str>,
    /// Minimum values accepted per occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_values: Option<usize>,
    /// Maximum values accepted per occurrence; absent means unbounded/unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_values: Option<usize>,
    /// Other arguments Clap reports as conflicts.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts_with: Vec<String>,
    /// Whether Clap hides this argument from help.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// A reflected Clap argument group.
#[derive(Debug, Clone, Serialize)]
pub struct ArgumentGroupSchema {
    /// Group identifier.
    pub name: String,
    /// Whether at least one member is required.
    pub required: bool,
    /// Argument spellings belonging to the group.
    pub args: Vec<String>,
}

/// Stability declared for an application command contract.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    /// Stable public contract.
    Stable,
    /// Public beta contract.
    Beta,
    /// Experimental contract.
    Experimental,
    /// Deprecated contract retained for compatibility.
    Deprecated,
}

/// One field in a command's structured output.
#[derive(Debug, Clone, Serialize)]
pub struct OutputField {
    /// Stable field name.
    pub name: String,
    /// CLI Spec type description.
    #[serde(rename = "type")]
    pub value_type: String,
    /// Human-readable field description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl OutputField {
    /// Create an output field.
    pub fn new(name: impl Into<String>, value_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value_type: value_type.into(),
            description: None,
        }
    }

    /// Add a field description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A self-contained command invocation used by compliance tooling.
#[derive(Debug, Clone, Serialize)]
pub struct CommandExample {
    /// Arguments after the command path.
    pub args: Vec<String>,
    /// Optional standard input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
}

impl CommandExample {
    /// Create an example from command arguments.
    pub fn new<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            args: args.into_iter().map(Into::into).collect(),
            stdin: None,
        }
    }

    /// Add standard input to the example.
    #[must_use]
    pub fn stdin(mut self, input: impl Into<String>) -> Self {
        self.stdin = Some(input.into());
        self
    }
}

/// Structured error kind supplied by the application.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorMetadata {
    /// Stable machine-readable error kind.
    pub kind: String,
    /// Process exit code for this error kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u8>,
    /// Whether retrying can succeed without changing the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ErrorMetadata {
    /// Create an error declaration.
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            exit_code: None,
            retryable: None,
            description: None,
        }
    }

    /// Set the error exit code.
    #[must_use]
    pub const fn exit_code(mut self, code: u8) -> Self {
        self.exit_code = Some(code);
        self
    }

    /// Set whether the error is retryable.
    #[must_use]
    pub const fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = Some(retryable);
        self
    }

    /// Add an error description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A documented non-zero exit that represents data rather than failure.
#[derive(Debug, Clone, Serialize)]
pub struct OutcomeMetadata {
    /// Process exit code.
    pub code: u8,
    /// Stable machine-readable outcome name.
    pub name: String,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl OutcomeMetadata {
    /// Create an outcome declaration.
    pub fn new(code: u8, name: impl Into<String>) -> Self {
        Self {
            code,
            name: name.into(),
            description: None,
        }
    }

    /// Add an outcome description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Application-owned facts that Clap cannot derive safely.
#[derive(Debug, Clone, Default)]
pub struct SchemaMetadata {
    version: Option<String>,
    commands: BTreeMap<String, CommandMetadata>,
    errors: Vec<ErrorMetadata>,
    outcomes: Vec<OutcomeMetadata>,
}

impl SchemaMetadata {
    /// Create empty metadata. Mutation and output contracts remain unknown.
    pub fn new() -> Self {
        Self::default()
    }

    /// Supply the application version when the Clap command does not declare it.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Attach facts to an exact, space-separated command path.
    #[must_use]
    pub fn command(mut self, path: impl Into<String>, metadata: CommandMetadata) -> Self {
        self.commands.insert(path.into(), metadata);
        self
    }

    /// Declare a structured error kind.
    #[must_use]
    pub fn error(mut self, error: ErrorMetadata) -> Self {
        self.errors.push(error);
        self
    }

    /// Declare a non-error process outcome.
    #[must_use]
    pub fn outcome(mut self, outcome: OutcomeMetadata) -> Self {
        self.outcomes.push(outcome);
        self
    }
}

/// Application-owned facts for one exact command path.
#[derive(Debug, Clone, Default)]
pub struct CommandMetadata {
    mutating: Option<bool>,
    stability: Option<Stability>,
    output_fields: Vec<OutputField>,
    example: Option<CommandExample>,
}

impl CommandMetadata {
    /// Create empty command metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare whether the command modifies state.
    #[must_use]
    pub const fn mutating(mut self, mutating: bool) -> Self {
        self.mutating = Some(mutating);
        self
    }

    /// Declare the command contract's stability.
    #[must_use]
    pub const fn stability(mut self, stability: Stability) -> Self {
        self.stability = Some(stability);
        self
    }

    /// Add a structured output field.
    #[must_use]
    pub fn output_field(mut self, field: OutputField) -> Self {
        self.output_fields.push(field);
        self
    }

    /// Add a self-contained example invocation.
    #[must_use]
    pub fn example(mut self, example: CommandExample) -> Self {
        self.example = Some(example);
        self
    }
}

/// Schema construction failure caused by an incomplete or inconsistent contract.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// Neither Clap nor application metadata supplied a version.
    #[error(
        "the CLI schema needs an application version; add #[command(version)] or SchemaMetadata::version"
    )]
    MissingVersion,
    /// Metadata named a path not present in the Clap tree.
    #[error("schema metadata names unknown command path `{0}`")]
    UnknownCommand(String),
    /// An error and outcome use the same exit code.
    #[error("exit code {0} is declared as both an error and an outcome")]
    OverlappingExitCode(u8),
    /// A requested schema filter is not a command subtree.
    #[error("unknown schema command path `{0}`")]
    UnknownFilter(String),
    /// The selected application version is empty.
    #[error("the CLI schema application version cannot be empty")]
    EmptyVersion,
    /// A CLI Spec snake-case identifier is invalid.
    #[error("invalid {field} `{value}`; expected lower snake_case")]
    InvalidIdentifier {
        /// Metadata field being validated.
        field: &'static str,
        /// Invalid value.
        value: String,
    },
    /// CLI Spec reserves zero for successful process exits.
    #[error("exit code {0} is invalid in CLI schema metadata; expected 1..=255")]
    InvalidExitCode(u8),
    /// Structured output field names must be nonempty.
    #[error("command `{0}` contains an empty output field name")]
    EmptyOutputField(String),
}

/// Generate a CLI Spec document from a consumer's Clap command tree.
///
/// # Errors
///
/// Returns an error when the command has no version, metadata names an unknown
/// path, or error and outcome exit codes overlap.
pub fn schema_for<T: CommandFactory>(
    metadata: &SchemaMetadata,
) -> Result<SchemaDocument, SchemaError> {
    schema_from_command(T::command(), metadata, &[])
}

pub(super) fn schema_from_command(
    mut root: Command,
    metadata: &SchemaMetadata,
    filter: &[String],
) -> Result<SchemaDocument, SchemaError> {
    root.build();
    let name = root.get_name().to_owned();
    let version = metadata
        .version
        .clone()
        .or_else(|| root.get_version().map(str::to_owned))
        .ok_or(SchemaError::MissingVersion)?;
    if version.trim().is_empty() {
        return Err(SchemaError::EmptyVersion);
    }
    let description = root.get_about().map(ToString::to_string);

    let global_args = root
        .get_arguments()
        .filter(|arg| arg.is_global_set())
        .map(|arg| argument_schema(&root, arg))
        .collect();

    let mut commands = Vec::new();
    walk_commands(&root, "", metadata, &mut commands);
    validate_metadata(metadata, &commands)?;
    filter_commands(&mut commands, filter)?;

    Ok(SchemaDocument {
        clispec: CLI_SPEC_VERSION,
        name,
        version,
        description,
        global_args,
        commands,
        errors: metadata.errors.clone(),
        outcomes: metadata.outcomes.clone(),
        output: OutputBehavior {
            tty: "text",
            piped: "json",
        },
    })
}

fn walk_commands(
    command: &Command,
    parent: &str,
    metadata: &SchemaMetadata,
    output: &mut Vec<CommandSchema>,
) {
    for subcommand in command
        .get_subcommands()
        .filter(|subcommand| subcommand.get_name() != "help")
    {
        let path = if parent.is_empty() {
            subcommand.get_name().to_owned()
        } else {
            format!("{parent} {}", subcommand.get_name())
        };
        let supplied = metadata.commands.get(&path);
        let args = subcommand
            .get_arguments()
            .filter(|arg| !arg.is_global_set())
            .map(|arg| argument_schema(subcommand, arg))
            .collect();
        let aliases = subcommand.get_all_aliases().map(str::to_owned).collect();
        let arg_groups = group_schemas(subcommand);

        output.push(CommandSchema {
            name: path.clone(),
            description: subcommand.get_about().map(ToString::to_string),
            mutating: supplied.and_then(|value| value.mutating),
            stability: supplied.and_then(|value| value.stability),
            args,
            output_fields: supplied
                .map(|value| value.output_fields.clone())
                .unwrap_or_default(),
            example: supplied.and_then(|value| value.example.clone()),
            aliases,
            hidden: subcommand.is_hide_set(),
            arg_groups,
        });
        walk_commands(subcommand, &path, metadata, output);
    }
}

fn validate_metadata(
    metadata: &SchemaMetadata,
    commands: &[CommandSchema],
) -> Result<(), SchemaError> {
    for error in &metadata.errors {
        validate_identifier("error kind", &error.kind)?;
        if error.exit_code == Some(0) {
            return Err(SchemaError::InvalidExitCode(0));
        }
    }
    for outcome in &metadata.outcomes {
        validate_identifier("outcome name", &outcome.name)?;
        if outcome.code == 0 {
            return Err(SchemaError::InvalidExitCode(0));
        }
    }
    if let Some((path, _)) = metadata.commands.iter().find(|(_, command)| {
        command
            .output_fields
            .iter()
            .any(|field| field.name.is_empty())
    }) {
        return Err(SchemaError::EmptyOutputField(path.clone()));
    }

    let paths: BTreeSet<&str> = commands
        .iter()
        .map(|command| command.name.as_str())
        .collect();
    if let Some(unknown) = metadata
        .commands
        .keys()
        .find(|path| !paths.contains(path.as_str()))
    {
        return Err(SchemaError::UnknownCommand(unknown.clone()));
    }

    let error_codes: BTreeSet<u8> = metadata
        .errors
        .iter()
        .filter_map(|error| error.exit_code)
        .collect();
    if let Some(code) = metadata
        .outcomes
        .iter()
        .map(|outcome| outcome.code)
        .find(|code| error_codes.contains(code))
    {
        return Err(SchemaError::OverlappingExitCode(code));
    }

    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), SchemaError> {
    let mut characters = value.chars();
    let valid = characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        });
    if valid {
        Ok(())
    } else {
        Err(SchemaError::InvalidIdentifier {
            field,
            value: value.to_owned(),
        })
    }
}

fn filter_commands(
    commands: &mut Vec<CommandSchema>,
    filter: &[String],
) -> Result<(), SchemaError> {
    if filter.is_empty() {
        return Ok(());
    }
    let prefix = filter.join(" ");
    let subtree = format!("{prefix} ");
    if !commands
        .iter()
        .any(|command| command.name == prefix || command.name.starts_with(&subtree))
    {
        return Err(SchemaError::UnknownFilter(prefix));
    }
    commands.retain(|command| command.name == prefix || command.name.starts_with(&subtree));
    Ok(())
}

fn argument_schema(command: &Command, arg: &Arg) -> ArgumentSchema {
    let base_type = argument_base_type(arg);
    let is_array = matches!(arg.get_action(), ArgAction::Append)
        || arg
            .get_num_args()
            .is_some_and(|range| range.max_values() > 1);
    let value_type = if is_array && base_type != "boolean" {
        format!("{base_type}[]")
    } else {
        base_type.to_owned()
    };
    let (min_values, max_values) = arg.get_num_args().map_or((None, None), |range| {
        let max = (range.max_values() != usize::MAX).then_some(range.max_values());
        (Some(range.min_values()), max)
    });
    let conflicts_with = command
        .get_arg_conflicts_with(arg)
        .into_iter()
        .map(argument_name)
        .collect();

    ArgumentSchema {
        name: argument_name(arg),
        value_type,
        required: arg.is_required_set(),
        default: argument_default(arg, base_type, is_array),
        possible_values: arg
            .get_possible_values()
            .into_iter()
            .map(|value| value.get_name().to_owned())
            .collect(),
        description: arg.get_help().map(ToString::to_string),
        aliases: argument_aliases(arg),
        value_hint: value_hint_name(arg.get_value_hint()),
        min_values,
        max_values,
        conflicts_with,
        hidden: arg.is_hide_set(),
    }
}

fn argument_name(arg: &Arg) -> String {
    arg.get_long().map_or_else(
        || {
            arg.get_short().map_or_else(
                || arg.get_id().as_str().to_owned(),
                |short| format!("-{short}"),
            )
        },
        |long| format!("--{long}"),
    )
}

fn argument_aliases(arg: &Arg) -> Vec<String> {
    let mut aliases = Vec::new();
    if let Some(values) = arg.get_all_aliases() {
        aliases.extend(values.into_iter().map(|value| format!("--{value}")));
    }
    if let Some(values) = arg.get_all_short_aliases() {
        aliases.extend(values.into_iter().map(|value| format!("-{value}")));
    }
    aliases
}

fn argument_base_type(arg: &Arg) -> &'static str {
    match arg.get_action() {
        ArgAction::SetTrue
        | ArgAction::SetFalse
        | ArgAction::Help
        | ArgAction::HelpShort
        | ArgAction::HelpLong
        | ArgAction::Version => return "boolean",
        ArgAction::Count => return "integer",
        _ => {}
    }

    let id = arg.get_value_parser().type_id();
    if id == TypeId::of::<PathBuf>() {
        "path"
    } else if id == TypeId::of::<u8>()
        || id == TypeId::of::<u16>()
        || id == TypeId::of::<u32>()
        || id == TypeId::of::<u64>()
        || id == TypeId::of::<usize>()
        || id == TypeId::of::<i8>()
        || id == TypeId::of::<i16>()
        || id == TypeId::of::<i32>()
        || id == TypeId::of::<i64>()
        || id == TypeId::of::<isize>()
    {
        "integer"
    } else if id == TypeId::of::<f32>() || id == TypeId::of::<f64>() {
        "number"
    } else if id == TypeId::of::<bool>() {
        "boolean"
    } else {
        "string"
    }
}

fn argument_default(arg: &Arg, value_type: &str, is_array: bool) -> Option<Value> {
    let values: Vec<_> = arg
        .get_default_values()
        .iter()
        .map(|value| scalar_value(value.to_string_lossy().as_ref(), value_type))
        .collect();
    match values.as_slice() {
        [] => None,
        [value] if !is_array => Some(value.clone()),
        _ => Some(Value::Array(values)),
    }
}

fn scalar_value(value: &str, value_type: &str) -> Value {
    match value_type {
        "boolean" => value
            .parse::<bool>()
            .map_or_else(|_| Value::String(value.to_owned()), Value::Bool),
        "integer" => value
            .parse::<i64>()
            .map(Value::from)
            .or_else(|_| value.parse::<u64>().map(Value::from))
            .unwrap_or_else(|_| Value::String(value.to_owned())),
        "number" => value
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(value.to_owned())),
        _ => Value::String(value.to_owned()),
    }
}

fn group_schemas(command: &Command) -> Vec<ArgumentGroupSchema> {
    command
        .get_groups()
        .filter_map(|group| {
            let mut seen = BTreeSet::new();
            let args: Vec<_> = group
                .get_args()
                .filter_map(|id| {
                    command
                        .get_arguments()
                        .find(|arg| arg.get_id() == id)
                        .map(argument_name)
                })
                .filter(|name| seen.insert(name.clone()))
                .collect();
            (!args.is_empty()).then(|| ArgumentGroupSchema {
                name: group.get_id().as_str().to_owned(),
                required: group.is_required_set(),
                args,
            })
        })
        .collect()
}

const fn value_hint_name(hint: ValueHint) -> Option<&'static str> {
    match hint {
        ValueHint::Unknown => None,
        ValueHint::Other => Some("other"),
        ValueHint::AnyPath => Some("any-path"),
        ValueHint::FilePath => Some("file-path"),
        ValueHint::DirPath => Some("dir-path"),
        ValueHint::ExecutablePath => Some("executable-path"),
        ValueHint::CommandName => Some("command-name"),
        ValueHint::CommandString => Some("command-string"),
        ValueHint::CommandWithArguments => Some("command-with-arguments"),
        ValueHint::Username => Some("username"),
        ValueHint::Hostname => Some("hostname"),
        ValueHint::Url => Some("url"),
        ValueHint::EmailAddress => Some("email-address"),
        _ => Some("other"),
    }
}
