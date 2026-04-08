# Rebar: Rust Application Foundation Crate

**Date:** 2026-04-06
**Status:** Design approved, pending implementation plan

## Problem

claylo-rs is a Copier template that generates production-ready Rust CLI applications. Over 30+ feature flags, it generates ~1,500 lines of runtime behavior (config loading, structured logging, OTEL export, MCP server wiring, CLI scaffolding) as Jinja-templated Rust code. Every improvement requires re-copying and reconciling diffs across downstream projects.

This is a library masquerading as a template. Runtime behavior that is identical across projects should be a versioned crate, not generated text. Improvements flow downstream through `cargo update`, not `copier recopy`.

## Prior Art

An ecosystem survey (April 2026) found no existing crate that fills this role:

- **`foundations` (Cloudflare)** — right scope, wrong primitives (slog, serde_yaml, Linux-centric)
- **`abscissa_core` (iqlusion)** — right primitives, wrong pattern (framework with inversion of control)
- **`cli-batteries`** — right idea, dead since 2023
- **`figue` (bearcove)** — CLI parser, not a config library; facet ecosystem, no serde; 10 weeks old

The gap is real: a feature-flagged Rust application foundation that composes CLI, config, logging, observability, and lifecycle management as a library, not a framework.

## Design Principles

1. **You own `main()`.** Rebar never calls you. You call rebar. No inversion of control.
2. **Toggle, not wire.** Adding a capability should be one word in a features list, not 80 lines of boilerplate. The wiring is rebar's job.
3. **Explicit over magic.** The builder is explicit (you see each `.method()` call), not auto-detecting. Each module is also usable independently.
4. **No unnecessary deps.** No default features. You opt into exactly what you need.
5. **Phase yamalgam in.** Config works today with toml + serde_saphyr + serde_json. yamalgam upgrades the YAML path and adds provenance later.

## Crate Structure

Single crate, optional dependencies gated by Cargo features. No workspace, no sub-crates.

```
rebar/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Re-exports, prelude, App type, builder
│   ├── cli.rs              # CommonArgs, ColorChoice, HelpShort  (feature: cli)
│   ├── config.rs           # ConfigLoader<C>, discovery, merge   (feature: config)
│   ├── logging.rs          # JSONL layer, log target resolution   (feature: logging)
│   ├── otel.rs             # OTLP setup, TracerProvider           (feature: otel)
│   ├── mcp.rs              # rmcp server helpers                  (feature: mcp)
│   ├── http.rs             # HTTP client with tracing             (feature: http)
│   ├── shutdown.rs         # Signal handling, cleanup hooks       (feature: shutdown)
│   ├── crash.rs            # Panic hook, crash dumps              (feature: crash)
│   ├── update.rs           # Version check, self-update           (feature: update)
│   ├── lockfile.rs         # Exclusive operation locking          (feature: lockfile)
│   ├── cache.rs            # XDG cache, TTL, offline support      (feature: cache)
│   ├── dispatch.rs         # Git-style external command dispatch  (feature: dispatch)
│   ├── diagnostics.rs      # Doctor framework, debug bundles      (feature: diagnostics)
│   └── bench.rs            # Benchmark harness helpers            (feature: bench)
└── tests/
```

## Feature Dependency Graph

```
cli          (standalone — clap, owo-colors)
config       (standalone — toml, serde_saphyr, serde_json, camino, directories)
logging      (standalone — tracing-subscriber, tracing-appender, serde_json, directories)
otel         (implies logging — opentelemetry, opentelemetry-otlp [http-proto, hyper-client], tracing-opentelemetry, tokio)
otel-grpc    (implies otel — adds tonic for gRPC transport)
mcp          (standalone — rmcp, tokio)
http         (standalone — hyper, hyper-util, tokio, tracing)
shutdown     (standalone — tokio::signal when tokio present, ctrlc otherwise)
crash        (standalone — minimal deps)
update       (implies http — version check + download)
lockfile     (standalone — fd-lock)
cache        (standalone — directories, serde)
dispatch     (implies cli — which)
diagnostics  (implies config + logging)
bench        (dev-only — divan)
```

No default features. The template generates the right feature list per preset.

### HTTP Stack

One HTTP stack: hyper. Used by `otel` (OTLP export via hyper-client), `http` (general client), and `update` (version check). No reqwest, no ureq. When `otel` or `mcp` is enabled, tokio + hyper are already present — `http` adds zero incremental deps.

## Core API: Builder + App

### App Type

```rust
pub struct App<C = ()> {
    config: C,
    config_sources: ConfigSources,
    cli: CommonArgs,
    _logging_guard: Option<LoggingGuard>,
}

impl<C> App<C> {
    pub fn config(&self) -> &C { &self.config }
    pub fn config_sources(&self) -> &ConfigSources { &self.config_sources }
    pub fn cli(&self) -> &CommonArgs { &self.cli }
}
```

### Builder (Approach C: builder with escape hatches)

```rust
let app = rebar::init(env!("CARGO_PKG_NAME"))
    .with_cli(cli.common)       // pass parsed CommonArgs
    .config::<Config>()          // discover + load config
    .logging()                   // JSONL logging
    .otel()                      // OTLP export
    .shutdown()                  // signal handlers
    .crash_handler()             // panic hook
    .start()?;                   // init everything in correct order
```

**Init order (handled by `start()`):**

Note: color, chdir, and version_only are handled by the user in `main.rs` before calling the builder. The builder handles:

1. Load config (if `.config::<C>()` was called)
2. Init logging (reads log_level and log_dir from config if available)
3. Init OTEL (after logging, reads otel_endpoint from config if available)
4. Register shutdown hooks (after OTEL, so it can flush on exit)
5. Install crash handler (last — catches panics in all above)

### Escape Hatches

```rust
// Pre-load config with custom logic
let my_config = rebar::config::load_from_file::<Config>("custom/path.toml")?;
let app = rebar::init(env!("CARGO_PKG_NAME"))
    .with_config(my_config)
    .logging()
    .start()?;

// Or bypass builder entirely
let config = rebar::config::discover::<Config>("myapp", &cwd)?;
let _guard = rebar::logging::init_with(&config)?;
```

### CLI Parsing

CLI parsing stays with the user. Rebar provides `CommonArgs` (shared flags) and `ColorChoice`. The user's `Cli` struct and `Commands` enum are theirs.

```rust
#[derive(Parser)]
#[command(name = "fancy", about = "Does fancy things")]
#[command(arg_required_else_help = true, disable_help_flag = true)]
pub struct Cli {
    #[command(flatten)]
    pub common: rebar::cli::CommonArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}
```

### Generated main.rs (~30 lines)

```rust
use anyhow::Result;
use clap::FromArgMatches;
use fancy::{Cli, Commands, commands};
use fancy_core::config::Config;

fn main() -> Result<()> {
    let cli = Cli::from_arg_matches(&fancy::command().get_matches())
        .expect("clap mismatch");

    if cli.common.version_only {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let Some(command) = cli.command else { return Ok(()) };

    let app = rebar::init(env!("CARGO_PKG_NAME"))
        .with_cli(cli.common)
        .config::<Config>()
        .logging()
        .start()?;

    match command {
        Commands::Info(args) => commands::info::cmd_info(args, &app),
        Commands::Doctor(args) => commands::doctor::cmd_doctor(args, &app),
    }
}
```

## Config Module

### Discovery (same as current template, extracted)

1. `.config/{app}.{ext}` walking up from cwd
2. `.{app}.{ext}` walking up from cwd
3. `{app}.{ext}` walking up from cwd
4. `~/.config/{app}/config.{ext}` (XDG user config)
5. Defaults from `C::default()`

Stops at `.git` boundary (configurable). Extensions: `.toml`, `.yaml`, `.yml`, `.json`.

### Merge (no figment, no config-rs)

Parse each discovered file into `serde_json::Value` (TOML via `toml` crate, YAML via `serde_saphyr`, JSON via `serde_json`). Deep-merge values (overlay wins for scalars, recursive merge for objects). Deserialize merged result via `serde_json::from_value::<C>()`.

The merge function is ~50 lines. No external config framework dependency.

### Provenance (phased)

**Phase 1 (rebar v0.1):** `ConfigSources` tracks which files were loaded. Good enough for `doctor` and `info` commands.

**Phase 2 (after yamalgam Span threading):** Per-field provenance. yamalgam's Value DOM carries source Span metadata through composition. Rebar can report "this value came from line 7 of ~/.config/myapp/config.toml."

## Observability

### Logging (feature: `logging`)

Direct extraction of current template's `observability.rs`:

- `JsonLogLayer` — custom tracing Layer writing JSONL to file
- `resolve_log_target` — log directory resolution (env var → config → platform default → cwd)
- `platform_log_dir` — macOS ~/Library/Logs, Linux XDG state, Windows LocalAppData
- `env_filter` — quiet/verbose/RUST_LOG precedence
- `LoggingGuard` — held by App, ensures clean flush on drop

Service name and env var prefix parameterized instead of Jinja-baked.

### OTEL (feature: `otel`, implies `logging`)

Key change from current template: **HTTP-proto + hyper-client as default.** gRPC available via `otel-grpc` feature.

- TracerProvider setup with resource attributes (service.name, deployment.environment, service.version)
- OTLP span export via HTTP protobuf (default) or gRPC (feature: `otel-grpc`)
- Graceful shutdown (provider flushed on Drop/shutdown signal)
- Reads endpoint from config or `OTEL_EXPORTER_OTLP_ENDPOINT` env var
- Protocol selection via `OTEL_EXPORTER_OTLP_PROTOCOL` env var (http/protobuf default, grpc when `otel-grpc` enabled)

Escape hatch: `rebar::otel::build_provider()` for custom OTEL setup, pass to builder via `.with_tracer_provider()`.

## Lifecycle Features

### Graceful Shutdown (feature: `shutdown`, roadmap #37)

```rust
let app = rebar::init("myapp").shutdown().start()?;

// Cancellation token pattern
let token = app.shutdown_token();
tokio::select! {
    _ = do_work() => {},
    _ = token.cancelled() => { /* cleanup */ },
}
```

Registers SIGTERM/SIGINT handlers. OTEL TracerProvider flushed automatically. Logging guard dropped cleanly.

### Crash Reporting (feature: `crash`, roadmap #30)

Custom panic hook. Writes structured crash info (panic message, backtrace, OS, version) to XDG cache. User-friendly message instead of raw stack trace. No network dependency.

### Lockfile (feature: `lockfile`, roadmap #36)

`fd-lock` based exclusive operation locking. Lock file in XDG runtime directory. Fails fast if another instance holds it. `--force` override support.

### Update Notifications (feature: `update`, roadmap #28)

Checks GitHub releases API via `http` feature. Caches result in XDG cache (once per day). Respects `{APP}_NO_UPDATE_CHECK=1`. Non-blocking.

### Diagnostics (feature: `diagnostics`, roadmap #39 + #40)

Doctor command framework — register checks, run them, report summary. Debug bundle export — collects sanitized config, recent logs, doctor output into a tar.gz.

### HTTP Client (feature: `http`, roadmap #31)

Thin wrapper around hyper with tracing integration, timeouts, sensible defaults. Shared HTTP stack with OTEL.

### Cache (feature: `cache`, roadmap #33)

XDG cache storage, TTL configuration, `--offline` flag pattern, cache management.

### External Command Dispatch (feature: `dispatch`, roadmap #23)

Git-style `{app}-{subcommand}` lookup on PATH.

### Benchmarks (feature: `bench`)

Dev-only. divan harness setup and helpers.

## Scope Boundary

**Rebar handles:** Things that are the same in every project but tedious to wire up. Graceful shutdown, crash reporting, HTTP client with tracing, config discovery, structured logging, OTEL export, version checking, lockfiles, diagnostics, external command dispatch, benchmark harness.

**Not rebar (stays in user code or template):**

- REPL mode (#35) — project-specific (your commands, your completions)
- Daemon mode (#34) — project-specific (your service loop)
- Plugin system (#24) — depends on what plugins do
- Anonymous telemetry (#38) — consent flow is project-specific
- Config extensions (#25) — plugin config namespacing is project-specific
- Secrets/credentials (#26) — pluggability-first design, rebar may provide trait, user provides backend
- Table output (#20) — UI choice, direct dep
- Progress bars (#19) — UI choice, direct dep
- Interactive prompts (#21) — UI choice, direct dep

## Dependency Decisions

| Dep | Decision | Rationale |
|---|---|---|
| **clap** | keep | No lighter alternative with derive + subcommands + completions |
| **figment** | drop | Dormant (no commits since Sept 2024), depends on deprecated serde_yaml |
| **config-rs** | skip | 64 deps; direct parse + merge is lighter |
| **OTEL features** | reduce default, gRPC opt-in | `otel` = http-proto + hyper-client (106 crates); `otel-grpc` adds tonic (~10 more) |
| **rmcp** | keep | Official MCP SDK, best API ergonomics |
| **schemars** | keep (rmcp requires it) | Wait for rmcp to make it optional |
| **hyper** | use for all HTTP | One HTTP stack; shared with OTEL |
| **ureq** | drop | hyper already present via OTEL/HTTP features |
| **reqwest** | drop | hyper replaces it; reqwest contradicts ureq preference anyway |
| **yamalgam** | Phase 2 | Config works without it initially; adds provenance later |

## Impact on claylo-rs

### Template files that disappear

| File | Lines | Replaced by |
|---|---|---|
| `observability.rs.jinja` | ~750 | `rebar::logging` + `rebar::otel` |
| `config.rs.jinja` (loader) | ~620 | `rebar::config` + ~15 line user Config struct |
| Most of `lib.rs.jinja` | ~100 | `rebar::cli::CommonArgs` |
| Half of `main.rs.jinja` | ~75 | `rebar::init().start()` |

~1,500 lines of Jinja-templated Rust replaced by ~30 lines of rebar API calls.

### What stays in the template

- Project structure (Cargo.toml, workspace layout, directory structure)
- CI/CD workflows, deny.toml, clippy.toml
- Community files, license, .gitignore, .editorconfig
- User's `Cli` struct with `#[command(flatten)] rebar::cli::CommonArgs`
- User's `Commands` enum (subcommand definitions)
- User's `Config` struct (project-specific fields)
- Thin `main.rs` glue (~30 lines)
- Non-Rust structural files

### copier.yaml

Feature flags (`has_config`, `has_jsonl_logging`, etc.) remain as user-facing prompts but map to `rebar = { features = [...] }` in Cargo.toml instead of conditional Jinja code blocks.

### Testing

- Conditional file tests stay (verify Cargo.toml features, file presence)
- Preset build tests get faster (less generated code, heavy deps cached in rebar)
- Feature interaction testing moves to rebar's CI
- Progressive tests (22-minute runs) become largely redundant

## Distribution

Git dependency initially. Crate name doesn't matter.

```toml
rebar = { git = "https://github.com/claylo/rebar", features = ["cli", "config", "logging"] }
```

Pin to tag or branch for stability across projects. Publish to crates.io later if it stabilizes.

## Sequencing

### Phase 1: Minimum viable rebar

- `cli` — CommonArgs, ColorChoice, HelpShort
- `config` — Phase 1 discovery + merge (toml + serde_saphyr + serde_json, no yamalgam)
- `logging` — JSONL layer, log target resolution, env_filter
- Builder + App type

Enough to port the standard preset. One project adopts it.

### Phase 2: Full preset parity

- `otel` — OTLP with reduced deps (http-proto + hyper-client)
- `mcp` — rmcp wrapper
- `shutdown` — signal handling
- `crash` — panic hook

### Phase 3: Roadmap features

- `http`, `update`, `lockfile`, `cache`, `dispatch`, `diagnostics`, `bench`
- yamalgam integration for config provenance

### Phase 4: Template migration

- Update claylo-rs to generate rebar-based projects
- Simplify copier.yaml
- Reduce template test surface
