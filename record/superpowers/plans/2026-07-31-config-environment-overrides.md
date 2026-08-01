# Configuration Environment Overrides Implementation Plan

**Status:** Implemented

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Do not create a worktree.

**Goal:** Add typed `{APP}_{FIELD}` environment overlays with `__` nesting,
programmatic CLI overrides, source provenance, and the adjacent config/CLI
corrections accepted in the design.

**Architecture:** Keep `ConfigLoader` as the orchestrator. A focused
`src/config/environment.rs` module owns environment access, prefix/path
normalization, schema-driven parsing, unknown-path policy, and sparse overlay
construction. `ConfigLoader` merges that overlay after every file and applies
typed programmatic overrides last.

**Tech Stack:** Rust 2024, Serde, `serde_json`, Clap 4.6, existing librebar
errors and integration tests. No new dependencies.

**Design:**
`record/superpowers/specs/2026-07-31-config-environment-overrides.md`

---

## File map

| File | Responsibility |
|---|---|
| `src/config/environment.rs` | Environment source interface, process source, unknown policy, prefix/path parsing, schema coercion, overlay insertion |
| `src/config.rs` | Loader orchestration, source metadata, programmatic overrides, shared `load`/`load_or_error` path, `LogLevel::Trace` |
| `src/error.rs` | Contextual environment and programmatic override errors |
| `src/lib.rs` | Configured-builder override plumbing and stale crate docs |
| `src/cli.rs` | Correct `CommonArgs` derive and `Clone` |
| `tests/config_test.rs` | Environment behavior, precedence, failures, provenance, loader API |
| `tests/builder_test.rs` | Programmatic override integration through `ConfiguredBuilder` |
| `tests/cli_test.rs` | Consumer compile contract for `Args + Clone` |
| `README.md` | Naming, precedence, strict booleans, empty strings, unknowns, shell quoting |

---

### Task 1: Pin the adjacent type corrections

**Files:**

- Modify: `tests/config_test.rs`
- Modify: `tests/cli_test.rs`
- Modify: `src/config.rs`
- Modify: `src/cli.rs`

- [ ] **Step 1: Write failing tests for `Trace` and `CommonArgs` traits**

Add to `tests/config_test.rs`:

```rust
#[test]
fn log_level_trace_round_trips() {
    let level: librebar::config::LogLevel = serde_json::from_str(r#""trace""#).unwrap();
    assert_eq!(level, librebar::config::LogLevel::Trace);
    assert_eq!(level.as_str(), "trace");
}
```

Add to `tests/cli_test.rs`:

```rust
fn assert_args<T: clap::Args>() {}

#[test]
fn common_args_is_cloneable_flattened_args() {
    assert_args::<librebar::cli::CommonArgs>();
    let cli = TestCli::parse_from(["test-app", "info"]);
    let copy = cli.common.clone();
    assert_eq!(copy.verbose, cli.common.verbose);
}
```

- [ ] **Step 2: Run both tests and verify RED**

Run:

```bash
cargo test --all-features --test config_test log_level_trace_round_trips -- --exact
cargo test --all-features --test cli_test common_args_is_cloneable_flattened_args -- --exact
```

Expected: the config test fails because `LogLevel::Trace` does not exist; the
CLI test fails because `CommonArgs` does not implement `Clone`.

- [ ] **Step 3: Add the minimum implementations**

Change `LogLevel` in `src/config.rs`:

```rust
pub enum LogLevel {
    /// Maximum diagnostic detail.
    Trace,
    /// Verbose output for debugging and development.
    Debug,
    // existing variants remain unchanged
}

pub const fn as_str(&self) -> &'static str {
    match self {
        Self::Trace => "trace",
        Self::Debug => "debug",
        Self::Info => "info",
        Self::Warn => "warn",
        Self::Error => "error",
    }
}
```

Change `CommonArgs` in `src/cli.rs` while retaining the command attributes:

```rust
use clap::Args;

#[derive(Args, Clone, Debug)]
#[command(about = None, long_about = None)]
pub struct CommonArgs {
    // fields unchanged
}
```

- [ ] **Step 4: Run the targeted tests and existing help regressions**

Run:

```bash
cargo test --all-features --test config_test log_level_trace_round_trips -- --exact
cargo test --all-features --test cli_test
```

Expected: PASS, including both help-text regression tests.

- [ ] **Step 5: Commit checkpoint (Clay-owned)**

Suggested message: `fix(config): complete shared config and CLI types`

---

### Task 2: Introduce the environment source and path policy

**Files:**

- Create: `src/config/environment.rs`
- Modify: `src/config.rs`
- Modify: `src/error.rs`
- Modify: `tests/config_test.rs`

- [ ] **Step 1: Add a fixed source and failing flat/nested API tests**

Add imports and the test source to `tests/config_test.rs`:

```rust
use std::ffi::OsString;

#[derive(Debug)]
struct FixedEnvironment(Vec<(OsString, OsString)>);

impl FixedEnvironment {
    fn new(values: &[(&str, &str)]) -> Self {
        Self(
            values
                .iter()
                .map(|(key, value)| (OsString::from(key), OsString::from(value)))
                .collect(),
        )
    }
}

impl librebar::config::EnvironmentSource for FixedEnvironment {
    fn vars(&self) -> Vec<(OsString, OsString)> {
        self.0.clone()
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
struct EnvironmentConfig {
    database_url: String,
    database: DatabaseConfig,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
struct DatabaseConfig {
    url: String,
}

fn loader_with(values: &[(&str, &str)]) -> librebar::config::ConfigLoader {
    librebar::config::ConfigLoader::new("my-app")
        .with_user_config(false)
        .without_boundary_marker()
        .with_environment_source(FixedEnvironment::new(values))
}
```

Add tests:

```rust
#[test]
fn environment_uses_normalized_prefix_and_preserves_single_underscores() {
    let (config, sources): (EnvironmentConfig, _) =
        librebar::config::ConfigLoader::new("my-app")
            .with_user_config(false)
            .with_environment_source(FixedEnvironment::new(&[(
                "MY_APP_DATABASE_URL",
                "postgres://flat",
            )]))
            .load()
            .unwrap();

    assert_eq!(config.database_url, "postgres://flat");
    assert_eq!(sources.environment_variables, ["MY_APP_DATABASE_URL"]);
}

#[test]
fn environment_double_underscore_addresses_nested_fields() {
    let (config, _): (EnvironmentConfig, _) =
        librebar::config::ConfigLoader::new("my-app")
            .with_user_config(false)
            .with_environment_source(FixedEnvironment::new(&[(
                "MY_APP_DATABASE__URL",
                "postgres://nested",
            )]))
            .load()
            .unwrap();

    assert_eq!(config.database.url, "postgres://nested");
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cargo test --all-features --test config_test environment_ -- --nocapture
```

Expected: compile failure because the environment source API and provenance
field do not exist.

- [ ] **Step 3: Define the public environment boundary**

Create `src/config/environment.rs` with these public types:

```rust
use std::ffi::OsString;

/// Source of process-style configuration variables.
pub trait EnvironmentSource: std::fmt::Debug {
    /// Return environment key/value pairs.
    fn vars(&self) -> Vec<(OsString, OsString)>;
}

/// The current process environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessEnvironment;

impl EnvironmentSource for ProcessEnvironment {
    fn vars(&self) -> Vec<(OsString, OsString)> {
        std::env::vars_os().collect()
    }
}

/// Policy for prefixed variables whose paths are absent from lower layers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnknownEnvironment {
    /// Ignore unknown paths without decoding their values.
    #[default]
    Ignore,
    /// Insert unknown paths as strings.
    Collect,
}
```

In `src/config.rs`, add and re-export the module:

```rust
mod environment;
pub use environment::{EnvironmentSource, ProcessEnvironment, UnknownEnvironment};
```

Extend `ConfigSources`:

```rust
/// Applied environment variable names. Values are never recorded.
#[serde(skip_serializing_if = "Vec::is_empty")]
pub environment_variables: Vec<String>,

/// Applied programmatic override paths.
#[serde(skip_serializing_if = "Vec::is_empty")]
pub override_paths: Vec<String>,
```

Extend `ConfigLoader` and initialize the fields in `new`:

```rust
environment_source: Option<Box<dyn EnvironmentSource>>,
unknown_environment: UnknownEnvironment,
overrides: Vec<ConfigOverride>,
```

```rust
environment_source: Some(Box::new(ProcessEnvironment)),
unknown_environment: UnknownEnvironment::Ignore,
overrides: Vec::new(),
```

Add builder methods:

```rust
pub fn with_environment_source<E>(mut self, source: E) -> Self
where
    E: EnvironmentSource + 'static,
{
    self.environment_source = Some(Box::new(source));
    self
}

pub fn without_environment(mut self) -> Self {
    self.environment_source = None;
    self
}

pub const fn with_unknown_environment(mut self, policy: UnknownEnvironment) -> Self {
    self.unknown_environment = policy;
    self
}
```

Replace derived `Default` on `ConfigLoader` with a manual implementation that
calls `Self::new("")`; this keeps process-environment behavior consistent.

- [ ] **Step 4: Add the contextual error variants**

Add to `src/error.rs`:

```rust
/// Environment configuration could not be applied.
#[cfg(feature = "config")]
#[error("invalid environment configuration from {variable}: {reason}")]
ConfigEnvironment {
    /// Variable that failed validation or parsing.
    variable: String,
    /// Safe diagnostic that never includes the variable's value.
    reason: String,
},

/// A programmatic configuration override is invalid.
#[cfg(feature = "config")]
#[error("invalid configuration override {path}: {reason}")]
ConfigOverride {
    /// Dotted configuration path.
    path: String,
    /// Serialization or path error.
    reason: String,
},
```

- [ ] **Step 5: Implement prefix/path normalization and string overlays**

In `src/config/environment.rs`, add internal helpers with these contracts:

```rust
pub(crate) const ENV_PATH_DEPTH_LIMIT: usize = 64;

fn prefix(app_name: &str) -> String {
    let mut result: String = app_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    result.push('_');
    result
}

fn path(variable: &str, prefix: &str) -> crate::Result<Option<Vec<String>>> {
    let Some(suffix) = variable.strip_prefix(prefix) else {
        return Ok(None);
    };
    let parts: Vec<String> = suffix.split("__").map(str::to_ascii_lowercase).collect();
    if parts.is_empty() || parts.iter().any(String::is_empty) {
        return Err(environment_error(variable, "environment path contains an empty segment"));
    }
    if parts.len() > ENV_PATH_DEPTH_LIMIT {
        return Err(environment_error(variable, "environment path exceeds 64 levels"));
    }
    Ok(Some(parts))
}
```

Add `schema_at`, conflict-checking `insert`, and
`pub(crate) fn overlay(...) -> Result<(Value, Vec<String>)>`. Sort Unicode
prefix matches by variable name, discard unknown paths before decoding values,
and initially insert known/null values as strings. `insert` must reject an
existing leaf and any scalar/object parent-child collision.

- [ ] **Step 6: Merge the environment overlay before explicit files**

In the loader path, call:

```rust
if let Some(source) = self.environment_source.as_deref() {
    let (overlay, variables) = environment::overlay(
        &self.app_name,
        &merged,
        source,
        self.unknown_environment,
    )?;
    deep_merge(&mut merged, overlay)?;
    sources.environment_variables = variables;
}
```

- [ ] **Step 7: Run the flat/nested tests and verify GREEN**

Run:

```bash
cargo test --all-features --test config_test environment_ -- --nocapture
```

Expected: both new tests pass.

- [ ] **Step 8: Commit checkpoint (Clay-owned)**

Suggested message: `feat(config): add environment source and nested paths`

---

### Task 3: Implement schema parsing, unknown policy, empty strings, and errors

**Files:**

- Modify: `src/config/environment.rs`
- Modify: `tests/config_test.rs`

- [ ] **Step 1: Expand the fixture schema**

Add fields to `EnvironmentConfig`:

```rust
enabled: bool,
port: u16,
ratio: f64,
tags: Vec<String>,
metadata: serde_json::Value,
level: librebar::config::LogLevel,
build_id: Option<String>,
optional_port: Option<u16>,
```

Give `port` and `ratio` non-zero defaults with a manual `Default`
implementation so their serialized values establish numeric schema.

Use this complete default shape:

```rust
impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            database_url: String::new(),
            database: DatabaseConfig::default(),
            enabled: false,
            port: 8080,
            ratio: 1.0,
            tags: Vec::new(),
            metadata: serde_json::json!({}),
            level: librebar::config::LogLevel::Info,
            build_id: None,
            optional_port: None,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct StrictEnvironmentConfig {
    known: String,
}
```

- [ ] **Step 2: Write failing tests for strict, typed parsing**

Add one test table covering:

```rust
#[test]
fn environment_parses_values_from_lower_layer_schema() {
    let source = FixedEnvironment::new(&[
        ("MY_APP_ENABLED", "true"),
        ("MY_APP_PORT", "9090"),
        ("MY_APP_RATIO", "1.25"),
        ("MY_APP_TAGS", r#"["worker","blue"]"#),
        ("MY_APP_METADATA", r#"{"region":"iad"}"#),
        ("MY_APP_LEVEL", "trace"),
    ]);

    let (config, _): (EnvironmentConfig, _) =
        librebar::config::ConfigLoader::new("my-app")
            .with_user_config(false)
            .with_environment_source(source)
            .load()
            .unwrap();

    assert!(config.enabled);
    assert_eq!(config.port, 9090);
    assert_eq!(config.ratio, 1.25);
    assert_eq!(config.tags, ["worker", "blue"]);
    assert_eq!(config.metadata["region"], "iad");
    assert_eq!(config.level, librebar::config::LogLevel::Trace);
}
```

Add a loop asserting `1`, `0`, `yes`, `no`, `TRUE`, and `FALSE` each return
`Error::ConfigEnvironment`, and `true`/`false` pass.

- [ ] **Step 3: Write failing tests for empty and null-schema values**

```rust
#[test]
fn empty_environment_string_is_a_value() {
    let (config, _): (EnvironmentConfig, _) =
        loader_with(&[("MY_APP_BUILD_ID", "")]).load().unwrap();
    assert_eq!(config.build_id.as_deref(), Some(""));
}

#[test]
fn empty_non_string_environment_value_is_a_parse_error() {
    let err = loader_with(&[("MY_APP_PORT", "")])
        .load::<EnvironmentConfig>()
        .unwrap_err();
    assert!(matches!(err, librebar::Error::ConfigEnvironment { .. }));
}

#[test]
fn null_schema_values_remain_strings() {
    let (config, _): (EnvironmentConfig, _) =
        loader_with(&[("MY_APP_BUILD_ID", "00123")]).load().unwrap();
    assert_eq!(config.build_id.as_deref(), Some("00123"));
}

#[test]
fn a_file_value_supplies_schema_for_an_optional_number() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("config.toml");
    fs::write(&file, "optional_port = 7000\n").unwrap();
    let file = camino::Utf8PathBuf::try_from(file).unwrap();

    let (config, _): (EnvironmentConfig, _) = loader_with(&[(
        "MY_APP_OPTIONAL_PORT",
        "8000",
    )])
    .with_file(file)
    .load()
    .unwrap();

    assert_eq!(config.optional_port, Some(8000));
}
```

- [ ] **Step 4: Write failing tests for unknown-path policy and ordering**

Use a `#[serde(deny_unknown_fields)]` fixture. Verify:

```rust
#[test]
fn unknown_prefixed_variables_are_ignored_by_default() {
    let (config, sources): (EnvironmentConfig, _) =
        loader_with(&[("MY_APP_TYPO_FIELD", "ignored")]).load().unwrap();
    assert_eq!(config, EnvironmentConfig::default());
    assert!(sources.environment_variables.is_empty());
}

#[test]
fn unknown_prefixed_variables_can_be_collected() {
    let err = loader_with(&[("MY_APP_TYPO_FIELD", "collected")])
        .with_unknown_environment(librebar::config::UnknownEnvironment::Collect)
        .load::<StrictEnvironmentConfig>()
        .unwrap_err();
    assert!(matches!(err, librebar::Error::ConfigDeserialize(_)));
}
```

On Unix, add a non-UTF-8 value to an ignored unknown variable and verify it is
discarded without error. Add the same value to a known variable and verify
`ConfigEnvironment` names the key without including the value.

- [ ] **Step 5: Write failing conflict and path-limit tests**

Cover duplicate normalized paths, known parent/child conflicts, empty segments,
and 65 nested segments. Assert every error is `ConfigEnvironment` and names the
current variable.

- [ ] **Step 6: Run the new tests and verify RED**

Run:

```bash
cargo test --all-features --test config_test environment_ -- --nocapture
```

Expected: typed, policy, and validation cases fail against the string-only
overlay.

- [ ] **Step 7: Implement schema-driven coercion**

Add to `src/config/environment.rs`:

```rust
fn coerce(variable: &str, raw: String, schema: Option<&Value>) -> crate::Result<Value> {
    match schema {
        Some(Value::Bool(_)) => match raw.as_str() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(environment_error(variable, "expected `true` or `false`")),
        },
        Some(Value::Number(number)) if number.is_i64() => raw
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| environment_error(variable, "expected a signed integer")),
        Some(Value::Number(number)) if number.is_u64() => raw
            .parse::<u64>()
            .map(Value::from)
            .map_err(|_| environment_error(variable, "expected an unsigned integer")),
        Some(Value::Number(_)) => raw
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| environment_error(variable, "expected a finite number")),
        Some(Value::Array(_)) => parse_compound(variable, &raw, "array", Value::is_array),
        Some(Value::Object(_)) => parse_compound(variable, &raw, "object", Value::is_object),
        Some(Value::String(_) | Value::Null) | None => Ok(Value::String(raw)),
    }
}
```

`parse_compound` must parse with `serde_json::from_str`, verify the expected
JSON kind, and map failures to `ConfigEnvironment` without echoing `raw`.

Perform known-path filtering before `OsString::into_string`, then call
`coerce`. Record only variables that reach insertion. This ordering makes the
unknown/non-UTF-8 tests pass.

- [ ] **Step 8: Run the config environment suite and verify GREEN**

Run:

```bash
cargo test --all-features --test config_test environment_ -- --nocapture
```

Expected: all environment parsing and validation tests pass.

- [ ] **Step 9: Commit checkpoint (Clay-owned)**

Suggested message: `feat(config): parse typed environment overlays`

---

### Task 4: Complete precedence, provenance, and `load_or_error`

**Files:**

- Modify: `src/config.rs`
- Modify: `tests/config_test.rs`

- [ ] **Step 1: Write failing precedence tests**

Create one temp explicit TOML file and a fixed environment source. Verify the
file beats defaults and environment, then verify a later programmatic override
beats the explicit file:

```rust
#[test]
fn explicit_file_overrides_environment() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("config.toml");
    fs::write(&file, "port = 7000\n").unwrap();
    let file = camino::Utf8PathBuf::try_from(file).unwrap();

    let (config, sources): (EnvironmentConfig, _) =
        librebar::config::ConfigLoader::new("my-app")
            .with_user_config(false)
            .with_file(file)
            .with_environment_source(FixedEnvironment::new(&[("MY_APP_PORT", "8000")]))
            .load()
            .unwrap();

    assert_eq!(config.port, 7000);
    assert_eq!(sources.environment_variables, ["MY_APP_PORT"]);
}
```

Add a second test with `.with_override("port", 9000_u16)` and verify the final
port is `9000`.

- [ ] **Step 2: Write failing `load_or_error` source tests**

Verify the consuming method succeeds with only a known environment variable,
succeeds with only an override, and returns `ConfigNotFound` with an empty fixed
environment and no files or overrides.

- [ ] **Step 3: Run the tests and verify RED**

Run:

```bash
cargo test --all-features --test config_test explicit_file_overrides_environment -- --exact
cargo test --all-features --test config_test load_or_error_ -- --nocapture
```

Expected: compile failures for `with_override`, plus the old borrowed
`load_or_error` behavior.

- [ ] **Step 4: Add serialized programmatic overrides**

In `src/config.rs`, define:

```rust
#[derive(Debug)]
pub(crate) struct ConfigOverride {
    path: String,
    value: std::result::Result<Value, serde_json::Error>,
}

impl ConfigOverride {
    pub(crate) fn new<V>(path: String, value: V) -> Self
    where
        V: Serialize,
    {
        Self {
            path,
            value: serde_json::to_value(value),
        }
    }
}
```

Add the public loader method:

```rust
pub fn with_override<V>(mut self, path: impl Into<String>, value: V) -> Self
where
    V: Serialize,
{
    self.overrides
        .push(ConfigOverride::new(path.into(), value));
    self
}
```

Add `pub(crate) fn with_serialized_override` for `ConfiguredBuilder` to transfer
an already-created `ConfigOverride` without serializing twice.

Implement dotted path validation and insertion. Reject empty segments and more
than 64 segments with `ConfigOverride`. Create object parents when a lower layer
is null or missing. Apply overrides in call order and record each applied path.

Add a `FailingSerialize` test type whose `Serialize` implementation returns
`serde::ser::Error::custom("nope")`. Verify loading returns
`Error::ConfigOverride`, names the dotted path, and does not include a value.
Also test an empty dotted segment and a 65-segment path.

- [ ] **Step 5: Refactor to one consuming loader path**

Replace the current cloned-loader implementation with:

```rust
pub fn load<C>(self) -> Result<(C, ConfigSources)>
where
    C: serde::de::DeserializeOwned + Default + Serialize,
{
    self.load_inner(false)
}

pub fn load_or_error<C>(self) -> Result<(C, ConfigSources)>
where
    C: serde::de::DeserializeOwned + Default + Serialize,
{
    self.load_inner(true)
}
```

`load_inner` performs all layers once, checks `ConfigSources::is_empty()` after
environment and overrides, then deserializes. `is_empty` returns true only when
all file fields, `environment_variables`, and `override_paths` are empty.

- [ ] **Step 6: Update existing config tests to disable the real environment**

Add `.without_environment()` to existing `ConfigLoader` tests that are not
testing environment behavior. This prevents host variables from influencing
CI while leaving production defaults unchanged.

- [ ] **Step 7: Run the full config test target**

Run:

```bash
cargo test --all-features --test config_test
```

Expected: PASS.

- [ ] **Step 8: Commit checkpoint (Clay-owned)**

Suggested message: `feat(config): complete override precedence and provenance`

---

### Task 5: Expose CLI overrides through `ConfiguredBuilder`

**Files:**

- Modify: `src/lib.rs`
- Modify: `tests/builder_test.rs`

- [ ] **Step 1: Write a failing builder override test**

Add to `tests/builder_test.rs`:

```rust
#[test]
fn configured_builder_applies_programmatic_override_last() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    fs::write(&config_path, r#"custom = "file""#).unwrap();
    let config_path = camino::Utf8PathBuf::try_from(config_path).unwrap();

    let app = librebar::init("test-app")
        .config_from_file::<TestConfig>(&config_path)
        .with_config_override("custom", "cli")
        .start()
        .unwrap();

    assert_eq!(app.config().custom.as_deref(), Some("cli"));
    assert_eq!(app.config_sources().override_paths, ["custom"]);
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test --all-features --test builder_test configured_builder_applies_programmatic_override_last -- --exact
```

Expected: compile failure because `with_config_override` does not exist.

- [ ] **Step 3: Add override storage to `ConfiguredBuilder`**

Add `config_overrides: Vec<config::ConfigOverride>` to `ConfiguredBuilder<C>`
and initialize it in `config`, `config_from_file`, and `with_config`.

Add:

```rust
pub fn with_config_override<V>(mut self, path: impl Into<String>, value: V) -> Self
where
    V: serde::Serialize,
{
    self.config_overrides
        .push(config::ConfigOverride::new(path.into(), value));
    self
}
```

For discovered and explicit-file sources, transfer the overrides to the loader
before `load`. For `CfgSource::Preloaded`, serialize the preloaded value, apply
only the explicit programmatic overrides, deserialize it again, and return
override provenance. Do not apply process environment to a preloaded escape
hatch.

- [ ] **Step 4: Run builder tests and verify GREEN**

Run:

```bash
cargo test --all-features --test builder_test
```

Expected: PASS.

- [ ] **Step 5: Commit checkpoint (Clay-owned)**

Suggested message: `feat(builder): expose typed config overrides`

---

### Task 6: Update public documentation

**Files:**

- Modify: `README.md`
- Modify: `src/config.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Update merge-order documentation**

In `src/config.rs` and README, document:

```text
defaults
  < user config
  < project config
  < environment
  < explicit files
  < programmatic CLI overrides
```

Explain that an explicitly selected file is a deliberate user instruction and
therefore overrides environment variables.

- [ ] **Step 2: Add the environment naming and value contract**

Add README examples that include:

```bash
MY_APP_DATABASE_URL='postgres://localhost/app'
MY_APP_DATABASE__POOL_SIZE=16
MY_APP_FEATURE_ENABLED=true
MY_APP_TAGS='["worker", "blue"]'
```

State that booleans accept only lowercase `true`/`false`; empty strings remain
values; unknown prefixed paths are ignored unless collection is enabled; null
schema positions remain strings; compound values use quoted JSON.

- [ ] **Step 3: Document direct-loader and CLI override APIs**

Include runnable Rust examples for:

```rust
let (config, sources) = librebar::config::ConfigLoader::new("my-app")
    .with_project_search(&cwd)
    .with_override("database.pool_size", cli.pool_size)
    .load::<Config>()?;
```

and:

```rust
let app = librebar::init("my-app")
    .config::<Config>()
    .with_config_override("database.pool_size", cli.pool_size)
    .start()?;
```

- [ ] **Step 4: Fix stale crate documentation**

Change `Liblibrebar` to `Librebar` in `src/lib.rs`. Change dependency examples
from `version = "0.2"` to `version = "0.3"`. Do not edit `Cargo.toml` or any
project version field.

- [ ] **Step 5: Run doc tests**

Run:

```bash
cargo test --doc --all-features
```

Expected: PASS.

- [ ] **Step 6: Commit checkpoint (Clay-owned)**

Suggested message: `docs(config): document environment override contract`

---

### Task 7: Full verification and spec closure

**Files:**

- Modify: `record/superpowers/specs/2026-07-31-config-environment-overrides.md`
- Modify: `record/superpowers/plans/2026-07-31-config-environment-overrides.md`

- [ ] **Step 1: Run the repository completion gate**

Run:

```bash
just check
```

Expected: formatting, Clippy with `-D warnings`, cargo-deny, nextest, and doc
tests all pass.

- [ ] **Step 2: Inspect the final diff**

Run:

```bash
git --no-pager diff --check
git --no-pager diff --stat
git --no-pager status --short
```

Expected: no whitespace errors; only the planned source, tests, README, spec,
and plan files are changed. Preserve the user's `.config/bito.yaml` changes.

- [ ] **Step 3: Reconcile requirements**

Check every bullet in the accepted design against a test or documentation
section. Do not mark the design implemented if any behavior is missing.

- [ ] **Step 4: Mark artifacts complete**

Change the design status from `Accepted` to `Implemented` and check every plan
step only after `just check` succeeds.

- [ ] **Step 5: Final commit checkpoint (Clay-owned)**

Suggested message: `feat(config): add environment override layer`
