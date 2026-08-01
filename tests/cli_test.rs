#![allow(missing_docs)]
#![cfg(feature = "cli")]

use librebar::cli::clap;
use librebar::cli::clap::Parser;

/// Test harness that embeds librebar's CommonArgs the way a consumer would.
#[derive(Parser, Debug)]
#[command(name = "test-app")]
struct TestCli {
    #[command(flatten)]
    pub common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    pub command: Option<TestCommands>,
}

#[derive(clap::Subcommand, Debug)]
enum TestCommands {
    Info,
}

#[derive(Parser, Debug)]
#[command(
    name = "schema-app",
    version = "1.2.3",
    about = "Exercise schema reflection"
)]
struct SchemaCli {
    #[command(flatten)]
    common: librebar::cli::CommonArgs,

    /// Select a project file.
    #[arg(long, global = true, value_hint = clap::ValueHint::FilePath)]
    project: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: SchemaCommands,
}

#[derive(clap::Subcommand, Debug)]
enum SchemaCommands {
    /// Work with widgets.
    Widget {
        #[command(subcommand)]
        command: WidgetCommands,
    },
    /// Show current status.
    Status,
}

#[derive(clap::Subcommand, Debug)]
enum WidgetCommands {
    /// List widgets.
    #[command(group(
        clap::ArgGroup::new("scope")
            .required(true)
            .args(["owner", "all"])
    ))]
    List {
        /// Widget owner.
        #[arg(long, group = "scope")]
        owner: Option<String>,

        /// Include every owner.
        #[arg(long, group = "scope", conflicts_with = "limit")]
        all: bool,

        /// Maximum number of widgets.
        #[arg(long, visible_alias = "max", default_value_t = 100)]
        limit: u16,

        /// Statuses to include.
        #[arg(long, value_enum)]
        status: Vec<WidgetStatus>,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum WidgetStatus {
    Ready,
    Busy,
}

#[derive(Parser, Debug)]
#[command(name = "collision-app", version = "1.0.0")]
struct SchemaCollisionCli {
    #[command(subcommand)]
    command: SchemaCollisionCommands,
}

#[derive(clap::Subcommand, Debug)]
enum SchemaCollisionCommands {
    Schema,
}

#[derive(Parser, Debug)]
#[command(name = "completion-collision-app", version = "1.0.0")]
struct CompletionCollisionCli {
    #[command(subcommand)]
    command: CompletionCollisionCommands,
}

#[derive(clap::Subcommand, Debug)]
enum CompletionCollisionCommands {
    Completions,
}

const fn assert_args<T: clap::Args>() {}

#[test]
fn common_args_is_cloneable_flattened_args() {
    assert_args::<librebar::cli::CommonArgs>();
    let cli = TestCli::parse_from(["test-app", "info"]);
    let copy = cli.common.clone();
    assert_eq!(copy.verbose, cli.common.verbose);
}

#[test]
fn common_args_defaults() {
    let cli = TestCli::parse_from(["test-app", "info"]);
    assert!(!cli.common.quiet);
    assert_eq!(cli.common.verbose, 0);
    assert_eq!(cli.common.format, librebar::cli::OutputFormat::Auto);
    assert!(!cli.common.version_only);
    assert!(cli.common.chdir.is_none());
}

#[test]
fn common_args_quiet_flag() {
    let cli = TestCli::parse_from(["test-app", "--quiet", "info"]);
    assert!(cli.common.quiet);
}

#[test]
fn common_args_verbose_stacks() {
    let cli = TestCli::parse_from(["test-app", "-vv", "info"]);
    assert_eq!(cli.common.verbose, 2);
}

#[test]
fn output_format_parses_explicit_values() {
    let text = TestCli::parse_from(["test-app", "--format", "text", "info"]);
    let json = TestCli::parse_from(["test-app", "--format", "json", "info"]);

    assert_eq!(text.common.format, librebar::cli::OutputFormat::Text);
    assert_eq!(json.common.format, librebar::cli::OutputFormat::Json);
}

#[test]
fn output_format_auto_resolves_from_terminal_state() {
    let cli = TestCli::parse_from(["test-app", "info"]);

    assert_eq!(
        cli.common.output_format_for(true),
        librebar::cli::ResolvedOutputFormat::Text
    );
    assert_eq!(
        cli.common.output_format_for(false),
        librebar::cli::ResolvedOutputFormat::Json
    );
}

#[test]
fn output_json_compatibility_flag_selects_json() {
    let cli = TestCli::parse_from(["test-app", "--json", "info"]);

    assert_eq!(
        cli.common.output_format_for(true),
        librebar::cli::ResolvedOutputFormat::Json
    );
}

#[test]
fn output_json_compatibility_flag_is_hidden() {
    use clap::CommandFactory;

    let help = TestCli::command().render_long_help().to_string();
    assert!(
        !help.contains("--json"),
        "legacy flag leaked into help: {help}"
    );
    assert!(help.contains("--format"), "typed selector missing: {help}");
}

#[test]
fn output_json_conflicts_with_explicit_format() {
    let error = TestCli::try_parse_from(["test-app", "--format", "text", "--json", "info"])
        .expect_err("two explicit output selectors must be rejected");

    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn common_args_chdir() {
    let cli = TestCli::parse_from(["test-app", "-C", "/tmp", "info"]);
    assert_eq!(
        cli.common.chdir.as_deref(),
        Some(std::path::Path::new("/tmp"))
    );
}

#[test]
fn every_common_flag_propagates_to_subcommands() {
    // All of CommonArgs is `global`. A flag that works at the top level but
    // errors after a subcommand name is the kind of inconsistency a user only
    // discovers by tripping over it.
    let cli = TestCli::parse_from([
        "test-app",
        "info",
        "--version-only",
        "--quiet",
        "-vv",
        "--format",
        "json",
        "--color",
        "never",
        "-C",
        "/tmp",
    ]);

    assert!(cli.common.version_only);
    assert!(cli.common.quiet);
    assert_eq!(cli.common.verbose, 2);
    assert_eq!(cli.common.format, librebar::cli::OutputFormat::Json);
    assert!(matches!(
        cli.common.color,
        librebar::cli::ColorChoice::Never
    ));
    assert_eq!(
        cli.common.chdir.as_deref(),
        Some(std::path::Path::new("/tmp"))
    );
}

// ─── CommonArgs::apply ──────────────────────────────────────────────
//
// These deliberately avoid exercising a *successful* `--chdir`: that mutates
// process-global state and would race the rest of the suite. The failure path
// leaves the cwd untouched, so it is safe to run in parallel.

#[test]
fn apply_continues_when_no_terminal_flag_is_set() {
    let cli = TestCli::parse_from(["test-app", "info"]);
    let startup = cli.common.apply("1.2.3").expect("apply should succeed");

    assert_eq!(startup, librebar::cli::Startup::Continue);
    assert!(startup.is_continue());
    assert!(!startup.is_exit());
}

#[test]
fn apply_exits_on_version_only() {
    let cli = TestCli::parse_from(["test-app", "--version-only"]);
    let startup = cli.common.apply("1.2.3").expect("apply should succeed");

    assert_eq!(startup, librebar::cli::Startup::Exit);
    assert!(startup.is_exit());
}

#[test]
fn apply_reports_a_bad_chdir() {
    let cli = TestCli::parse_from([
        "test-app",
        "-C",
        "/librebar-nonexistent-directory-for-tests",
        "info",
    ]);

    let err = cli
        .common
        .apply("1.2.3")
        .expect_err("a nonexistent --chdir target should error");

    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    // The bare OS message names neither the flag nor the path, which leaves
    // the user nothing to act on.
    let msg = err.to_string();
    assert!(
        msg.contains("--chdir"),
        "message should name the flag: {msg}"
    );
    assert!(
        msg.contains("/librebar-nonexistent-directory-for-tests"),
        "message should name the path: {msg}"
    );
}

#[test]
fn version_only_is_answered_before_chdir() {
    // `--version-only` is a scripting query and must not fail because of an
    // unrelated bad `-C`. This pins the ordering inside `apply`.
    let cli = TestCli::parse_from([
        "test-app",
        "--version-only",
        "-C",
        "/librebar-nonexistent-directory-for-tests",
    ]);

    let startup = cli
        .common
        .apply("1.2.3")
        .expect("--version-only should not touch the filesystem");
    assert_eq!(startup, librebar::cli::Startup::Exit);
}

#[test]
fn color_choice_default_is_auto() {
    let cli = TestCli::parse_from(["test-app", "info"]);
    assert!(matches!(cli.common.color, librebar::cli::ColorChoice::Auto));
}

#[test]
fn color_choice_never() {
    let cli = TestCli::parse_from(["test-app", "--color", "never", "info"]);
    assert!(matches!(
        cli.common.color,
        librebar::cli::ColorChoice::Never
    ));
}

// NOTE: these harnesses deliberately use `//` rather than `///`. A doc comment
// here would itself become the command's about/long_about and mask the very
// leak these tests pin.

// A consumer that describes itself with `about` but never sets `long_about` —
// the overwhelmingly common shape, and the one where a leak hides in `--help`
// while `-h` looks fine.
#[derive(Parser, Debug)]
#[command(name = "about-app", about = "Do the thing")]
struct AboutCli {
    #[command(flatten)]
    #[allow(dead_code)]
    pub common: librebar::cli::CommonArgs,
}

// A consumer that describes itself not at all.
#[derive(Parser, Debug)]
#[command(name = "bare-app")]
struct BareCli {
    #[command(flatten)]
    #[allow(dead_code)]
    pub common: librebar::cli::CommonArgs,
}

#[test]
fn flattening_common_args_does_not_describe_the_consumer() {
    use clap::CommandFactory;

    // `Args::augment_args` applies a flattened struct's doc comment as the
    // parent command's `about`/`long_about`. `CommonArgs` has rustdoc aimed at
    // library readers, so without suppression every consumer's help text
    // describes librebar instead of the consumer.
    let cmd = BareCli::command();
    assert_eq!(
        cmd.get_about(),
        None,
        "flattening CommonArgs must not supply the consumer's short description"
    );
    assert_eq!(
        cmd.get_long_about(),
        None,
        "flattening CommonArgs must not supply the consumer's long description"
    );
}

#[test]
fn a_consumer_that_sets_about_keeps_it_in_long_help() {
    use clap::CommandFactory;

    // The regression: `-h` rendered the consumer's `about` because the derive
    // applies the parent's own attributes last, but `--help` fell through to
    // `long_about`, which only the flattened struct had set.
    let rendered = AboutCli::command().render_long_help().to_string();

    assert!(
        rendered.contains("Do the thing"),
        "--help should describe the consumer: {rendered}"
    );
    assert!(
        !rendered.contains("librebar-based applications"),
        "--help must not leak librebar's rustdoc: {rendered}"
    );
    assert!(
        !rendered.contains("command(flatten)"),
        "--help must not leak the rustdoc example: {rendered}"
    );
}

// ─── CLI Spec reflection ────────────────────────────────────────────

fn generated_schema() -> serde_json::Value {
    let document = librebar::cli::schema_for::<SchemaCli>(&librebar::cli::SchemaMetadata::new())
        .expect("fixture command should produce a schema");
    serde_json::to_value(document).expect("schema should serialize")
}

#[test]
fn schema_reflects_root_and_nested_command_metadata() {
    let schema = generated_schema();

    assert_eq!(schema["clispec"], "0.2");
    assert_eq!(schema["name"], "schema-app");
    assert_eq!(schema["version"], "1.2.3");
    assert_eq!(schema["description"], "Exercise schema reflection");
    assert_eq!(schema["output"]["tty"], "text");
    assert_eq!(schema["output"]["piped"], "json");

    let commands = schema["commands"].as_array().unwrap();
    assert!(commands.iter().any(|command| command["name"] == "widget"));
    assert!(
        commands
            .iter()
            .any(|command| command["name"] == "widget list")
    );
    assert!(commands.iter().any(|command| command["name"] == "status"));
}

#[test]
fn schema_separates_global_args_and_reflects_argument_details() {
    let schema = generated_schema();
    let globals = schema["global_args"].as_array().unwrap();
    let project = globals
        .iter()
        .find(|arg| arg["name"] == "--project")
        .expect("project should be global");
    assert_eq!(project["type"], "path");
    assert_eq!(project["value_hint"], "file-path");

    let list = schema["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "widget list")
        .unwrap();
    let args = list["args"].as_array().unwrap();

    let all = args.iter().find(|arg| arg["name"] == "--all").unwrap();
    assert_eq!(all["type"], "boolean");
    assert_eq!(all["conflicts_with"], serde_json::json!(["--limit"]));

    let limit = args.iter().find(|arg| arg["name"] == "--limit").unwrap();
    assert_eq!(limit["type"], "integer");
    assert_eq!(limit["default"], 100);
    assert_eq!(limit["aliases"], serde_json::json!(["--max"]));

    let status = args.iter().find(|arg| arg["name"] == "--status").unwrap();
    assert_eq!(status["type"], "string[]");
    assert_eq!(status["enum"], serde_json::json!(["ready", "busy"]));

    let scope = list["arg_groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["name"] == "scope")
        .expect("explicit scope group should be reflected");
    assert_eq!(
        scope,
        &serde_json::json!({
            "name": "scope",
            "required": true,
            "args": ["--owner", "--all"]
        })
    );
}

#[test]
fn schema_reflects_clap_terminal_actions_as_booleans() {
    let schema = generated_schema();
    let help_args: Vec<_> = schema["commands"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|command| command["args"].as_array().into_iter().flatten())
        .filter(|arg| arg["name"] == "--help")
        .collect();

    assert!(!help_args.is_empty());
    assert!(help_args.iter().all(|arg| arg["type"] == "boolean"));
}

#[test]
fn schema_omits_clap_generated_help_command_tree() {
    let schema = generated_schema();
    let names: Vec<_> = schema["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|command| command["name"].as_str().unwrap())
        .collect();

    assert!(
        names.iter().all(|name| {
            *name != "help" && !name.starts_with("help ") && !name.contains(" help")
        }),
        "generated Clap help commands leaked into schema: {names:?}"
    );
}

#[test]
fn schema_does_not_invent_application_semantics() {
    let schema = generated_schema();
    let list = schema["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "widget list")
        .unwrap();

    assert!(list.get("mutating").is_none());
    assert!(list.get("output_fields").is_none());
    assert!(schema.get("errors").is_none());
    assert!(schema.get("outcomes").is_none());
}

#[test]
fn schema_merges_explicit_application_metadata() {
    let metadata = librebar::cli::SchemaMetadata::new()
        .command(
            "widget list",
            librebar::cli::CommandMetadata::new()
                .mutating(false)
                .stability(librebar::cli::Stability::Stable)
                .output_field(librebar::cli::OutputField::new("id", "string"))
                .example(librebar::cli::CommandExample::new(["--all"])),
        )
        .error(
            librebar::cli::ErrorMetadata::new("not_found")
                .exit_code(4)
                .retryable(false)
                .description("Widget does not exist"),
        )
        .outcome(
            librebar::cli::OutcomeMetadata::new(3, "partial")
                .description("Some widgets were unavailable"),
        );

    let document = librebar::cli::schema_for::<SchemaCli>(&metadata).unwrap();
    let schema = serde_json::to_value(document).unwrap();
    let list = schema["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "widget list")
        .unwrap();

    assert_eq!(list["mutating"], false);
    assert_eq!(list["stability"], "stable");
    assert_eq!(list["output_fields"][0]["name"], "id");
    assert_eq!(list["example"]["args"], serde_json::json!(["--all"]));
    assert_eq!(schema["errors"][0]["kind"], "not_found");
    assert_eq!(schema["outcomes"][0]["name"], "partial");
}

#[test]
fn schema_rejects_unknown_metadata_command_paths() {
    let metadata = librebar::cli::SchemaMetadata::new().command(
        "widget typo",
        librebar::cli::CommandMetadata::new().mutating(false),
    );

    let error = librebar::cli::schema_for::<SchemaCli>(&metadata)
        .expect_err("unknown metadata paths must not disappear silently");
    assert!(error.to_string().contains("widget typo"));
}

#[test]
fn schema_rejects_overlapping_error_and_outcome_codes() {
    let metadata = librebar::cli::SchemaMetadata::new()
        .error(librebar::cli::ErrorMetadata::new("unavailable").exit_code(5))
        .outcome(librebar::cli::OutcomeMetadata::new(5, "different"));

    let error = librebar::cli::schema_for::<SchemaCli>(&metadata)
        .expect_err("error and outcome codes must be disjoint");
    assert!(error.to_string().contains("exit code 5"));
}

#[test]
fn schema_rejects_metadata_that_cannot_validate_as_cli_spec() {
    let invalid_kind =
        librebar::cli::SchemaMetadata::new().error(librebar::cli::ErrorMetadata::new("Not Found"));
    let error = librebar::cli::schema_for::<SchemaCli>(&invalid_kind).unwrap_err();
    assert!(error.to_string().contains("Not Found"));

    let zero_code = librebar::cli::SchemaMetadata::new()
        .outcome(librebar::cli::OutcomeMetadata::new(0, "empty"));
    let error = librebar::cli::schema_for::<SchemaCli>(&zero_code).unwrap_err();
    assert!(error.to_string().contains("exit code 0"));

    let empty_version = librebar::cli::SchemaMetadata::new().version("");
    let error = librebar::cli::schema_for::<SchemaCli>(&empty_version).unwrap_err();
    assert!(error.to_string().contains("version"));

    let empty_field = librebar::cli::SchemaMetadata::new().command(
        "widget list",
        librebar::cli::CommandMetadata::new()
            .output_field(librebar::cli::OutputField::new("", "string")),
    );
    let error = librebar::cli::schema_for::<SchemaCli>(&empty_field).unwrap_err();
    assert!(error.to_string().contains("output field"));
}

// ─── Pre-startup parse path ─────────────────────────────────────────

#[test]
fn parse_command_adds_visible_schema_subcommand() {
    let mut command =
        librebar::cli::command::<SchemaCli>().expect("schema should augment a normal command tree");
    let help = command.render_long_help().to_string();

    assert!(help.contains("schema"), "schema missing from help: {help}");
}

#[test]
fn parse_normal_arguments_returns_the_consumer_cli() {
    let outcome = librebar::cli::try_parse_from::<SchemaCli, _, _>(
        ["schema-app", "status"],
        librebar::cli::SchemaMetadata::new(),
    )
    .expect("normal command should parse");

    let librebar::cli::ParseOutcome::Run(cli) = outcome else {
        panic!("normal command was intercepted")
    };
    assert!(matches!(cli.command, SchemaCommands::Status));
}

#[test]
fn parse_intercepts_schema_before_constructing_the_consumer_cli() {
    let outcome = librebar::cli::try_parse_from::<SchemaCli, _, _>(
        ["schema-app", "schema"],
        librebar::cli::SchemaMetadata::new(),
    )
    .expect("schema command should be handled");

    let librebar::cli::ParseOutcome::Schema(schema) = outcome else {
        panic!("schema command reached the application")
    };
    assert_eq!(schema.name, "schema-app");
    assert!(
        schema
            .commands
            .iter()
            .any(|command| command.name == "schema")
    );
    let schema_command = schema
        .commands
        .iter()
        .find(|command| command.name == "schema")
        .unwrap();
    assert_eq!(schema_command.mutating, Some(false));
}

#[test]
fn parse_schema_command_filters_to_a_command_subtree() {
    let outcome = librebar::cli::try_parse_from::<SchemaCli, _, _>(
        ["schema-app", "schema", "widget"],
        librebar::cli::SchemaMetadata::new(),
    )
    .unwrap();
    let librebar::cli::ParseOutcome::Schema(schema) = outcome else {
        panic!("schema command reached the application")
    };

    assert!(!schema.commands.is_empty());
    assert!(
        schema
            .commands
            .iter()
            .all(|command| command.name == "widget" || command.name.starts_with("widget "))
    );
}

#[test]
fn parse_schema_command_rejects_an_unknown_filter() {
    let error = librebar::cli::try_parse_from::<SchemaCli, _, _>(
        ["schema-app", "schema", "typo"],
        librebar::cli::SchemaMetadata::new(),
    )
    .expect_err("unknown schema filters must fail");

    assert_eq!(error.kind(), clap::error::ErrorKind::InvalidValue);
    assert!(error.to_string().contains("typo"));
}

#[test]
fn parse_rejects_a_consumer_owned_schema_subcommand() {
    let error = librebar::cli::try_parse_from::<SchemaCollisionCli, _, _>(
        ["collision-app", "schema"],
        librebar::cli::SchemaMetadata::new(),
    )
    .expect_err("schema ownership must be unambiguous");

    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    assert!(error.to_string().contains("schema"));
}

// ─── Shell completions ──────────────────────────────────────────────

#[test]
fn parse_command_adds_visible_completions_subcommand() {
    let mut command = librebar::cli::command::<SchemaCli>().unwrap();
    let help = command.render_long_help().to_string();

    assert!(
        help.contains("completions"),
        "completions missing from help: {help}"
    );
}

#[test]
fn parse_intercepts_bash_completions() {
    let outcome = librebar::cli::try_parse_from::<SchemaCli, _, _>(
        ["schema-app", "completions", "bash"],
        librebar::cli::SchemaMetadata::new(),
    )
    .unwrap();
    let librebar::cli::ParseOutcome::Completions(bytes) = outcome else {
        panic!("completions command reached the application")
    };
    let script = String::from_utf8(bytes).unwrap();

    assert!(script.contains("schema-app"));
    assert!(script.contains("completions"));
    assert!(script.contains("schema"));
}

#[test]
fn parse_intercepts_zsh_completions() {
    let outcome = librebar::cli::try_parse_from::<SchemaCli, _, _>(
        ["schema-app", "completions", "zsh"],
        librebar::cli::SchemaMetadata::new(),
    )
    .unwrap();
    let librebar::cli::ParseOutcome::Completions(bytes) = outcome else {
        panic!("completions command reached the application")
    };
    let script = String::from_utf8(bytes).unwrap();

    assert!(script.contains("#compdef schema-app"));
}

#[test]
fn parse_rejects_a_consumer_owned_completions_subcommand() {
    let error = librebar::cli::try_parse_from::<CompletionCollisionCli, _, _>(
        ["completion-collision-app", "completions"],
        librebar::cli::SchemaMetadata::new(),
    )
    .expect_err("completions ownership must be unambiguous");

    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    assert!(error.to_string().contains("completions"));
}

// ─── Manpages ───────────────────────────────────────────────────────

#[test]
fn manpage_render_uses_the_augmented_command_tree() {
    let mut output = Vec::new();
    librebar::cli::render_manpage::<SchemaCli>(&mut output).unwrap();
    let roff = String::from_utf8(output).unwrap();

    assert!(roff.contains(".TH schema-app 1"));
    assert!(roff.contains("schema"));
    assert!(roff.contains("completions"));
    assert!(roff.contains("Exercise schema reflection"));
}

#[test]
fn manpage_generation_writes_collision_free_full_command_paths() {
    let directory = tempfile::tempdir().unwrap();
    let generated = librebar::cli::generate_manpages::<SchemaCli>(directory.path()).unwrap();
    let mut names: Vec<_> = generated
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();

    assert!(names.contains(&"schema-app.1".to_owned()), "{names:?}");
    assert!(
        names.contains(&"schema-app-widget.1".to_owned()),
        "{names:?}"
    );
    assert!(
        names.contains(&"schema-app-widget-list.1".to_owned()),
        "{names:?}"
    );
    assert!(
        names.contains(&"schema-app-schema.1".to_owned()),
        "{names:?}"
    );
    assert!(
        names.contains(&"schema-app-completions.1".to_owned()),
        "{names:?}"
    );

    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len());
    assert!(generated.iter().all(|path| path.is_file()));
}
