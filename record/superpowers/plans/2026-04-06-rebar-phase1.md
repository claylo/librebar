# Rebar Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the minimum viable rebar crate with `cli`, `config`, and `logging` features plus the builder/App orchestration layer.

**Architecture:** Single library crate with optional feature-gated modules. Builder pattern orchestrates initialization order. Each module is also usable independently (escape hatches). Code is extracted from claylo-rs template with parameterization replacing Jinja.

**Tech Stack:** Rust 2024 edition, clap (derive), tracing + tracing-subscriber + tracing-appender, serde + serde_json, toml, serde-saphyr, camino, directories, owo-colors, thiserror, anyhow

**Spec:** `record/superpowers/specs/2026-04-06-rebar-design.md`

**Source material:** The code being extracted lives in the claylo-rs template at `~/source/claylo/claylo-rs/claylo-rs/template/`. Key files:
- `{% if has_cli %}crates{% endif %}/{{ project_name if has_cli else "" }}/src/lib.rs.jinja` — CLI struct, ColorChoice, CommonArgs, HelpShort
- `{% if has_cli %}crates{% endif %}/{{ project_name + "-core" if has_core_library else "" }}/src/{% if has_config %}config.rs{% endif %}.jinja` — ConfigLoader, discovery, merge
- `{% if has_cli %}crates{% endif %}/{{ project_name + "-core" if has_core_library else "" }}/src/{% if has_core_library and (has_jsonl_logging or has_opentelemetry) %}observability.rs{% endif %}.jinja` — logging layer, log target resolution
- `{% if has_cli %}crates{% endif %}/{{ project_name if has_cli else "" }}/src/main.rs.jinja` — orchestration that becomes the builder

---

## File Structure

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Feature flags, optional deps, profiles |
| `src/lib.rs` | Crate root: `init()`, `App<C>`, `Builder`, module re-exports |
| `src/cli.rs` | `CommonArgs`, `ColorChoice`, `command()` helper |
| `src/config.rs` | `ConfigLoader<C>`, `ConfigSources`, `LogLevel`, discovery, deep merge |
| `src/logging.rs` | `JsonLogLayer`, `LoggingGuard`, `env_filter`, log target resolution |
| `src/error.rs` | `Error` enum, `Result<T>` alias |
| `tests/cli_test.rs` | CommonArgs parsing, ColorChoice behavior |
| `tests/config_test.rs` | Merge, discovery, ConfigLoader API |
| `tests/logging_test.rs` | env_filter, log target resolution, timestamp formatting |
| `tests/builder_test.rs` | Builder API, App accessors, feature interaction |

---

### Task 1: Scaffold Project

**Files:**
- Create: `Cargo.toml`, `src/lib.rs`, `src/error.rs`, plus structural files via template

- [ ] **Step 1: Scaffold with claylo-rs library preset**

```bash
cd ~/source/claylo/claylo-rs/claylo-rs
copier copy --trust --defaults \
  --data preset=library \
  --data project_name=rebar \
  --data owner=claylo \
  --data copyright_name="Clay Loveless" \
  --data project_description="Rust application foundation crate" \
  --data has_benchmarks=false \
  template ~/source/claylo/rebar
```

Note: `has_benchmarks=false` for now — benchmarks are a later feature.

- [ ] **Step 2: Verify scaffold**

```bash
cd ~/source/claylo/rebar
ls src/
```

Expected: `lib.rs`, `error.rs`

- [ ] **Step 3: Initialize git repo**

```bash
cd ~/source/claylo/rebar
git init && git add -A && git commit -m "chore: scaffold from claylo-rs library preset"
```

- [ ] **Step 4: Rewrite Cargo.toml for rebar features**

Replace the `[dependencies]` and `[dev-dependencies]` sections in `Cargo.toml`. Keep the `[package]`, `[lints]`, and `[profile.*]` sections from the scaffold.

The `[dependencies]` section should be:

```toml
[dependencies]
# Always present (core)
serde = { version = "1.0", features = ["derive"] }
tracing = "0.1"
thiserror = "2.0"

# Feature: cli
clap = { version = "4.6", features = ["derive"], optional = true }
owo-colors = { version = "4.3", features = ["supports-colors"], optional = true }
anyhow = { version = "1.0", optional = true }

# Feature: config
toml = { version = "0.8", optional = true }
serde-saphyr = { version = "0.0", optional = true }
serde_json = { version = "1.0", optional = true }
camino = { version = "1.2", features = ["serde1"], optional = true }
directories = { version = "6.0", optional = true }

# Feature: logging
tracing-subscriber = { version = "0.3", features = ["env-filter"], optional = true }
tracing-appender = { version = "0.2", optional = true }

[dev-dependencies]
tempfile = "3.27"
serde_json = "1.0"

[features]
cli = ["dep:clap", "dep:owo-colors", "dep:anyhow"]
config = ["dep:toml", "dep:serde-saphyr", "dep:serde_json", "dep:camino", "dep:directories"]
logging = ["dep:tracing-subscriber", "dep:tracing-appender", "dep:serde_json", "dep:directories"]
```

Note: `serde_json` and `directories` appear in both `config` and `logging` features. Cargo deduplicates — the dep is compiled once regardless of which feature enables it.

- [ ] **Step 5: Replace src/error.rs**

```rust
//! Error types for rebar.

use thiserror::Error;

/// Errors that can occur during rebar initialization.
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration file could not be parsed.
    #[error("failed to parse config file {path}: {source}")]
    ConfigParse {
        /// Path to the config file that failed.
        path: String,
        /// Underlying parse error.
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Configuration deserialization failed.
    #[error("failed to deserialize config: {0}")]
    ConfigDeserialize(Box<dyn std::error::Error + Send + Sync>),

    /// No configuration file found (when one was required).
    #[error("no configuration file found")]
    ConfigNotFound,

    /// Log directory is not writable.
    #[error("no writable log directory found")]
    LogDirNotWritable,

    /// I/O error during initialization.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result type alias using rebar's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 6: Replace src/lib.rs with module stubs**

```rust
//! Rebar: Rust application foundation crate.
//!
//! Feature-gated modules for CLI, config, logging, and more.
//! Use the builder to orchestrate initialization:
//!
//! ```ignore
//! let app = rebar::init(env!("CARGO_PKG_NAME"))
//!     .with_cli(cli.common)
//!     .config::<Config>()
//!     .logging()
//!     .start()?;
//! ```
#![deny(unsafe_code)]

pub mod error;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "config")]
pub mod config;

#[cfg(feature = "logging")]
pub mod logging;

pub use error::{Error, Result};
```

- [ ] **Step 7: Create module stubs so it compiles**

Create `src/cli.rs`:
```rust
//! CLI argument types shared across rebar-based applications.
```

Create `src/config.rs`:
```rust
//! Configuration discovery, loading, and merging.
```

Create `src/logging.rs`:
```rust
//! Structured JSONL logging with daily rotation.
```

- [ ] **Step 8: Verify it compiles with each feature**

```bash
cargo check --no-default-features
cargo check --features cli
cargo check --features config
cargo check --features logging
cargo check --features cli,config,logging
```

Expected: All pass with no errors.

- [ ] **Step 9: Commit**

Write `commit.txt`:
```
feat: initial rebar crate with feature-gated module stubs

Sets up the crate structure with cli, config, and logging features.
All modules are stubs that compile but have no implementation yet.
```

---

### Task 2: CLI Module

**Files:**
- Modify: `src/cli.rs`
- Create: `tests/cli_test.rs`

- [ ] **Step 1: Write failing test for CommonArgs**

Create `tests/cli_test.rs`:

```rust
#![cfg(feature = "cli")]

use clap::Parser;

/// Test harness that embeds rebar's CommonArgs the way a consumer would.
#[derive(Parser, Debug)]
#[command(name = "test-app")]
struct TestCli {
    #[command(flatten)]
    pub common: rebar::cli::CommonArgs,

    #[command(subcommand)]
    pub command: Option<TestCommands>,
}

#[derive(clap::Subcommand, Debug)]
enum TestCommands {
    Info,
}

#[test]
fn common_args_defaults() {
    let cli = TestCli::parse_from(["test-app", "info"]);
    assert!(!cli.common.quiet);
    assert_eq!(cli.common.verbose, 0);
    assert!(!cli.common.json);
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
fn common_args_json_flag() {
    let cli = TestCli::parse_from(["test-app", "--json", "info"]);
    assert!(cli.common.json);
}

#[test]
fn common_args_chdir() {
    let cli = TestCli::parse_from(["test-app", "-C", "/tmp", "info"]);
    assert_eq!(
        cli.common.chdir.as_ref().map(|p| p.as_path()),
        Some(std::path::Path::new("/tmp"))
    );
}

#[test]
fn color_choice_default_is_auto() {
    let cli = TestCli::parse_from(["test-app", "info"]);
    assert!(matches!(cli.common.color, rebar::cli::ColorChoice::Auto));
}

#[test]
fn color_choice_never() {
    let cli = TestCli::parse_from(["test-app", "--color", "never", "info"]);
    assert!(matches!(cli.common.color, rebar::cli::ColorChoice::Never));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features cli -E 'test(cli_test)'
```

Expected: FAIL — `CommonArgs` and `ColorChoice` don't exist yet.

- [ ] **Step 3: Implement cli module**

Write `src/cli.rs`:

```rust
//! CLI argument types shared across rebar-based applications.
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

/// Common CLI arguments shared across all rebar-based applications.
///
/// Embed in your app's CLI struct with `#[command(flatten)]`:
///
/// ```ignore
/// #[derive(Parser)]
/// struct MyCli {
///     #[command(flatten)]
///     pub common: rebar::cli::CommonArgs,
///     #[command(subcommand)]
///     pub command: Option<MyCommands>,
/// }
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
/// ```ignore
/// let cmd = rebar::cli::with_help_short(MyCli::command());
/// let cli = MyCli::from_arg_matches(&cmd.get_matches())?;
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features cli -E 'test(cli_test)'
```

Expected: All 7 tests PASS.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features cli --all-targets --message-format=short -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat(cli): add CommonArgs, ColorChoice, and HelpShort helper

Extracts the shared CLI argument types from the claylo-rs template.
CommonArgs provides quiet, verbose, json, color, chdir, and version_only
flags that consumers embed via #[command(flatten)].
```

---

### Task 3: Config Merge and File Parsing

**Files:**
- Modify: `src/config.rs`
- Create: `tests/config_test.rs`

This task implements the merge utilities and format-specific parsing. Discovery comes in Task 4.

- [ ] **Step 1: Write failing tests for deep_merge and file parsing**

Create `tests/config_test.rs`:

```rust
#![cfg(feature = "config")]

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// ─── deep_merge tests ───────────────────────────────────────────────

#[test]
fn merge_scalar_override() {
    let mut base = json!({"level": "info"});
    rebar::config::deep_merge(&mut base, json!({"level": "debug"}));
    assert_eq!(base["level"], "debug");
}

#[test]
fn merge_nested_objects() {
    let mut base = json!({"logging": {"level": "info", "dir": "/var/log"}});
    rebar::config::deep_merge(&mut base, json!({"logging": {"level": "debug"}}));
    assert_eq!(base["logging"]["level"], "debug");
    assert_eq!(base["logging"]["dir"], "/var/log"); // preserved
}

#[test]
fn merge_array_replaces() {
    let mut base = json!({"tags": ["a", "b"]});
    rebar::config::deep_merge(&mut base, json!({"tags": ["c"]}));
    assert_eq!(base["tags"], json!(["c"]));
}

#[test]
fn merge_adds_new_keys() {
    let mut base = json!({"a": 1});
    rebar::config::deep_merge(&mut base, json!({"b": 2}));
    assert_eq!(base, json!({"a": 1, "b": 2}));
}

#[test]
fn merge_null_overlay_replaces() {
    let mut base = json!({"a": 1});
    rebar::config::deep_merge(&mut base, json!({"a": null}));
    assert!(base["a"].is_null());
}

// ─── file parsing tests ─────────────────────────────────────────────

#[test]
fn parse_toml_to_value() {
    let content = r#"
        log_level = "debug"
        [nested]
        key = "value"
    "#;
    let value = rebar::config::parse_toml(content).unwrap();
    assert_eq!(value["log_level"], "debug");
    assert_eq!(value["nested"]["key"], "value");
}

#[test]
fn parse_yaml_to_value() {
    let content = "log_level: debug\nnested:\n  key: value\n";
    let value = rebar::config::parse_yaml(content).unwrap();
    assert_eq!(value["log_level"], "debug");
    assert_eq!(value["nested"]["key"], "value");
}

#[test]
fn parse_json_to_value() {
    let content = r#"{"log_level": "debug", "nested": {"key": "value"}}"#;
    let value = rebar::config::parse_json(content).unwrap();
    assert_eq!(value["log_level"], "debug");
    assert_eq!(value["nested"]["key"], "value");
}

// ─── deserialization into typed config ──────────────────────────────

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
struct TestConfig {
    log_level: rebar::config::LogLevel,
    log_dir: Option<camino::Utf8PathBuf>,
    custom_field: Option<String>,
}

#[test]
fn merge_and_deserialize() {
    let base = r#"log_level = "info""#;
    let overlay = r#"custom_field = "hello""#;

    let mut merged = rebar::config::parse_toml(base).unwrap();
    rebar::config::deep_merge(&mut merged, rebar::config::parse_toml(overlay).unwrap());

    let config: TestConfig = serde_json::from_value(merged).unwrap();
    assert_eq!(config.log_level, rebar::config::LogLevel::Info);
    assert_eq!(config.custom_field.as_deref(), Some("hello"));
}

#[test]
fn log_level_default_is_info() {
    assert_eq!(rebar::config::LogLevel::default(), rebar::config::LogLevel::Info);
}

#[test]
fn log_level_as_str() {
    assert_eq!(rebar::config::LogLevel::Debug.as_str(), "debug");
    assert_eq!(rebar::config::LogLevel::Info.as_str(), "info");
    assert_eq!(rebar::config::LogLevel::Warn.as_str(), "warn");
    assert_eq!(rebar::config::LogLevel::Error.as_str(), "error");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features config -E 'test(config_test)'
```

Expected: FAIL — config module functions and types don't exist yet.

- [ ] **Step 3: Implement config merge and parsing**

Write `src/config.rs`:

```rust
//! Configuration discovery, loading, and merging.
//!
//! Provides format-agnostic config file discovery, layered merging, and
//! deserialization into user-defined config types.
//!
//! # Supported formats
//!
//! - TOML (`.toml`)
//! - YAML (`.yaml`, `.yml`)
//! - JSON (`.json`)

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

/// Supported configuration file extensions (in order of preference).
const CONFIG_EXTENSIONS: &[&str] = &["toml", "yaml", "yml", "json"];

// ─── LogLevel ───────────────────────────────────────────────────────

/// Log level configuration, deserializable from config files.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Verbose output for debugging and development.
    Debug,
    /// Standard operational information (default).
    #[default]
    Info,
    /// Warnings about potential issues.
    Warn,
    /// Errors that indicate failures.
    Error,
}

impl LogLevel {
    /// Returns the log level as a lowercase string slice.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

// ─── ConfigSources ──────────────────────────────────────────────────

/// Metadata about which configuration sources were loaded.
///
/// Returned alongside the config from [`ConfigLoader::load()`] so commands
/// like `doctor` and `info` can report the actual config files.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ConfigSources {
    /// Project config file found by walking up from the search root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_file: Option<Utf8PathBuf>,
    /// User config file from XDG config directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_file: Option<Utf8PathBuf>,
    /// Explicit config files loaded (e.g., from `--config` flag).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub explicit_files: Vec<Utf8PathBuf>,
}

impl ConfigSources {
    /// Returns the highest-precedence config file that was loaded.
    ///
    /// Precedence: explicit files > project file > user file.
    pub fn primary_file(&self) -> Option<&Utf8Path> {
        self.explicit_files
            .last()
            .map(Utf8PathBuf::as_path)
            .or(self.project_file.as_deref())
            .or(self.user_file.as_deref())
    }
}

// ─── Deep Merge ─────────────────────────────────────────────────────

/// Deep-merge `overlay` into `base`.
///
/// - Objects: recursively merge, overlay keys win.
/// - Scalars and arrays: overlay replaces base.
pub fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                deep_merge(base_map.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, overlay) => *base = overlay,
    }
}

// ─── File Parsing ───────────────────────────────────────────────────

/// Parse TOML content into a `serde_json::Value`.
pub fn parse_toml(content: &str) -> Result<Value> {
    let toml_value: toml::Value = toml::from_str(content).map_err(|e| Error::ConfigParse {
        path: "<toml>".to_string(),
        source: Box::new(e),
    })?;
    serde_json::to_value(toml_value).map_err(|e| Error::ConfigDeserialize(Box::new(e)))
}

/// Parse YAML content into a `serde_json::Value`.
pub fn parse_yaml(content: &str) -> Result<Value> {
    let yaml_value: Value =
        serde_saphyr::from_str(content).map_err(|e| Error::ConfigParse {
            path: "<yaml>".to_string(),
            source: Box::new(e),
        })?;
    Ok(yaml_value)
}

/// Parse JSON content into a `serde_json::Value`.
pub fn parse_json(content: &str) -> Result<Value> {
    serde_json::from_str(content).map_err(|e| Error::ConfigParse {
        path: "<json>".to_string(),
        source: Box::new(e),
    })
}

/// Parse a config file, detecting format from extension.
pub fn parse_file(path: &Utf8Path) -> Result<Value> {
    let content = std::fs::read_to_string(path.as_str()).map_err(|e| Error::ConfigParse {
        path: path.to_string(),
        source: Box::new(e),
    })?;

    match path.extension() {
        Some("toml") => parse_toml(&content),
        Some("yaml" | "yml") => parse_yaml(&content),
        Some("json") => parse_json(&content),
        _ => parse_toml(&content), // default to TOML
    }
    .map_err(|e| match e {
        Error::ConfigParse { source, .. } => Error::ConfigParse {
            path: path.to_string(),
            source,
        },
        other => other,
    })
}

// ─── ConfigLoader ───────────────────────────────────────────────────

/// Builder for loading configuration from multiple sources.
///
/// Discovers config files by walking up directories, loads user config
/// from XDG directories, merges all sources, and deserializes into the
/// consumer's config type.
#[derive(Debug, Default)]
pub struct ConfigLoader {
    app_name: String,
    project_search_root: Option<Utf8PathBuf>,
    include_user_config: bool,
    boundary_marker: Option<String>,
    explicit_files: Vec<Utf8PathBuf>,
}

impl ConfigLoader {
    /// Create a new config loader for the given application name.
    ///
    /// The app name is used for XDG directory lookup and config file names.
    pub fn new(app_name: &str) -> Self {
        Self {
            app_name: app_name.to_string(),
            project_search_root: None,
            include_user_config: true,
            boundary_marker: Some(".git".to_string()),
            explicit_files: Vec::new(),
        }
    }

    /// Set the starting directory for project config search.
    pub fn with_project_search<P: AsRef<Utf8Path>>(mut self, path: P) -> Self {
        self.project_search_root = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set whether to include user config from XDG directory.
    pub const fn with_user_config(mut self, include: bool) -> Self {
        self.include_user_config = include;
        self
    }

    /// Set a boundary marker to stop directory traversal (default: `.git`).
    pub fn with_boundary_marker<S: Into<String>>(mut self, marker: S) -> Self {
        self.boundary_marker = Some(marker.into());
        self
    }

    /// Disable boundary marker (search all the way to filesystem root).
    pub fn without_boundary_marker(mut self) -> Self {
        self.boundary_marker = None;
        self
    }

    /// Add an explicit config file to load (highest precedence).
    pub fn with_file<P: AsRef<Utf8Path>>(mut self, path: P) -> Self {
        self.explicit_files.push(path.as_ref().to_path_buf());
        self
    }

    /// Load configuration, merging all discovered sources.
    ///
    /// Returns the merged config alongside metadata about which files
    /// were loaded.
    #[tracing::instrument(skip(self), fields(app = %self.app_name, search_root = ?self.project_search_root))]
    pub fn load<C: serde::de::DeserializeOwned + Default>(self) -> Result<(C, ConfigSources)> {
        tracing::debug!("loading configuration");
        let mut merged = serde_json::to_value(C::default())
            .map_err(|e| Error::ConfigDeserialize(Box::new(e)))?;
        let mut sources = ConfigSources::default();

        // User config (lowest precedence of file sources)
        if self.include_user_config {
            if let Some(user_config) = self.find_user_config() {
                if let Ok(value) = parse_file(&user_config) {
                    deep_merge(&mut merged, value);
                    sources.user_file = Some(user_config);
                }
            }
        }

        // Project config
        if let Some(ref root) = self.project_search_root {
            if let Some(project_config) = self.find_project_config(root) {
                if let Ok(value) = parse_file(&project_config) {
                    deep_merge(&mut merged, value);
                    sources.project_file = Some(project_config);
                }
            }
        }

        // Explicit files (highest precedence)
        for file in &self.explicit_files {
            let value = parse_file(file)?;
            deep_merge(&mut merged, value);
        }
        sources.explicit_files = self.explicit_files;

        let config: C =
            serde_json::from_value(merged).map_err(|e| Error::ConfigDeserialize(Box::new(e)))?;
        tracing::info!("configuration loaded");
        Ok((config, sources))
    }

    /// Load configuration, returning an error if no config file is found.
    pub fn load_or_error<C: serde::de::DeserializeOwned + Default>(
        &self,
    ) -> Result<(C, ConfigSources)> {
        let has_user = self.include_user_config && self.find_user_config().is_some();
        let has_project = self
            .project_search_root
            .as_ref()
            .and_then(|root| self.find_project_config(root))
            .is_some();
        let has_explicit = !self.explicit_files.is_empty();

        if !has_user && !has_project && !has_explicit {
            return Err(Error::ConfigNotFound);
        }

        // Clone self's fields to create a new loader for the actual load
        ConfigLoader {
            app_name: self.app_name.clone(),
            project_search_root: self.project_search_root.clone(),
            include_user_config: self.include_user_config,
            boundary_marker: self.boundary_marker.clone(),
            explicit_files: self.explicit_files.clone(),
        }
        .load()
    }

    /// Find project config by walking up from the given directory.
    fn find_project_config(&self, start: &Utf8Path) -> Option<Utf8PathBuf> {
        let mut current = Some(start.to_path_buf());

        while let Some(dir) = current {
            for ext in CONFIG_EXTENSIONS {
                // .config/app.ext
                let dotconfig = dir.join(format!(".config/{}.{ext}", self.app_name));
                if dotconfig.is_file() {
                    return Some(dotconfig);
                }

                // .app.ext
                let dotfile = dir.join(format!(".{}.{ext}", self.app_name));
                if dotfile.is_file() {
                    return Some(dotfile);
                }

                // app.ext
                let regular = dir.join(format!("{}.{ext}", self.app_name));
                if regular.is_file() {
                    return Some(regular);
                }
            }

            // Check boundary after checking config (so same-dir config is found)
            if let Some(ref marker) = self.boundary_marker
                && dir.join(marker).exists()
                && dir != start
            {
                break;
            }

            current = dir.parent().map(Utf8Path::to_path_buf);
        }

        None
    }

    /// Find user config in XDG config directory.
    fn find_user_config(&self) -> Option<Utf8PathBuf> {
        let proj_dirs = directories::ProjectDirs::from("", "", &self.app_name)?;
        let config_dir = proj_dirs.config_dir();

        for ext in CONFIG_EXTENSIONS {
            let config_path = config_dir.join(format!("config.{ext}"));
            if config_path.is_file() {
                return Utf8PathBuf::from_path_buf(config_path).ok();
            }
        }

        None
    }
}

// ─── XDG Helpers ────────────────────────────────────────────────────

/// Get the user config directory for an application.
pub fn user_config_dir(app_name: &str) -> Option<Utf8PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Utf8PathBuf::from_path_buf(proj_dirs.config_dir().to_path_buf()).ok()
}

/// Get the user cache directory for an application.
pub fn user_cache_dir(app_name: &str) -> Option<Utf8PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Utf8PathBuf::from_path_buf(proj_dirs.cache_dir().to_path_buf()).ok()
}

/// Get the user data directory for an application.
pub fn user_data_dir(app_name: &str) -> Option<Utf8PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Utf8PathBuf::from_path_buf(proj_dirs.data_dir().to_path_buf()).ok()
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features config -E 'test(config_test)'
```

Expected: All tests PASS.

- [ ] **Step 5: Commit**

Write `commit.txt`:
```
feat(config): add config merge, file parsing, and discovery

Implements deep_merge for layered config values, parsers for TOML/YAML/JSON
into serde_json::Value, ConfigLoader with directory-walking discovery,
and ConfigSources for provenance tracking. No figment dependency —
merge is done directly over serde_json::Value.
```

---

### Task 4: Config Discovery Tests

**Files:**
- Modify: `tests/config_test.rs`

Adds filesystem-based tests for the ConfigLoader discovery logic.

- [ ] **Step 1: Add discovery tests**

Append to `tests/config_test.rs`:

```rust
use std::fs;
use tempfile::TempDir;

// ─── ConfigLoader discovery tests ───────────────────────────────────

#[test]
fn loader_defaults_when_no_files() {
    let loader = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_boundary_marker();

    let (config, sources): (TestConfig, _) = loader.load().unwrap();
    assert_eq!(config.log_level, rebar::config::LogLevel::Info);
    assert!(sources.primary_file().is_none());
}

#[test]
fn loader_explicit_file_overrides_default() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    fs::write(&config_path, r#"log_level = "debug""#).unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let (config, sources): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .with_file(&config_path)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Debug);
    assert!(sources.primary_file().is_some());
}

#[test]
fn loader_later_file_overrides_earlier() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base.toml");
    fs::write(&base, r#"log_level = "warn""#).unwrap();
    let over = tmp.path().join("override.toml");
    fs::write(&over, r#"log_level = "error""#).unwrap();

    let base = camino::Utf8PathBuf::try_from(base).unwrap();
    let over = camino::Utf8PathBuf::try_from(over).unwrap();

    let (config, _): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .with_file(&base)
        .with_file(&over)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Error);
}

#[test]
fn loader_discovers_dotfile() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("project");
    let sub_dir = project_dir.join("src").join("deep");
    fs::create_dir_all(&sub_dir).unwrap();

    fs::write(
        project_dir.join(".test-app.toml"),
        r#"log_level = "debug""#,
    )
    .unwrap();

    let sub_dir = camino::Utf8PathBuf::try_from(sub_dir).unwrap();

    let (config, sources): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_boundary_marker()
        .with_project_search(&sub_dir)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Debug);
    assert!(sources.project_file.is_some());
}

#[test]
fn loader_dotconfig_dir_takes_precedence() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("project");
    let dotconfig_dir = project_dir.join(".config");
    fs::create_dir_all(&dotconfig_dir).unwrap();

    fs::write(dotconfig_dir.join("test-app.toml"), r#"log_level = "debug""#).unwrap();
    fs::write(
        project_dir.join(".test-app.toml"),
        r#"log_level = "warn""#,
    )
    .unwrap();

    let project_dir = camino::Utf8PathBuf::try_from(project_dir).unwrap();

    let (config, sources): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_boundary_marker()
        .with_project_search(&project_dir)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Debug);
    let found = sources.project_file.unwrap();
    assert!(found.as_str().contains(".config/"));
}

#[test]
fn loader_boundary_marker_stops_search() {
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("parent");
    let child = parent.join("child");
    let work = child.join("work");
    fs::create_dir_all(&work).unwrap();

    fs::write(parent.join(".test-app.toml"), r#"log_level = "warn""#).unwrap();
    fs::create_dir(child.join(".git")).unwrap();

    let work = camino::Utf8PathBuf::try_from(work).unwrap();

    let (config, sources): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .with_boundary_marker(".git")
        .with_project_search(&work)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Info); // default, not parent's warn
    assert!(sources.project_file.is_none());
}

#[test]
fn loader_load_or_error_fails_when_no_config() {
    let result = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .without_boundary_marker()
        .load_or_error::<TestConfig>();

    assert!(matches!(result, Err(rebar::Error::ConfigNotFound)));
}

#[test]
fn loader_yaml_file() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.yaml");
    fs::write(&config_path, "log_level: debug\ncustom_field: hello\n").unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let (config, _): (TestConfig, _) = rebar::config::ConfigLoader::new("test-app")
        .with_user_config(false)
        .with_file(&config_path)
        .load()
        .unwrap();

    assert_eq!(config.log_level, rebar::config::LogLevel::Debug);
    assert_eq!(config.custom_field.as_deref(), Some("hello"));
}
```

- [ ] **Step 2: Run all config tests**

```bash
cargo nextest run --features config -E 'test(config_test)'
```

Expected: All tests PASS (implementation was done in Task 3).

- [ ] **Step 3: Run clippy**

```bash
cargo clippy --features config --all-targets --message-format=short -- -D warnings
```

Expected: No warnings.

- [ ] **Step 4: Commit**

Write `commit.txt`:
```
test(config): add discovery and boundary marker tests

Tests ConfigLoader's directory-walking discovery, dotconfig precedence,
boundary marker behavior, multi-file merging, and YAML format support.
```

---

### Task 5: Logging Module

**Files:**
- Modify: `src/logging.rs`
- Create: `tests/logging_test.rs`

Extracts the observability code from the template. Parameterizes service name and env var prefix.

- [ ] **Step 1: Write failing tests**

Create `tests/logging_test.rs`:

```rust
#![cfg(feature = "logging")]

use rebar::logging;

// ─── env_filter tests ───────────────────────────────────────────────

#[test]
fn env_filter_quiet_overrides() {
    let filter = logging::env_filter(true, 0, "info");
    assert_eq!(filter.to_string(), "error");
}

#[test]
fn env_filter_verbose_debug() {
    let filter = logging::env_filter(false, 1, "info");
    assert_eq!(filter.to_string(), "debug");
}

#[test]
fn env_filter_verbose_trace() {
    let filter = logging::env_filter(false, 2, "info");
    assert_eq!(filter.to_string(), "trace");
}

#[test]
fn env_filter_default_level() {
    let filter = logging::env_filter(false, 0, "warn");
    assert_eq!(filter.to_string(), "warn");
}

// ─── log target resolution tests ────────────────────────────────────

#[test]
fn log_target_from_path_uses_parent() {
    let temp_dir = std::env::temp_dir().join("rebar-test-log-path");
    let file_path = temp_dir.join("custom.jsonl");

    let target =
        logging::resolve_log_target_with("demo", Some(file_path.clone()), None, None).unwrap();
    assert_eq!(target.dir, temp_dir);
    assert_eq!(target.file_name, "custom.jsonl");
}

#[test]
fn log_target_from_dir_appends_service() {
    let temp_dir = std::env::temp_dir().join("rebar-test-log-dir");
    let target =
        logging::resolve_log_target_with("demo", None, Some(temp_dir.clone()), None).unwrap();
    assert_eq!(target.dir, temp_dir);
    assert_eq!(target.file_name, "demo.jsonl");
}

#[test]
fn log_target_path_overrides_dir() {
    let temp_dir = std::env::temp_dir().join("rebar-test-log-override");
    let file_path = temp_dir.join("override.jsonl");

    let target = logging::resolve_log_target_with(
        "demo",
        Some(file_path.clone()),
        Some(std::env::temp_dir()),
        None,
    )
    .unwrap();
    assert_eq!(target.dir, temp_dir);
    assert_eq!(target.file_name, "override.jsonl");
}

// ─── timestamp tests ────────────────────────────────────────────────

#[test]
fn format_timestamp_produces_rfc3339() {
    let ts = logging::format_timestamp();
    assert!(ts.ends_with('Z'), "should end with Z: {ts}");
    assert_eq!(ts.len(), 24, "should be 24 chars: {ts}");
    assert_eq!(&ts[10..11], "T", "date-time separator");
}

// ─── platform log dir tests ─────────────────────────────────────────

#[test]
fn platform_log_dir_contains_service_name() {
    let dir = logging::platform_log_dir("test-svc").expect("should return Some");
    let path = dir.to_str().expect("valid UTF-8");
    assert!(path.contains("test-svc"), "should contain service name: {path}");
}

#[cfg(target_os = "macos")]
#[test]
fn platform_log_dir_uses_library_logs_on_macos() {
    let dir = logging::platform_log_dir("test-svc").unwrap();
    let path = dir.to_str().unwrap();
    assert!(
        path.contains("Library/Logs"),
        "macOS should use ~/Library/Logs/: {path}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features logging -E 'test(logging_test)'
```

Expected: FAIL — logging functions don't exist yet.

- [ ] **Step 3: Implement logging module**

Write `src/logging.rs`. This is the largest single file — it contains the JSON log layer, log target resolution, and timestamp formatting extracted from the template's `observability.rs`.

```rust
//! Structured JSONL logging with daily rotation.
//!
//! Provides a custom tracing Layer that writes JSONL to file with daily
//! rotation. All logging goes to files or stderr — never stdout, which
//! is reserved for application output (e.g., MCP server communication).

use serde_json::{Map, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::Event;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::{Context as LayerContext, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

use crate::error::Result;

const LOG_FILE_SUFFIX: &str = ".jsonl";
const DEFAULT_LOG_DIR_UNIX: &str = "/var/log";

// ─── Public API ─────────────────────────────────────────────────────

/// Configuration for logging setup.
#[derive(Clone, Debug)]
pub struct LoggingConfig {
    /// Service name used in log file names.
    pub service: String,
    /// Env var name for log file path override (e.g., `MYAPP_LOG_PATH`).
    pub env_log_path: String,
    /// Env var name for log directory override (e.g., `MYAPP_LOG_DIR`).
    pub env_log_dir: String,
    /// Directory for JSONL log files (from config). Falls back to platform defaults.
    pub log_dir: Option<PathBuf>,
}

impl LoggingConfig {
    /// Create a logging config from an application name.
    ///
    /// Derives env var names as `{APP}_LOG_PATH` and `{APP}_LOG_DIR`
    /// where `{APP}` is the uppercased, hyphen-to-underscore service name.
    pub fn from_app_name(app_name: &str) -> Self {
        let prefix = app_name.to_uppercase().replace('-', "_");
        Self {
            service: app_name.to_string(),
            env_log_path: format!("{prefix}_LOG_PATH"),
            env_log_dir: format!("{prefix}_LOG_DIR"),
            log_dir: None,
        }
    }

    /// Set the log directory from config.
    pub fn with_log_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.log_dir = dir;
        self
    }
}

/// Guard that must be held for the application lifetime to ensure logs flush.
pub struct LoggingGuard {
    _log_guard: tracing_appender::non_blocking::WorkerGuard,
}

/// Initialize logging and return a guard that must be held.
///
/// # Errors
///
/// Falls back to stderr if no writable log directory is found.
pub fn init(cfg: &LoggingConfig, env_filter: EnvFilter) -> Result<LoggingGuard> {
    let (log_writer, log_guard) =
        match build_log_writer(&cfg.service, &cfg.env_log_path, &cfg.env_log_dir, cfg.log_dir.as_deref()) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("Warning: {err}. Falling back to stderr logging.");
                let (writer, guard) = tracing_appender::non_blocking(std::io::stderr());
                (writer, guard)
            }
        };

    let log_layer = JsonLogLayer::new(log_writer);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(log_layer)
        .init();

    tracing::debug!("logging initialized");

    Ok(LoggingGuard {
        _log_guard: log_guard,
    })
}

/// Build an `EnvFilter` based on CLI flags and environment.
///
/// Priority: quiet flag > verbose flag > `RUST_LOG` env > default_level.
pub fn env_filter(quiet: bool, verbose: u8, default_level: &str) -> EnvFilter {
    if quiet {
        return EnvFilter::new("error");
    }

    if verbose > 0 {
        let level = match verbose {
            1 => "debug",
            _ => "trace",
        };
        return EnvFilter::new(level);
    }

    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level))
}

// ─── Log Target Resolution ──────────────────────────────────────────

/// Resolved log file location.
#[derive(Clone, Debug)]
pub struct LogTarget {
    /// Directory containing the log file.
    pub dir: PathBuf,
    /// Log file name.
    pub file_name: String,
}

/// Resolve log target with explicit overrides (for testing).
///
/// Priority: path_override > dir_override > config_dir > platform default.
pub fn resolve_log_target_with(
    service: &str,
    path_override: Option<PathBuf>,
    dir_override: Option<PathBuf>,
    config_dir: Option<PathBuf>,
) -> std::result::Result<LogTarget, String> {
    if let Some(path) = path_override {
        return log_target_from_path(path);
    }

    if let Some(dir) = dir_override {
        return log_target_from_dir(dir, service);
    }

    if let Some(dir) = config_dir {
        return log_target_from_dir(dir, service);
    }

    let mut candidates = Vec::new();

    if cfg!(unix) {
        candidates.push(PathBuf::from(DEFAULT_LOG_DIR_UNIX));
    }

    if let Some(log_dir) = platform_log_dir(service) {
        candidates.push(log_dir);
    }

    if let Ok(dir) = std::env::current_dir() {
        candidates.push(dir);
    }

    let file_name = format!("{service}{LOG_FILE_SUFFIX}");

    for dir in candidates {
        if ensure_writable(&dir, &file_name).is_ok() {
            return Ok(LogTarget { dir, file_name });
        }
    }

    Err("No writable log directory found".to_string())
}

/// Resolve the platform-appropriate log directory for a service.
///
/// - macOS: `~/Library/Logs/{service}/`
/// - Linux/BSD: `$XDG_STATE_HOME/{service}/logs/`
/// - Windows: `{LocalAppData}/{service}/logs/`
pub fn platform_log_dir(service: &str) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Logs").join(service))
    } else if cfg!(unix) {
        let state_base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local/state"))
            })?;
        Some(state_base.join(service).join("logs"))
    } else {
        directories::ProjectDirs::from("", "", service)
            .map(|p| p.data_local_dir().join("logs"))
    }
}

/// Format timestamp as RFC 3339 using std::time (no external time crate).
pub fn format_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = now.as_secs();
    let nanos = now.subsec_nanos();

    let days_since_epoch = secs / 86400;
    let secs_of_day = secs % 86400;
    let hours = secs_of_day / 3600;
    let minutes = (secs_of_day % 3600) / 60;
    let seconds = secs_of_day % 60;

    let (year, month, day) = days_to_ymd(days_since_epoch as i64);

    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z",
        millis = nanos / 1_000_000
    )
}

/// Convert days since Unix epoch to (year, month, day).
const fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

// ─── Internal ───────────────────────────────────────────────────────

fn build_log_writer(
    service: &str,
    env_log_path: &str,
    env_log_dir: &str,
    config_log_dir: Option<&Path>,
) -> std::result::Result<
    (
        tracing_appender::non_blocking::NonBlocking,
        tracing_appender::non_blocking::WorkerGuard,
    ),
    String,
> {
    let path_override = std::env::var_os(env_log_path).map(PathBuf::from);
    let dir_override = std::env::var_os(env_log_dir).map(PathBuf::from);

    let target = resolve_log_target_with(
        service,
        path_override,
        dir_override,
        config_log_dir.map(PathBuf::from),
    )?;

    let appender = tracing_appender::rolling::daily(&target.dir, &target.file_name);
    let (writer, guard) = tracing_appender::non_blocking(appender);

    Ok((writer, guard))
}

fn log_target_from_dir(dir: PathBuf, service: &str) -> std::result::Result<LogTarget, String> {
    let file_name = format!("{service}{LOG_FILE_SUFFIX}");
    ensure_writable(&dir, &file_name)?;
    Ok(LogTarget { dir, file_name })
}

fn log_target_from_path(path: PathBuf) -> std::result::Result<LogTarget, String> {
    let file_name = path
        .file_name()
        .ok_or("log path must include a file name".to_string())
        .and_then(|name| {
            name.to_str()
                .map(|v| v.to_string())
                .ok_or("log path must be valid UTF-8".to_string())
        })?;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    ensure_writable(dir, &file_name)?;

    Ok(LogTarget {
        dir: dir.to_path_buf(),
        file_name,
    })
}

fn ensure_writable(dir: &Path, file_name: &str) -> std::result::Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create log directory {}: {e}", dir.display()))?;

    let path = dir.join(file_name);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Failed to open log file {}: {e}", path.display()))?;

    Ok(())
}

// ─── JSON Log Layer ─────────────────────────────────────────────────

struct JsonLogLayer<W> {
    writer: W,
}

impl<W> JsonLogLayer<W> {
    const fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<S, W> tracing_subscriber::Layer<S> for JsonLogLayer<W>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: LayerContext<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = JsonVisitor::default();
            attrs.record(&mut visitor);
            span.extensions_mut().insert(SpanFields {
                values: visitor.values,
            });
        }
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: LayerContext<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = JsonVisitor::default();
            values.record(&mut visitor);
            let mut extensions = span.extensions_mut();
            if let Some(fields) = extensions.get_mut::<SpanFields>() {
                fields.values.extend(visitor.values);
            } else {
                extensions.insert(SpanFields {
                    values: visitor.values,
                });
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: LayerContext<'_, S>) {
        let mut map = Map::new();

        let timestamp = format_timestamp();
        map.insert("timestamp".to_string(), Value::String(timestamp));
        map.insert(
            "level".to_string(),
            Value::String(event.metadata().level().as_str().to_lowercase()),
        );
        map.insert(
            "target".to_string(),
            Value::String(event.metadata().target().to_string()),
        );

        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if let Some(fields) = span.extensions().get::<SpanFields>() {
                    map.extend(fields.values.clone());
                }
            }
        }

        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);
        map.extend(visitor.values);

        if let Ok(mut buf) = serde_json::to_vec(&Value::Object(map)) {
            buf.push(b'\n');
            let mut writer = self.writer.make_writer();
            let _ = writer.write_all(&buf);
        }
    }
}

#[derive(Clone, Debug)]
struct SpanFields {
    values: Map<String, Value>,
}

#[derive(Default)]
struct JsonVisitor {
    values: Map<String, Value>,
}

impl tracing::field::Visit for JsonVisitor {
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.values
            .insert(field.name().to_string(), Value::Bool(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.values
            .insert(field.name().to_string(), Value::Number(value.into()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.values
            .insert(field.name().to_string(), Value::Number(value.into()));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        if let Some(number) = serde_json::Number::from_f64(value) {
            self.values
                .insert(field.name().to_string(), Value::Number(number));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.values
            .insert(field.name().to_string(), Value::String(value.to_string()));
    }

    fn record_error(
        &self,
        _field: &tracing::field::Field,
        _value: &(dyn std::error::Error + 'static),
    ) {
        // Intentionally not recording errors in the JSON visitor to avoid
        // lifetime issues. Errors are captured via record_debug.
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.values.insert(
            field.name().to_string(),
            Value::String(format!("{value:?}")),
        );
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features logging -E 'test(logging_test)'
```

Expected: All tests PASS.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features logging --all-targets --message-format=short -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat(logging): add JSONL log layer, log target resolution, and env_filter

Extracts structured logging from claylo-rs template. Parameterizes
service name and env var prefix. Includes daily rotation via
tracing_appender, platform-aware log directory resolution, and
custom JsonLogLayer for JSONL output.
```

---

### Task 6: Builder and App

**Files:**
- Modify: `src/lib.rs`
- Create: `tests/builder_test.rs`

This is the orchestration layer that wires everything together.

- [ ] **Step 1: Write failing builder tests**

Create `tests/builder_test.rs`:

```rust
#![cfg(all(feature = "cli", feature = "config", feature = "logging"))]

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use tempfile::TempDir;

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct TestConfig {
    log_level: rebar::config::LogLevel,
    custom: Option<String>,
}

#[derive(Parser)]
#[command(name = "test-app")]
struct TestCli {
    #[command(flatten)]
    pub common: rebar::cli::CommonArgs,

    #[command(subcommand)]
    pub command: Option<TestCommands>,
}

#[derive(clap::Subcommand)]
enum TestCommands {
    Run,
}

#[test]
fn builder_without_config() {
    let cli = TestCli::parse_from(["test-app", "run"]);

    let app: rebar::App = rebar::init("test-app")
        .with_cli(cli.common)
        .start()
        .unwrap();

    assert!(!app.cli().quiet);
}

#[test]
fn builder_with_config() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    fs::write(&config_path, r#"custom = "hello""#).unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let cli = TestCli::parse_from(["test-app", "run"]);

    let app: rebar::App<TestConfig> = rebar::init("test-app")
        .with_cli(cli.common)
        .config_from_file::<TestConfig>(&config_path)
        .start()
        .unwrap();

    assert_eq!(app.config().custom.as_deref(), Some("hello"));
}

#[test]
fn builder_with_preloaded_config() {
    let config = TestConfig {
        log_level: rebar::config::LogLevel::Debug,
        custom: Some("preloaded".to_string()),
    };
    let cli = TestCli::parse_from(["test-app", "run"]);

    let app = rebar::init("test-app")
        .with_cli(cli.common)
        .with_config(config)
        .start()
        .unwrap();

    assert_eq!(app.config().custom.as_deref(), Some("preloaded"));
}

#[test]
fn app_accessors() {
    let cli = TestCli::parse_from(["test-app", "--quiet", "run"]);

    let app: rebar::App = rebar::init("test-app")
        .with_cli(cli.common)
        .start()
        .unwrap();

    assert!(app.cli().quiet);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features cli,config,logging -E 'test(builder_test)'
```

Expected: FAIL — `rebar::init`, `rebar::App`, builder methods don't exist.

- [ ] **Step 3: Implement builder and App in lib.rs**

Replace `src/lib.rs`:

```rust
//! Rebar: Rust application foundation crate.
//!
//! Feature-gated modules for CLI, config, logging, and more.
//! Use the builder to orchestrate initialization:
//!
//! ```ignore
//! let app = rebar::init(env!("CARGO_PKG_NAME"))
//!     .with_cli(cli.common)
//!     .config::<Config>()
//!     .logging()
//!     .start()?;
//! ```
#![deny(unsafe_code)]

pub mod error;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "config")]
pub mod config;

#[cfg(feature = "logging")]
pub mod logging;

pub use error::{Error, Result};

use serde::Serialize;

/// The initialized application state.
///
/// Holds config, CLI args, and guards for logging/tracing.
/// `C` is the user's config type (defaults to `()` when config is not used).
pub struct App<C = ()> {
    app_name: String,
    config: C,
    #[cfg(feature = "config")]
    config_sources: config::ConfigSources,
    #[cfg(feature = "cli")]
    cli: cli::CommonArgs,
    #[cfg(feature = "logging")]
    _logging_guard: Option<logging::LoggingGuard>,
}

impl<C> App<C> {
    /// Returns a reference to the loaded configuration.
    pub fn config(&self) -> &C {
        &self.config
    }

    /// Returns the application name.
    pub fn app_name(&self) -> &str {
        &self.app_name
    }
}

#[cfg(feature = "config")]
impl<C> App<C> {
    /// Returns metadata about which config files were loaded.
    pub fn config_sources(&self) -> &config::ConfigSources {
        &self.config_sources
    }
}

#[cfg(feature = "cli")]
impl<C> App<C> {
    /// Returns the parsed common CLI arguments.
    pub fn cli(&self) -> &cli::CommonArgs {
        &self.cli
    }
}

/// Start building a rebar application.
///
/// ```ignore
/// let app = rebar::init(env!("CARGO_PKG_NAME"))
///     .with_cli(cli.common)
///     .config::<Config>()
///     .logging()
///     .start()?;
/// ```
pub fn init(app_name: &str) -> Builder {
    Builder {
        app_name: app_name.to_string(),
        #[cfg(feature = "cli")]
        cli: None,
        #[cfg(feature = "config")]
        config_source: ConfigSource::None,
        #[cfg(feature = "logging")]
        enable_logging: false,
    }
}

#[cfg(feature = "config")]
enum ConfigSource {
    None,
    Discover,
    File(camino::Utf8PathBuf),
    Preloaded(serde_json::Value),
}

/// Builder for rebar application initialization.
pub struct Builder {
    app_name: String,
    #[cfg(feature = "cli")]
    cli: Option<cli::CommonArgs>,
    #[cfg(feature = "config")]
    config_source: ConfigSource,
    #[cfg(feature = "logging")]
    enable_logging: bool,
}

impl Builder {
    /// Provide parsed CLI common arguments.
    #[cfg(feature = "cli")]
    pub fn with_cli(mut self, common: cli::CommonArgs) -> Self {
        self.cli = Some(common);
        self
    }

    /// Enable config discovery from standard locations.
    #[cfg(feature = "config")]
    pub fn config<C>(mut self) -> Self {
        self.config_source = ConfigSource::Discover;
        self
    }

    /// Load config from a specific file.
    #[cfg(feature = "config")]
    pub fn config_from_file<C>(mut self, path: &camino::Utf8Path) -> Self {
        self.config_source = ConfigSource::File(path.to_path_buf());
        self
    }

    /// Enable JSONL logging.
    #[cfg(feature = "logging")]
    pub fn logging(mut self) -> Self {
        self.enable_logging = true;
        self
    }

    /// Finalize initialization without config.
    ///
    /// Use this when the `config` feature is not enabled or when
    /// no config is needed.
    pub fn start(self) -> Result<App> {
        #[cfg(feature = "logging")]
        let logging_guard = if self.enable_logging {
            let (quiet, verbose) = self.cli_verbosity();
            let log_cfg = logging::LoggingConfig::from_app_name(&self.app_name);
            let filter = logging::env_filter(quiet, verbose, "info");
            Some(logging::init(&log_cfg, filter)?)
        } else {
            None
        };

        Ok(App {
            app_name: self.app_name,
            config: (),
            #[cfg(feature = "config")]
            config_sources: config::ConfigSources::default(),
            #[cfg(feature = "cli")]
            cli: self.cli.unwrap_or_else(|| {
                // If no CLI was provided, use defaults
                cli::CommonArgs {
                    version_only: false,
                    chdir: None,
                    quiet: false,
                    verbose: 0,
                    color: cli::ColorChoice::Auto,
                    json: false,
                }
            }),
            #[cfg(feature = "logging")]
            _logging_guard: logging_guard,
        })
    }

    #[cfg(feature = "logging")]
    fn cli_verbosity(&self) -> (bool, u8) {
        #[cfg(feature = "cli")]
        if let Some(ref cli) = self.cli {
            return (cli.quiet, cli.verbose);
        }
        (false, 0)
    }
}

#[cfg(feature = "config")]
impl Builder {
    /// Provide a pre-loaded config (escape hatch).
    pub fn with_config<C: Serialize>(mut self, config: C) -> ConfiguredBuilder<C> {
        let value = serde_json::to_value(&config).expect("config must be serializable");
        ConfiguredBuilder {
            inner: self,
            config,
            sources: config::ConfigSources::default(),
        }
    }

    /// Finalize initialization with config discovery.
    pub fn start_with_config<C: serde::de::DeserializeOwned + Default + Serialize>(
        self,
    ) -> Result<App<C>> {
        let (config, sources) = match &self.config_source {
            ConfigSource::None => {
                let config = C::default();
                (config, config::ConfigSources::default())
            }
            ConfigSource::Discover => {
                let cwd = std::env::current_dir()
                    .map_err(crate::Error::Io)?;
                let cwd = camino::Utf8PathBuf::try_from(cwd)
                    .map_err(|e| crate::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("current directory is not valid UTF-8: {}", e.into_path_buf().display()),
                    )))?;
                config::ConfigLoader::new(&self.app_name)
                    .with_project_search(&cwd)
                    .load::<C>()?
            }
            ConfigSource::File(path) => config::ConfigLoader::new(&self.app_name)
                .with_user_config(false)
                .with_file(path)
                .load::<C>()?,
            ConfigSource::Preloaded(value) => {
                let config: C = serde_json::from_value(value.clone())
                    .map_err(|e| Error::ConfigDeserialize(Box::new(e)))?;
                (config, config::ConfigSources::default())
            }
        };

        #[cfg(feature = "logging")]
        let logging_guard = if self.enable_logging {
            let (quiet, verbose) = self.cli_verbosity();
            let log_cfg = logging::LoggingConfig::from_app_name(&self.app_name);
            let filter = logging::env_filter(quiet, verbose, "info");
            Some(logging::init(&log_cfg, filter)?)
        } else {
            None
        };

        Ok(App {
            app_name: self.app_name,
            config,
            config_sources: sources,
            #[cfg(feature = "cli")]
            cli: self.cli.unwrap_or_else(|| cli::CommonArgs {
                version_only: false,
                chdir: None,
                quiet: false,
                verbose: 0,
                color: cli::ColorChoice::Auto,
                json: false,
            }),
            #[cfg(feature = "logging")]
            _logging_guard: logging_guard,
        })
    }
}

/// Builder with a pre-loaded config (escape hatch path).
#[cfg(feature = "config")]
pub struct ConfiguredBuilder<C> {
    inner: Builder,
    config: C,
    sources: config::ConfigSources,
}

#[cfg(feature = "config")]
impl<C: Serialize> ConfiguredBuilder<C> {
    /// Enable JSONL logging.
    #[cfg(feature = "logging")]
    pub fn logging(mut self) -> Self {
        self.inner.enable_logging = true;
        self
    }

    /// Finalize initialization with the pre-loaded config.
    pub fn start(self) -> Result<App<C>> {
        #[cfg(feature = "logging")]
        let logging_guard = if self.inner.enable_logging {
            let (quiet, verbose) = self.inner.cli_verbosity();
            let log_cfg = logging::LoggingConfig::from_app_name(&self.inner.app_name);
            let filter = logging::env_filter(quiet, verbose, "info");
            Some(logging::init(&log_cfg, filter)?)
        } else {
            None
        };

        Ok(App {
            app_name: self.inner.app_name,
            config: self.config,
            config_sources: self.sources,
            #[cfg(feature = "cli")]
            cli: self.inner.cli.unwrap_or_else(|| cli::CommonArgs {
                version_only: false,
                chdir: None,
                quiet: false,
                verbose: 0,
                color: cli::ColorChoice::Auto,
                json: false,
            }),
            #[cfg(feature = "logging")]
            _logging_guard: logging_guard,
        })
    }
}
```

Note: The builder API has two terminal methods:
- `start()` — returns `App<()>` (no config)
- `start_with_config::<C>()` — returns `App<C>` (with config discovery/loading)
- `with_config(c).start()` — returns `App<C>` (pre-loaded config escape hatch)

The `config::<C>()` method is a type hint that doesn't immediately load — loading happens in `start_with_config()`. This may need refinement during implementation to find the most ergonomic API.

- [ ] **Step 4: Update builder tests to match actual API**

The builder API may require adjustments during implementation (e.g., `config::<C>()` might not carry the type through cleanly). Update `tests/builder_test.rs` to match the implemented API. The key behaviors to test remain:
- Builder without config returns `App<()>`
- Builder with config file returns `App<TestConfig>` with loaded values
- Builder with pre-loaded config returns `App<TestConfig>` with those values
- CLI accessors work

- [ ] **Step 5: Run all tests**

```bash
cargo nextest run --features cli,config,logging
```

Expected: All tests PASS across all test files.

- [ ] **Step 6: Run clippy with all Phase 1 features**

```bash
cargo clippy --features cli,config,logging --all-targets --message-format=short -- -D warnings
```

Expected: No warnings.

- [ ] **Step 7: Commit**

Write `commit.txt`:
```
feat: add builder and App orchestration layer

The init() builder wires config discovery, logging setup, and CLI
args in the correct order. App<C> holds initialized state and
provides accessors. Supports escape hatches for pre-loaded config
and direct module usage.
```

---

### Task 7: Feature Isolation Verification

**Files:** None (verification only)

Verify that each feature compiles and tests pass in isolation, and that the crate compiles with no features at all.

- [ ] **Step 1: Verify no-feature compilation**

```bash
cargo check --no-default-features
```

Expected: PASS — lib.rs with just `error` module.

- [ ] **Step 2: Verify each feature in isolation**

```bash
cargo check --features cli
cargo check --features config
cargo check --features logging
```

Expected: All PASS — no cross-feature dependencies in Phase 1.

- [ ] **Step 3: Verify all features together**

```bash
cargo clippy --features cli,config,logging --all-targets --message-format=short -- -D warnings
```

Expected: PASS, no warnings.

- [ ] **Step 4: Run full test suite**

```bash
cargo nextest run --features cli,config,logging
```

Expected: All tests PASS.

- [ ] **Step 5: Run cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 6: Final commit**

Write `commit.txt`:
```
chore: verify feature isolation and run full test suite

All features compile in isolation and together. Full test suite passes.
```

---

## Implementation Notes

### API Refinement Expected

The builder's type-state transitions (`Builder` → `ConfiguredBuilder<C>`) and the dual terminal methods (`start()` vs `start_with_config::<C>()`) are a first attempt. The implementing agent should refine the API to be as ergonomic as possible while maintaining type safety. The spec examples show the ideal API:

```rust
let app = rebar::init("myapp")
    .with_cli(cli.common)
    .config::<Config>()
    .logging()
    .start()?;
```

If this exact signature can be achieved cleanly with Rust's type system, prefer it over the split `start()`/`start_with_config()` approach.

### serde-saphyr Version

The `serde-saphyr` crate is at version `0.0`. Verify the exact current version on crates.io before writing the Cargo.toml.

### Logging Initialization Is Once-Only

`tracing_subscriber::registry().init()` can only be called once per process. The builder tests that call `logging()` will conflict if run in the same process. Use `cargo nextest run` (which runs each test in its own process) or gate logging tests to avoid double-init panics.

### record_error in JsonVisitor

The template's `record_error` implementation stores the error as a string. The implementation above uses a no-op for `record_error` because errors are also captured via `record_debug`. The implementing agent should verify this matches the desired behavior and adjust if needed.
