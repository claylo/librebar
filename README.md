# Librebar

[![CI](https://github.com/claylo/librebar/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/claylo/librebar/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/librebar.svg)](https://crates.io/crates/librebar)
[![docs.rs](https://docs.rs/librebar/badge.svg)](https://docs.rs/librebar)
[![MSRV](https://img.shields.io/badge/MSRV-1.89.0-blue.svg)](#msrv)

Opinionated application foundation for Rust CLIs and services. Add one dependency and get an agent-ready CLI, layered config with environment overrides, structured logging, crash dumps, file caching, and a diagnostics bundle — out of the box.

```rust,no_run
use anyhow::Result;
use librebar::cli::clap::{self, Parser};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    log_level: librebar::config::LogLevel,
    database_url: Option<String>,
}

#[derive(Parser)]
#[command(name = "myapp", version, about = "Does useful things")]
struct Cli {
    #[command(flatten)]
    pub common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    Info,
}

fn main() -> Result<()> {
    let cli = librebar::cli::parse::<Cli>();

    if cli.common.apply(env!("CARGO_PKG_VERSION"))?.is_exit() {
        return Ok(());
    }

    let app = librebar::init(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_cli(cli.common)
        .config::<Config>()
        .logging()
        .crash_handler()
        .start()?;

    match cli.command {
        Some(Commands::Info) => println!("config loaded from: {:?}", app.config_sources()),
        None => {}
    }
    Ok(())
}
```

Librebar is a library, not a framework. You own `main()`. You own your CLI struct. You own your config struct. Librebar handles the wiring that is identical across every project — and gets it right so you don't have to think about it.

## What you get

Out of the box, with no feature flags:

- **CLI** — typed `--format auto|text|json` output selection, `--quiet`/`--verbose`/`--color`/`--chdir`, machine-readable `schema` subcommand for agent discovery, shell completions, manpage generation
- **Config** — TOML/YAML/JSON file discovery with walk-up search, layered deep merge, typed environment variable overrides with `__` nesting, explicit file and programmatic CLI override layers
- **Logging** — JSONL structured logging to file with daily rotation, platform-aware log directories, never touches stdout
- **Crash handling** — structured JSON crash dumps on panic, written to XDG cache, chained with the default hook
- **Caching** — file-based key-value cache with TTL, collision-safe keys, XDG cache directory
- **Diagnostics** — `doctor` command framework and `.tar.gz` debug bundle builder

## Installation

```toml
[dependencies]
librebar = "0.3"
```

Default features give you the full foundation. To trim for a minimal binary, disable defaults and opt in:

```toml
[dependencies]
librebar = { version = "0.3", default-features = false, features = ["cli", "config", "logging"] }
```

## Features

### Default (the foundation)

| Feature | What it does |
|---------|-------------|
| `cli` | Typed output selection, CLI Spec schema, shared Clap arguments, completions, manpages |
| `config` | Layered config discovery, deep merge, environment overlays, TOML/YAML/JSON |
| `logging` | JSONL structured logging with daily rotation and platform-aware log directories |
| `crash` | Panic hook with structured JSON crash dumps written to the XDG cache directory |
| `cache` | File-based key-value cache with TTL (XDG cache directory) |
| `diagnostics` | `doctor` command framework + `.tar.gz` debug bundle builder |

### Opt-in (add what your project needs)

| Feature | What it does |
|---------|-------------|
| `http` | Hyper client with redirects, gzip/Brotli decompression, idempotent retries, timeouts, and rustls |
| `http-cookies` | Explicit per-client RFC 6265 cookie jars with JSON persistence |
| `http-cache` | RFC-aware private GET caching with ETag, Last-Modified, and `Vary` support |
| `update` | "Update available" notifications via the GitHub releases API (24-hour cache) |
| `shutdown` | Graceful shutdown with SIGINT/SIGTERM handling via `tokio::sync::watch` |
| `otel` | OpenTelemetry tracing export via OTLP/HTTP |
| `otel-http-json` | OpenTelemetry via OTLP/HTTP with JSON encoding |
| `otel-grpc` | OpenTelemetry via gRPC (adds Tonic transport) |
| `mcp` | Model Context Protocol server support (rmcp wrapper) |
| `lockfile` | Exclusive file locks to prevent concurrent instances |
| `dispatch` | Git-style `{app}-{subcommand}` plugin lookup on PATH |

### Benchmarking (dev-only)

| Feature | What it does |
|---------|-------------|
| `bench` | Wall-clock benchmarks via [divan](https://crates.io/crates/divan) (any platform) |
| `bench-gungraun` | Instruction-count benchmarks via [gungraun](https://crates.io/crates/gungraun) / Valgrind (Linux/Intel) |

Feature implications: `update` → `http` + `cache`; `http-cookies` → `http`; `http-cache` → `http` + `cache`; `dispatch` → `cli`; `diagnostics` → `config` + `logging`; `otel` → `logging`; `otel-http-json` → `otel`; `otel-grpc` → `otel`.

## Typical feature sets

```toml
# Full foundation (default features, nothing extra needed)
librebar = "0.3"

# Long-running service with graceful shutdown and observability
librebar = { version = "0.3", features = ["shutdown", "otel"] }

# CLI tool with update checks
librebar = { version = "0.3", features = ["update"] }

# Stateful HTTP client with an explicitly enabled cookie jar
librebar = { version = "0.3", features = ["http-cookies"] }

# RFC-aware persistent GET caching
librebar = { version = "0.3", features = ["http-cache"] }

# Plugin-extensible CLI (git-style subcommands)
librebar = { version = "0.3", features = ["dispatch"] }

# Minimal — just CLI and config, no logging or crash dumps
librebar = { version = "0.3", default-features = false, features = ["cli", "config"] }
```

The `http` client follows up to 10 redirects, transparently decodes gzip and
Brotli responses, and retries idempotent methods up to three times on 5xx or
transport failures with exponential backoff. Decoded response bodies are
limited to 16 MiB by default. Redirects cannot downgrade HTTPS to HTTP;
cross-origin redirects discard caller-supplied headers and request extensions,
then restore only the configured user-agent. `HttpClient::builder` can tune or
disable each behavior. Cookie handling remains stateless unless an individual
client calls `with_cookie_jar()` or `with_cookie_jar_from()`.

## CLI

Embed `CommonArgs` into your own clap struct with `#[command(flatten)]`:

```rust,no_run
# #[derive(librebar::cli::clap::Subcommand)]
# enum Commands { Info }
#[derive(librebar::cli::clap::Parser)]
struct Cli {
    #[command(flatten)]
    pub common: librebar::cli::CommonArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}
```

This gives every librebar-based app a consistent set of flags:

| Flag | Short | Effect |
|------|-------|--------|
| `--quiet` | `-q` | Only print errors |
| `--verbose` | `-v` | More detail (repeatable: `-vv` for trace) |
| `--format` | | `auto`, `text`, or `json` output |
| `--color` | | `auto`, `always`, or `never` |
| `--chdir` | `-C` | Run as if started in a different directory |
| `--version-only` | | Print version number and exit |

All of them are `global`, so they are accepted on any subcommand as well as at
the root. `myapp sub --version-only` prints the version and exits without
running `sub`.

`--format auto` resolves to text when stdout is a terminal and JSON when it is
redirected. An explicit format always wins. Use the typed result instead of
branching on a boolean:

```rust,no_run
# #[derive(librebar::cli::clap::Parser)]
# struct Cli {
#     #[command(flatten)]
#     common: librebar::cli::CommonArgs,
# }
# let cli = <Cli as librebar::cli::clap::Parser>::parse_from(["myapp"]);
match cli.common.output_format() {
    librebar::cli::ResolvedOutputFormat::Text => println!("human output"),
    librebar::cli::ResolvedOutputFormat::Json => println!(r#"{{"mode":"json"}}"#),
}
```

The old `--json` spelling remains accepted as a hidden compatibility alias for
`--format json`. Passing both selectors is an error.

That reserves `-q`, `-v` and `-C` across your whole command tree. Redeclaring
one in a subcommand is a clap conflict, and clap reports it as a panic on first
run rather than a compile error — so design your subcommand flags around these
three.

Parsing a flag is not the same as acting on it. Call `apply` once after
parsing and the whole set is live:

```rust,no_run
# #[derive(librebar::cli::clap::Parser)]
# struct Cli {
#     #[command(flatten)]
#     common: librebar::cli::CommonArgs,
# }
# fn main() -> librebar::Result<()> {
# let cli = <Cli as librebar::cli::clap::Parser>::parse_from(["myapp"]);
if cli.common.apply(env!("CARGO_PKG_VERSION"))?.is_exit() {
    return Ok(());
}
# Ok(())
# }
```

### Machine-readable schema

Use librebar's parser instead of calling Clap directly:

```rust,no_run
# #[derive(librebar::cli::clap::Parser)]
# struct Cli {}
let cli = librebar::cli::parse::<Cli>();
# let _ = cli;
```

It adds a visible `schema` subcommand and handles it before configuration,
logging, authentication, or network startup. `myapp schema` emits a CLI Spec
v0.2 document generated from the built Clap command tree. Large CLIs can narrow
the result with a path such as `myapp schema widgets list`.

Clap supplies command paths, descriptions, arguments, defaults, enums, value
hints, groups, aliases, and conflicts. It cannot know whether a command mutates
state or what its JSON and errors mean. Supply those facts explicitly:

```rust,no_run
# #[derive(librebar::cli::clap::Parser)]
# struct Cli {}
let metadata = librebar::cli::SchemaMetadata::new()
    .command(
        "widgets list",
        librebar::cli::CommandMetadata::new()
            .mutating(false)
            .stability(librebar::cli::Stability::Stable)
            .output_field(librebar::cli::OutputField::new("id", "string")),
    )
    .error(
        librebar::cli::ErrorMetadata::new("not_found")
            .exit_code(4)
            .retryable(false),
    );
let cli = librebar::cli::parse_with::<Cli>(metadata);
# let _ = cli;
```

Metadata must name a real command path, and error/outcome exit codes may not
overlap. Librebar rejects either mistake rather than publishing a quietly
incomplete contract. The root Clap command should use `#[command(version)]`;
alternatively, provide the application version through
`SchemaMetadata::version`.

This is a compliance-capable foundation, not an automatic compliance claim.
Applications still have to honor the selected format, emit declared structured
errors, keep data on stdout and diagnostics on stderr, and satisfy the CLI
Spec's behavioral rules.

### Completions and manpages

The same parse path also installs `completions <SHELL>` using Clap's official
generator. Bash, Elvish, Fish, PowerShell, and Zsh are supported without
application-specific wiring:

```bash
myapp completions zsh > _myapp
myapp completions bash > myapp.bash
```

For packaging, render one page or generate the complete visible command tree
through `clap_mangen`:

```rust,no_run
# #[derive(librebar::cli::clap::Parser)]
# struct Cli {}
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut page = Vec::new();
librebar::cli::render_manpage::<Cli>(&mut page)?;

let paths = librebar::cli::generate_manpages::<Cli>("target/man")?;
# let _ = paths;
# Ok(())
# }
```

Nested filenames contain the full path (`myapp-widgets-list.1`), preventing
same-named leaves in different command groups from overwriting each other.
Both outputs include librebar-owned commands because schema, help, completions,
and manpages all come from the same augmented `librebar::cli::clap::Command`.

`apply` sets the color override, prints the version and returns
`Startup::Exit` if `--version-only` was passed, and changes directory for
`-C` — in that order, so `--version-only` never fails because of an unrelated
bad `-C`, and the directory change lands before config discovery walks up from
the cwd. `Startup` is `#[must_use]`, so ignoring the result is a warning
rather than a silent bug.

The version string has to come from you: `env!("CARGO_PKG_VERSION")` expanded
inside librebar would yield librebar's version, not yours. Pass the same value
to `.with_version()` so crash dumps and OTEL resource attributes agree.

`apply_color()` and `apply_chdir()` remain available for apps that need a
different order or want to handle `--version-only` themselves.

For compact help (`-h` shows short help, `--help` shows long help):

```rust,no_run
use librebar::cli::clap::{CommandFactory, FromArgMatches};
# #[derive(librebar::cli::clap::Parser)]
# struct Cli {}
# fn main() -> Result<(), Box<dyn std::error::Error>> {

let cmd = librebar::cli::with_help_short(Cli::command());
let cli = Cli::from_arg_matches(&cmd.get_matches())?;
# let _ = cli;
# Ok(())
# }
```

## Config

Define your config struct with serde:

```rust
use librebar::camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    log_level: librebar::config::LogLevel,
    log_dir: Option<Utf8PathBuf>,
    database_url: Option<String>,
}
```

`camino` is re-exported by the `config` feature. Use `librebar::camino` rather
than adding your own dependency: `Utf8Path` shows up in librebar's public API,
and going through the re-export guarantees you get the same types librebar was
compiled against instead of a second, independently resolved copy.

### Discovery

The builder's `.config::<Config>()` discovers config files automatically. It walks up from the current directory looking for (in order):

1. `.config/{app}.{ext}`
2. `.{app}.{ext}`
3. `{app}.{ext}`

Then checks the user config directory (`~/.config/{app}/config.{ext}` on macOS/Linux).

Supported extensions: `.toml`, `.yaml`, `.yml`, `.json`. Walking stops at a `.git` boundary by default.

### Layered merge

Values merge with later layers winning. Objects merge recursively. Scalars and
arrays replace entirely. Your struct's `#[serde(default)]` values serve as the
base layer.

```text
defaults from Config::default()
  ← ~/.config/myapp/config.toml      (user config)
    ← ./myapp.toml                    (project config)
      ← MYAPP_*                       (environment)
        ← explicit file via --config
          ← typed CLI overrides       (highest precedence)
```

Environment variables override passively discovered files. An explicit
`--config foo.toml` is a deliberate choice, so that file overrides the
environment. Application-specific CLI flags remain the final word.

### Environment variables

The application name becomes an uppercase prefix: `my-app` uses `MY_APP_`.
Single underscores remain part of a field name; `__` crosses a nested struct
boundary.

```bash
MY_APP_DATABASE_URL='postgres://localhost/app'
MY_APP_DATABASE__POOL_SIZE=16
MY_APP_FEATURE_ENABLED=true
MY_APP_TAGS='["worker", "blue"]'
```

Values are parsed against the current default/discovered-file value. Strings
stay strings, numbers stay numeric, and arrays/objects use JSON syntax.
Booleans accept only lowercase `true` and `false`; `1`, `yes`, and `TRUE` are
errors. Quote JSON arrays and objects so the shell passes them intact.

An empty value is still a value: string fields receive `""`, while numeric,
boolean, array, and object fields report a parse error. A null schema position,
including `Option<T>` with a `None` default, is treated as a string because the
serialized schema cannot reveal `T`. A discovered file can provide a non-null
schema value for an optional non-string field.

Unknown prefixed paths are ignored by default. Dynamic-map consumers can opt
in to collecting them as strings:

```rust,no_run
use librebar::config::{ConfigLoader, UnknownEnvironment};
# #[derive(Default, serde::Deserialize, serde::Serialize)]
# struct Config {}
# fn main() -> librebar::Result<()> {

let (config, sources) = ConfigLoader::new("my-app")
    .with_unknown_environment(UnknownEnvironment::Collect)
    .load::<Config>()?;
let _ = (config, sources);
# Ok(())
# }
```

### Explicit files

Load from a specific path instead of discovery:

```rust,no_run
# #[derive(Default, serde::Deserialize, serde::Serialize)]
# struct Config {}
# struct Cli { pool_size: u16 }
# fn main() -> librebar::Result<()> {
# let config_path = librebar::camino::Utf8PathBuf::from("config.toml");
# let cli = Cli { pool_size: 16 };
let app = librebar::init("myapp")
    .config_from_file::<Config>(&config_path)
    .with_config_override("database.pool_size", cli.pool_size)
    .start()?;
# let _ = app;
# Ok(())
# }
```

Only add overrides for CLI flags the user actually supplied. Typed overrides
are applied in call order and beat every file and environment variable.

### Escape hatch

Skip the builder entirely and use the config module directly:

```rust,no_run
# #[derive(Default, serde::Deserialize, serde::Serialize)]
# struct Config {}
# fn main() -> librebar::Result<()> {
# let cwd = librebar::camino::Utf8Path::new(".");
let (config, sources) = librebar::config::ConfigLoader::new("myapp")
    .with_project_search(&cwd)
    .with_boundary_marker(".git")
    .load::<Config>()?;
# let _ = (config, sources);
# Ok(())
# }
```

Or load a pre-built config:

```rust,no_run
# #[derive(Default, serde::Deserialize, serde::Serialize)]
# struct Config {}
# fn main() -> librebar::Result<()> {
# let my_config = Config::default();
let app = librebar::init("myapp")
    .with_config(my_config)
    .start()?;
# let _ = app;
# Ok(())
# }
```

## Logging

The `logging` feature provides JSONL structured logging to file with daily rotation. Logs go to files or stderr, never stdout (which stays clear for application output like MCP communication).

### Log directory resolution

The logging system finds a writable log directory using this priority:

1. `{APP}_LOG_PATH` env var (exact file path)
2. `{APP}_LOG_DIR` env var (directory, appends `{app}.jsonl`)
3. `log_dir` from config
4. Platform default:
   - macOS: `~/Library/Logs/{app}/`
   - Linux: `$XDG_STATE_HOME/{app}/logs/`
5. `/var/log` on Unix
6. stderr (if no writable directory is found)

Where `{APP}` is the uppercased, hyphen-to-underscore app name (e.g., `my-tool` becomes `MY_TOOL_LOG_PATH`).

### Log level precedence

```text
--quiet       → error only
-v            → debug
-vv           → trace
RUST_LOG=...  → custom filter
(none)        → info (default)
```

### Direct usage

Use the logging module without the builder:

```rust,no_run
# fn main() -> librebar::Result<()> {
let log_cfg = librebar::logging::LoggingConfig::from_app_name("myapp");
let filter = librebar::logging::env_filter(false, 0, "info");
let _guard = librebar::logging::init(&log_cfg, filter)?;
# Ok(())
# }
```

Hold the guard for the application's lifetime. When it drops, logs flush.

## Builder API

The builder wires everything in the correct initialization order:

1. Load config (if requested)
2. Initialize logging (reads log settings from config if available)
3. Return `App<C>` with everything wired up

```rust,no_run
# #[derive(Default, serde::Deserialize, serde::Serialize)]
# struct Config {}
# #[derive(librebar::cli::clap::Parser)]
# struct Cli {
#     #[command(flatten)]
#     common: librebar::cli::CommonArgs,
# }
# fn main() -> librebar::Result<()> {
# let cli = <Cli as librebar::cli::clap::Parser>::parse_from(["myapp"]);
// Full setup — CLI, config, logging, crash handler
let app = librebar::init(env!("CARGO_PKG_NAME"))
    .with_cli(cli.common)
    .config::<Config>()
    .logging()
    .crash_handler()
    .start()?;

// Access initialized state
let cfg: &Config = app.config();
let sources = app.config_sources();
let cli_args = app.cli();
# let _ = (cfg, sources, cli_args);
# Ok(())
# }
```

Without config, `.start()` returns `App<()>`:

```rust,no_run
# #[derive(librebar::cli::clap::Parser)]
# struct Cli {
#     #[command(flatten)]
#     common: librebar::cli::CommonArgs,
# }
# fn main() -> librebar::Result<()> {
# let cli = <Cli as librebar::cli::clap::Parser>::parse_from(["myapp"]);
let app = librebar::init("myapp")
    .with_cli(cli.common)
    .logging()
    .start()?;
# let _ = app;
# Ok(())
# }
```

## Testing

The default test suite runs fully offline and finishes in under a second:

```sh
just check       # fmt + clippy (--all-features) + deny + nextest + doc-tests
just test        # just the nextest run, if you want to skip linting
```

### Opt-in network tests

Two tests in `tests/http_test.rs` hit the public internet
(`api.github.com/zen`, `httpbin.org/get`) and are marked `#[ignore]` so
they don't pretend to pass when they haven't actually run. Opt in with:

```sh
# Run only the ignored tests (nextest):
cargo nextest run --all-features --run-ignored only --test http_test

# Or with the stock runner:
cargo test --all-features --test http_test -- --ignored
```

### End-to-end OTEL export

The `otel` feature builds an OTLP/HTTP exporter that lights up whenever
`OTEL_EXPORTER_OTLP_ENDPOINT` is set to a non-empty value. To watch spans
arrive from the `service` example, stand up a receiver. The .NET Aspire
Dashboard bundles an OTLP/HTTP ingestor plus a unified UI for logs,
traces, and metrics in a single image:

```sh
# Start the dashboard (UI on 18888, OTLP/HTTP ingestor on 18890):
docker run --rm -d -p 18888:18888 \
    -e DOTNET_DASHBOARD_OTLP_HTTP_ENDPOINT_URL=http://0.0.0.0:18890 \
    -p 18890:18890 \
    mcr.microsoft.com/dotnet/aspire-dashboard:latest

# Run the service with export enabled:
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:18890 \
    cargo run --example service \
    --features "shutdown,otel" \
    -- -C examples run

# Open the UI; spans from the `service` service show up under that name:
#   http://localhost:18888
#
# First-launch login token is printed to the container logs:
#   docker logs $(docker ps -q --filter ancestor=mcr.microsoft.com/dotnet/aspire-dashboard)
```

Any OTLP/HTTP receiver works the same way — Jaeger's `all-in-one` image,
the OpenTelemetry Collector, or commercial backends (Honeycomb, Grafana
Cloud, etc.). Point `OTEL_EXPORTER_OTLP_ENDPOINT` at their ingest URL and
spans flow.

OTLP/HTTP uses protobuf encoding by default. To send JSON, enable the
`otel-http-json` feature and set `OTEL_EXPORTER_OTLP_PROTOCOL=http/json`.
Requesting JSON without the feature returns a configuration error rather than
silently sending protobuf.

## Versioning

librebar follows semantic versioning with the Rust-ecosystem pre-1.0
convention:

| Release track | Behavior |
|---------------|----------|
| **0.x** (current) | Minor bumps (`0.1.0` → `0.2.0`) **may contain breaking changes**. Patch bumps (`0.1.0` → `0.1.1`) are additive or bug-fix only. |
| **1.0 and beyond** | Strict semver. Breaking changes require a major bump. |

### What counts as breaking

During the 0.x line, the following changes warrant a minor bump:

- Removing or renaming any public item (type, function, method, module,
  feature flag).
- Changing a public function's signature in a way that breaks existing
  call sites — including parameter type changes, return-type changes,
  or trait-bound tightening.
- Removing or renaming a variant on the [`Error`](src/error.rs) enum
  (or any of its per-module companions: `HttpError`, `CacheError`,
  `ConfigParseError`). These enums are all `#[non_exhaustive]`, so
  **adding** a variant is additive and ships in a patch.
- Changing the semantics of a stable API (e.g., a method that previously
  returned `Ok(None)` now returns `Err`).
- Raising the MSRV beyond what is documented in `rust-version` in
  `Cargo.toml`.

The following changes are **not** breaking and can land in a patch:

- Adding new public items (types, functions, methods, feature flags).
- Adding new variants to `Error`, `HttpError`, `CacheError`, or
  `ConfigParseError` (all `#[non_exhaustive]`).
- Adding new optional config fields that have `#[serde(default)]`.
- Internal refactoring, performance improvements, and dependency bumps
  that don't change the public surface.

### Dependency types in public APIs

Librebar exposes established ecosystem types when they are part of a feature's
extension API. Reach those dependencies through the feature module that owns
the contract: `cli::clap`, `config::serde_json`, `logging::tracing_subscriber`,
`otel::tracing_subscriber`, and `mcp::rmcp`. HTTP protocol and buffer types are
re-exported directly from `librebar::http`.

Runtime implementation details remain private. In particular, Hyper is the
HTTP transport rather than the public HTTP type boundary, and MCP's stdio
helper does not expose Tokio's concrete stdin and stdout types. Changing a
deliberately exposed dependency type follows the same versioning rules as any
other public API change.

See [Update dependency imports](docs/migrations/2026-08-01-public-api-boundaries.md)
for the call-site changes and dependency cleanup steps.

### MSRV

The minimum supported Rust version is pinned in `Cargo.toml`'s
`rust-version` field (currently `1.89.0`) and tested against in CI.
MSRV increases are treated as breaking and batched into minor bumps
during the 0.x line and into major bumps after 1.0.

### When does 1.0 ship

When the public API holds stable across two consecutive minor releases
with no breaking changes. No external gate, no calendar deadline.

**Using librebar anywhere?** Open an
[issue on GitHub](https://github.com/claylo/librebar/issues) with a
one-line "using it for X" — not a gate for 1.0, just an invitation.
External consumers surface ergonomic issues that self-dogfooding can't,
and earlier signal makes for a better 1.0.

Until then, pin to a specific minor version (`librebar = "0.3"`) if you
want the patch-only guarantee.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) for the local checks, pull request
title format, and template to use for each kind of change.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
