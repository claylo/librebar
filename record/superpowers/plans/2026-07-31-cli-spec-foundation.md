# CLI Spec Foundation Implementation Plan

**Status:** Implemented

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Do not create a worktree.

**Goal:** Give every librebar CLI typed output negotiation and a pre-startup,
Clap-derived CLI Spec v0.2 `schema` command with explicit application metadata.

**Architecture:** `src/cli/parse.rs` owns the augmented parse path and terminal
command interception. `src/cli/schema.rs` converts a fully built Clap command
to serializable CLI Spec structures and merges checked application metadata.
`src/cli.rs` retains shared arguments and owns output format resolution.

**Tech Stack:** Rust 2024, Clap 4.6 reflection, Serde, existing optional
`serde_json`. No new crate dependency in this slice.

**Design:**
`record/superpowers/specs/2026-07-31-agent-ready-cli-foundation.md`

---

## File map

| File | Responsibility |
|---|---|
| `src/cli.rs` | Shared flags, typed output resolution, child modules and public re-exports |
| `src/cli/parse.rs` | Augmented command construction, exiting and non-exiting parse APIs, schema interception |
| `src/cli/schema.rs` | CLI Spec model, Clap reflection, application metadata, validation and filtering |
| `tests/cli_test.rs` | Consumer-shaped format, reflection, metadata, parse, and help contracts |
| `Cargo.toml` | Make the existing optional `serde_json` dependency available to `cli` |
| `README.md` | New startup API, format behavior, schema metadata, limits of compliance |
| `examples/*.rs` | Exercise `librebar::cli::parse` with Clap-owned package versions |

---

### Task 1: Replace the boolean output hint with a typed contract

**Files:** `tests/cli_test.rs`, `src/cli.rs`, `src/lib.rs`

- [x] Add failing tests proving `--format` defaults to `Auto`, explicit values
  parse, `resolve_for(true/false)` chooses text/JSON, hidden `--json` maps to
  JSON, and explicit `--format` conflicts with `--json`.
- [x] Run `cargo test --all-features --test cli_test output_` and confirm the
  tests fail because the typed API does not exist.
- [x] Add `OutputFormat`, `ResolvedOutputFormat`, the global `format` argument,
  a crate-visible hidden compatibility flag, and `CommonArgs::output_format` /
  `output_format_for`. Update the builder's `default_cli` literal.
- [x] Run the targeted tests and the complete `cli_test` target; expect PASS.

The public shape introduced by this task is:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat { #[default] Auto, Text, Json }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedOutputFormat { Text, Json }

impl CommonArgs {
    pub fn output_format(&self) -> ResolvedOutputFormat;
    pub const fn output_format_for(&self, stdout_is_terminal: bool)
        -> ResolvedOutputFormat;
}
```

### Task 2: Generate honest CLI Spec documents from Clap

**Files:** `tests/cli_test.rs`, `src/cli/schema.rs`, `src/cli.rs`, `Cargo.toml`

- [x] Add a nested consumer CLI fixture and failing tests for command paths,
  global/local argument separation, primitive/array/path/boolean types,
  defaults, enums, aliases, value hints, groups, and conflicts.
- [x] Add failing tests showing mutation/output/error/outcome fields are absent
  without metadata, present when supplied, and rejected for unknown command
  paths or overlapping error/outcome exit codes.
- [x] Run the targeted schema tests and confirm RED due to missing schema API.
- [x] Add serializable CLI Spec types, `SchemaMetadata` and its builder types,
  `schema_for::<Cli>()`, Clap tree walking, safe type reflection, metadata
  validation, and command-path filtering.
- [x] Enable the already-declared `serde_json` dependency from the `cli`
  feature and run the targeted tests; expect PASS.

Application semantics use explicit builders rather than prose parsing:

```rust
let metadata = librebar::cli::SchemaMetadata::new()
    .command(
        "widget list",
        librebar::cli::CommandMetadata::new()
            .mutating(false)
            .output_field(librebar::cli::OutputField::new("id", "string")),
    )
    .error(librebar::cli::ErrorMetadata::new("not_found")
        .exit_code(4)
        .retryable(false));
```

### Task 3: Intercept a real schema command before startup

**Files:** `tests/cli_test.rs`, `src/cli/parse.rs`, `src/cli.rs`

- [x] Add failing tests proving root help contains `schema`, normal args return
  `ParseOutcome::Run`, `schema` returns a document without constructing the
  consumer CLI, filtering works, and a consumer-owned `schema` collides.
- [x] Run the targeted parse tests and confirm RED due to missing parse API.
- [x] Implement `command::<Cli>()`, `try_parse_from::<Cli>()`,
  `parse::<Cli>()`, and `parse_with::<Cli>()`. The exiting wrapper mirrors
  Clap; the testable core returns `ParseOutcome`.
- [x] Run the targeted tests and full `cli_test`; expect PASS.

The parse API is:

```rust
pub enum ParseOutcome<T> { Run(T), Schema(SchemaDocument) }

pub fn parse<T: clap::Parser>() -> T;
pub fn parse_with<T: clap::Parser>(metadata: SchemaMetadata) -> T;
pub fn try_parse_from<T, I, S>(args: I, metadata: SchemaMetadata)
    -> Result<ParseOutcome<T>, clap::Error>
where
    T: clap::Parser,
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone;
```

### Task 4: Move consumers onto the best-practice path

**Files:** `README.md`, `src/lib.rs`, all CLI examples

- [x] Update examples to declare `#[command(version)]` and call
  `librebar::cli::parse::<Cli>()` before `CommonArgs::apply` and builder startup.
- [x] Update README/rustdoc tables and examples for `--format`, hidden `--json`,
  schema discovery, metadata, filtering, and the distinction between a
  compliance-capable foundation and application-level compliance.
- [x] Run `cargo test --doc --all-features` and every example via
  `cargo check --all-features --examples`; expect PASS.
- [x] Run `just check`; expect formatting, Clippy, dependency policy, tests,
  and doc-tests all to pass.

### Task 5: Record the completed slice

**Files:** `scratch/TODO.txt`, this plan

- [x] Mark all preceding checkboxes complete and add the CLI Spec foundation to
  the TODO status ledger without deleting the remaining artifact work.
- [x] Run `git --no-pager diff --check` and inspect
  `git --no-pager diff --stat`.
- [x] Leave the worktree uncommitted for Clay's `commit.txt`/`gtxt` workflow.
