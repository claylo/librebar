# Librebar

Opinionated application foundation for Rust CLIs and services. Wire up CLI flags, layered config, and structured logging in about 30 lines.

```rust
use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    log_level: librebar::config::LogLevel,
    database_url: Option<String>,
}

#[derive(Parser)]
#[command(name = "myapp", about = "Does useful things")]
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
    let cli = Cli::parse();
    cli.common.apply_color();

    let app = librebar::init(env!("CARGO_PKG_NAME"))
        .with_cli(cli.common)
        .config::<Config>()
        .logging()
        .start()?;

    match cli.command {
        Some(Commands::Info) => println!("config loaded from: {:?}", app.config_sources()),
        None => {}
    }
    Ok(())
}
```

Librebar is a library, not a framework. You own `main()`. You own your CLI struct. You own your config struct. Librebar handles the wiring that is identical across every project.

## Installation

Add librebar to your `Cargo.toml` with the features you need:

```toml
[dependencies]
librebar = { git = "https://github.com/claylo/librebar", features = ["cli", "config", "logging"] }
```

No default features. You opt into exactly what you need.

## Features

No default features. Opt in to exactly what your application needs.

### Core application

| Feature | What it does |
|---------|-------------|
| `cli` | `CommonArgs` with `--quiet`, `-v`, `--json`, `--color`, `-C`, `--version-only` |
| `config` | Layered config discovery, deep merge, TOML/YAML/JSON parsing |
| `logging` | JSONL structured logging with daily rotation and platform-aware log directories |
| `shutdown` | Graceful shutdown with SIGINT/SIGTERM handling via `tokio::sync::watch` |
| `crash` | Panic hook with structured JSON crash dumps written to the XDG cache directory |

### Networking and data

| Feature | What it does |
|---------|-------------|
| `http` | HTTPS client with tracing, timeouts, user-agent (rustls + Mozilla CA roots) |
| `cache` | File-based key-value cache with TTL (XDG cache directory) |
| `update` | "Update available" notifications via the GitHub releases API (24-hour cache) |

### Integration

| Feature | What it does |
|---------|-------------|
| `otel` | OpenTelemetry tracing export via OTLP/HTTP |
| `otel-grpc` | OpenTelemetry via gRPC (adds Tonic transport) |
| `mcp` | Model Context Protocol server support (rmcp wrapper) |

### Operational

| Feature | What it does |
|---------|-------------|
| `lockfile` | Exclusive file locks to prevent concurrent instances |
| `dispatch` | Git-style `{app}-{subcommand}` plugin lookup on PATH |
| `diagnostics` | `doctor` command framework + `.tar.gz` debug bundle builder |

### Benchmarking (dev-only)

| Feature | What it does |
|---------|-------------|
| `bench` | Wall-clock benchmarks via [divan](https://crates.io/crates/divan) (any platform) |
| `bench-gungraun` | Instruction-count benchmarks via [gungraun](https://crates.io/crates/gungraun) / Valgrind (Linux/Intel) |

Some features automatically enable their dependencies: `update` → `http` + `cache`; `dispatch` → `cli`; `diagnostics` → `config` + `logging`; `otel` → `logging`; `otel-grpc` → `otel`.

## CLI

Embed `CommonArgs` into your own clap struct with `#[command(flatten)]`:

```rust
#[derive(clap::Parser)]
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
| `--json` | | Output as JSON for scripting |
| `--color` | | `auto`, `always`, or `never` |
| `--chdir` | `-C` | Run as if started in a different directory |
| `--version-only` | | Print version number and exit |

For compact help (`-h` shows short help, `--help` shows long help):

```rust
use clap::CommandFactory;

let cmd = librebar::cli::with_help_short(Cli::command());
let cli = Cli::from_arg_matches(&cmd.get_matches())?;
```

## Config

Define your config struct with serde:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Config {
    log_level: librebar::config::LogLevel,
    log_dir: Option<camino::Utf8PathBuf>,
    database_url: Option<String>,
}
```

### Discovery

The builder's `.config::<Config>()` discovers config files automatically. It walks up from the current directory looking for (in order):

1. `.config/{app}.{ext}`
2. `.{app}.{ext}`
3. `{app}.{ext}`

Then checks the user config directory (`~/.config/{app}/config.{ext}` on macOS/Linux).

Supported extensions: `.toml`, `.yaml`, `.yml`, `.json`. Walking stops at a `.git` boundary by default.

### Layered merge

When multiple config files are found, values merge with later files winning. Objects merge recursively. Scalars and arrays replace entirely. Your struct's `#[serde(default)]` values serve as the base layer.

```
defaults from Config::default()
  ← ~/.config/myapp/config.toml      (user config)
    ← ./myapp.toml                    (project config)
      ← explicit file via --config    (highest precedence)
```

### Explicit files

Load from a specific path instead of discovery:

```rust
let app = librebar::init("myapp")
    .config_from_file::<Config>(&config_path)
    .start()?;
```

### Escape hatch

Skip the builder entirely and use the config module directly:

```rust
let (config, sources) = librebar::config::ConfigLoader::new("myapp")
    .with_project_search(&cwd)
    .with_boundary_marker(".git")
    .load::<Config>()?;
```

Or load a pre-built config:

```rust
let app = librebar::init("myapp")
    .with_config(my_config)
    .start()?;
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
5. Current directory (last resort)

Where `{APP}` is the uppercased, hyphen-to-underscore app name (e.g., `my-tool` becomes `MY_TOOL_LOG_PATH`).

### Log level precedence

```
--quiet       → error only
-v            → debug
-vv           → trace
RUST_LOG=...  → custom filter
(none)        → info (default)
```

### Direct usage

Use the logging module without the builder:

```rust
let log_cfg = librebar::logging::LoggingConfig::from_app_name("myapp");
let filter = librebar::logging::env_filter(false, 0, "info");
let _guard = librebar::logging::init(&log_cfg, filter)?;
```

Hold the guard for the application's lifetime. When it drops, logs flush.

## Builder API

The builder wires everything in the correct initialization order:

1. Load config (if requested)
2. Initialize logging (reads log settings from config if available)
3. Return `App<C>` with everything wired up

```rust
// Full setup — CLI, config, and logging
let app = librebar::init(env!("CARGO_PKG_NAME"))
    .with_cli(cli.common)
    .config::<Config>()
    .logging()
    .start()?;

// Access initialized state
let cfg: &Config = app.config();
let sources = app.config_sources();
let cli_args = app.cli();
```

Without config, `.start()` returns `App<()>`:

```rust
let app = librebar::init("myapp")
    .with_cli(cli.common)
    .logging()
    .start()?;
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
