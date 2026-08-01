# Agent-ready CLI foundation

**Status:** implemented
**Date:** 2026-07-31

## Product rule

Librebar should make mature CLI behavior the default. Applications still own
their domain commands and rendering, but they should not repeatedly rediscover
schema introspection, output negotiation, completions, or manual-page
generation.

The line is epistemic: librebar may publish facts present in Clap's command
model or explicitly supplied by the application. It must never infer output
fields, error kinds, outcomes, stability, or mutation safety from prose.

## Adopted standards and tools

- The CLI Spec v0.2 is the agent-facing contract.
- Clap's built `Command` tree is the source of truth for invocation metadata.
- `clap_complete` is the shell-completion implementation.
- `clap_mangen` is the manual-page implementation.

Librebar adapts these projects; it does not introduce a competing schema,
completion grammar, or roff generator.

## Startup API

`librebar::cli::parse::<Cli>()` replaces direct `Cli::parse()` in normal
consumer startup. It builds and validates the complete Clap tree, installs a
real visible `schema` subcommand, and parses arguments before configuration,
logging, authentication, network access, or any other application startup.

The convenient function follows Clap's existing process-level behavior:
help, version, parse failures, and librebar-owned terminal commands write their
response and exit. A separate `try_parse_from` API returns a `ParseOutcome`
without exiting so applications and librebar can test the same path.

`parse::<Cli>()` generates a structurally valid schema using only Clap facts.
`parse_with::<Cli>(SchemaMetadata)` additionally merges explicit application
contracts. The metadata also provides a version escape hatch; without it,
schema generation requires the root Clap command to declare a version.

Librebar rejects application metadata naming a command path that does not
exist. A typo must not silently erase a safety or output contract.

## Output contract

`CommonArgs` exposes `--format auto|text|json`, defaulting to `auto`.

- Explicit `text` and `json` always win.
- `auto` resolves to text when stdout is a terminal and JSON otherwise.
- The old `--json` spelling remains accepted as a hidden compatibility flag
  equivalent to `--format json`.
- Supplying both `--json` and an explicit `--format` is an error rather than an
  order-dependent result.

The public Rust contract is a typed `OutputFormat`/`ResolvedOutputFormat`, not
a boolean that merely suggests the application might implement JSON. Librebar
provides the decision; the application owns its text renderer and serialized
domain value.

## Generated schema

The document declares CLI Spec `0.2`, the Clap command name/version/about,
default TTY/piped output behavior, global arguments, flattened invocable
command paths, errors, and outcomes.

From Clap, librebar emits:

- command paths, descriptions, aliases, and hidden state;
- long/short/positional argument names;
- required state, value arity, actions, defaults, possible values, aliases,
  value hints, groups, and discoverable conflicts;
- best-effort primitive types based on Clap's actual value parser and action.

Unknown custom parser types remain `string`. That is a safe transport-level
description, not a guess about the application's domain type.

Application metadata supplies:

- command mutation markers and stability;
- structured output fields;
- executable examples;
- structured error kinds, exit codes, retryability, and descriptions;
- non-error outcomes.

The `schema [COMMAND_PATH]...` positional filter narrows large trees while
retaining top-level metadata. A missing path is a normal Clap usage error.

## High-value generated artifacts

After the foundation lands, the same augmented `Command` tree powers:

1. A visible `completions <SHELL>` terminal subcommand using `clap_complete`.
2. Public build/release helpers that render root and subcommand manpages using
   `clap_mangen` without requiring applications to understand roff.

Dynamic completion remains out of scope while `clap_complete` labels it
unstable. External-subcommand integration remains a separate dispatch design;
schema generation will report that the command accepts external subcommands
but will not invent plugin commands that are not present.

## Compatibility and failure behavior

- Existing application commands and flags keep their spelling.
- `--json` remains accepted but disappears from normal help.
- A consumer-defined `schema` or `completions` command is rejected with a
  focused construction error because silently choosing one implementation
  would make the advertised contract ambiguous.
- Normal commands remain application-owned. Librebar intercepts only the
  subcommands it installs.
- No version numbers are manually changed as part of this work.

## Verification

- Unit tests exercise output resolution, legacy compatibility, schema
  reflection, metadata rejection, filtering, and non-exiting parse outcomes.
- Consumer-shaped integration tests prove the synthetic command appears in
  help and runs before application startup.
- Generated schema is validated in tests against its required CLI Spec v0.2
  shape; `clispec score` is a downstream compliance check because full
  conformance also depends on application behavior librebar cannot prove.
- `just check` remains the repository-wide gate.
