---
audit_date: 2026-04-08
project: rebar
commit: 885e493cf5fcad7d8361d5a7e8cf4352480f8644
scope: Full crate audit of the rebar library, manifest, tests, and Rust review tool output
auditor: Codex GPT-5
findings:
  critical: 0
  significant: 3
  moderate: 2
  advisory: 1
  note: 0
---

# Audit: Rebar

`rebar` is a small Rust application foundation crate with feature-gated
startup, config, logging, OTEL, shutdown, crash, and MCP helpers. **The
Configuration Surface** is the weakest part of the crate because malformed
discovered config is ignored and the builder cannot route a loaded `log_dir`
into logging setup. **The Startup Surface** is otherwise tidy, but it still
hides two panic edges behind `Result`-returning APIs: tracing initialization
and Tokio runtime availability. **The Shutdown Surface** is structurally clean
and race-free, but one caller contract treats sender disappearance as though a
real shutdown signal happened. **The Supply Chain Surface** is current and
RustSec-clean after rerunning `cargo audit`, but it still carries one dead
dependency. Fix the startup/config boundary and this is solid infrastructure.

---

## The Configuration Surface

*The configuration surface fails open when discovery goes wrong and drops one documented logging input on the floor.*

### Discovered config parse failures are silently ignored

**significant** · `src/config.rs:267-283` · effort: small · <img src="assets/sparkline-config-loader-discards-discovered-config-errors.svg" height="14" alt="commit activity" />

Both discovered-file branches only merge on `Ok(value)`. Any read or parse
failure from `parse_file` is discarded without an error, warning, or source
record, so malformed or unreadable config silently degrades to lower-precedence
files or struct defaults.

```rust src/config.rs:267-283
// User config (lowest precedence of file sources)
if self.include_user_config
    && let Some(user_config) = self.find_user_config()
    && let Ok(value) = parse_file(&user_config)
{
    deep_merge(&mut merged, value);
    sources.user_file = Some(user_config);
}

// Project config
if let Some(ref root) = self.project_search_root
    && let Some(project_config) = self.find_project_config(root)
    && let Ok(value) = parse_file(&project_config)
{
    deep_merge(&mut merged, value);
    sources.project_file = Some(project_config);
}
```

**Remediation:** Propagate discovered config failures as typed startup errors,
or make fail-open behavior an explicit opt-in policy with a recorded warning.

<div>&hairsp;</div>

### Loaded config never reaches logging target selection

**moderate** · `src/lib.rs:502-530` · effort: medium · <img src="assets/sparkline-builder-ignores-configured-log-directory.svg" height="14" alt="commit activity" />

`ConfiguredBuilder::start` does load a typed config value, but the logging path
constructs `LoggingConfig` from only the app name and leaves `log_dir` at
`None`. There is no way for a config-sourced log directory to influence the
builder path.

```rust src/lib.rs:502-530
let (config, sources) = match self.config_source {
    CfgSource::Discover => {
        let cwd = std::env::current_dir().map_err(crate::Error::Io)?;
        let cwd = camino::Utf8PathBuf::try_from(cwd).map_err(|e| {
            crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "current directory is not valid UTF-8: {}",
                    e.into_path_buf().display()
                ),
            ))
        })?;
        config::ConfigLoader::new(&self.app_name)
            .with_project_search(&cwd)
            .load::<C>()?
    }
    CfgSource::File(path) => config::ConfigLoader::new(&self.app_name)
        .with_user_config(false)
        .with_file(&path)
        .load::<C>()?,
    CfgSource::Preloaded(config) => (config, config::ConfigSources::default()),
};

// Build layers
#[cfg(feature = "logging")]
let (log_layer, log_guard) = if do_logging {
    let log_cfg = logging::LoggingConfig::from_app_name(&self.app_name);
    let (layer, guard) = logging::build_json_layer(&log_cfg)?;
    (Some(layer), Some(logging::LoggingGuard::from_guard(guard)))
} else {
    (None, None)
};
```

**Remediation:** Add an explicit builder hook for the log directory, or remove
the implicit config-level promise and require callers to pass logging targets
explicitly.

*Verdict: This surface is the main source of real behavioral drift. Operator
inputs should either work or fail loudly; right now they can disappear into
defaults and fallbacks instead.*

<div>&nbsp;</div>

## The Startup Surface

*Initialization paths return Result but still hide realistic panic edges at the tracing-global and runtime boundaries.*

### Logging startup panics when tracing was initialized earlier

**significant** · `src/logging.rs:103-109` · effort: small · <img src="assets/sparkline-tracing-init-panics-instead-of-returning-error.svg" height="14" alt="commit activity" />

Both public startup paths call `SubscriberInitExt::init()`, which panics if a
global default subscriber already exists. That bypasses the typed startup error
contract entirely.

```rust src/logging.rs:103-109
pub fn init(cfg: &LoggingConfig, env_filter: EnvFilter) -> Result<LoggingGuard> {
    let (log_layer, log_guard) = build_json_layer(cfg)?;

    tracing_subscriber::registry()
        .with(env_filter)
        .with(log_layer)
        .init();
```

**Remediation:** Use `try_init()` and map duplicate-subscriber cases into a
typed error variant.

<div>&hairsp;</div>

### Shutdown startup assumes an active Tokio runtime

**significant** · `src/shutdown.rs:64-87` · effort: small · <img src="assets/sparkline-shutdown-startup-assumes-a-tokio-runtime.svg" height="14" alt="commit activity" />

The builder always calls `register_signals()` when shutdown is enabled, and
`register_signals()` always uses `tokio::spawn`. Without an active runtime,
that path panics instead of returning an error.

```rust src/shutdown.rs:64-87
pub fn register_signals(&self) -> crate::Result<()> {
    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| crate::Error::ShutdownInit(Box::new(e)))?;

    let handle = self.clone();

    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();

        #[cfg(unix)]
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }

        #[cfg(not(unix))]
        ctrl_c.await.ok();

        tracing::info!("shutdown signal received");
        handle.shutdown();
    });

    Ok(())
}
```

**Remediation:** Check runtime availability with `Handle::try_current()` and
return a typed error, or require the caller to provide a runtime handle.

*Verdict: Startup remains the second weak point in this crate. Once side
effects cross into global tracing state or runtime-managed tasks, the API stops
being honest about how it can fail.*

<div>&nbsp;</div>

## The Shutdown Surface

*The shutdown primitives are simple and race-free, but one API path conflates channel teardown with a real shutdown signal.*

### ShutdownToken::cancelled returns on sender drop without a shutdown signal

**moderate** · `src/shutdown.rs:104-111` · effort: small · <img src="assets/sparkline-shutdown-token-treats-channel-close-as-cancelled.svg" height="14" alt="commit activity" />

`watch::Receiver::changed()` returns an error when all senders are dropped.
Converting that result with `.ok()` makes channel teardown look like a real
shutdown.

```rust src/shutdown.rs:104-111
/// Wait until shutdown is triggered.
///
/// Resolves immediately if shutdown has already been triggered.
pub async fn cancelled(&mut self) {
    if *self.receiver.borrow_and_update() {
        return;
    }
    self.receiver.changed().await.ok();
}
```

**Remediation:** Preserve the distinction in the API by returning a status or
Result, or keep waiting until the observed value becomes `true`.

*Verdict: The concurrency surface itself is clean. This is a semantic contract
bug, not a structural race or deadlock issue.*

<div>&nbsp;</div>

## The Supply Chain Surface

*The dependency set is current and RustSec-clean, but one direct dependency is dead weight.*

### The cli feature carries an unused anyhow dependency

**advisory** · `Cargo.toml:35-35` · effort: trivial · <img src="assets/sparkline-unused-anyhow-direct-dependency.svg" height="14" alt="commit activity" />

The `cli` feature wires in `anyhow`, but the current source tree does not use
it and `cargo-machete` flagged it as unused.

```toml Cargo.toml:35-35
anyhow = { version = "1.0", optional = true }
```

**Remediation:** Remove `anyhow` from the manifest and the `cli` feature, or
document why it is intentionally staged.

*Verdict: This surface is otherwise healthy. `cargo audit` completed clean
after rerunning outside the sandbox and the direct dependency set is still
small.*

<div>&nbsp;</div>

## Remediation Ledger

| Finding | Concern | Location | Effort | Chains |
|---------|---------|----------|--------|--------|
| [config-loader-discards-discovered-config-errors](#discovered-config-parse-failures-are-silently-ignored) | significant | `src/config.rs:267-283` | small | related: builder-ignores-configured-log-directory |
| [builder-ignores-configured-log-directory](#loaded-config-never-reaches-logging-target-selection) | moderate | `src/lib.rs:502-530` | medium | related: config-loader-discards-discovered-config-errors |
| [tracing-init-panics-instead-of-returning-error](#logging-startup-panics-when-tracing-was-initialized-earlier) | significant | `src/logging.rs:103-109` | small | related: shutdown-startup-assumes-a-tokio-runtime |
| [shutdown-startup-assumes-a-tokio-runtime](#shutdown-startup-assumes-an-active-tokio-runtime) | significant | `src/shutdown.rs:64-87` | small | related: tracing-init-panics-instead-of-returning-error |
| [shutdown-token-treats-channel-close-as-cancelled](#shutdowntokencancelled-returns-on-sender-drop-without-a-shutdown-signal) | moderate | `src/shutdown.rs:104-111` | small | related: shutdown-startup-assumes-a-tokio-runtime |
| [unused-anyhow-direct-dependency](#the-cli-feature-carries-an-unused-anyhow-dependency) | advisory | `Cargo.toml:35-35` | trivial | — |

<sub>
Generated 2026-04-08 at commit 885e493c. Intermediate artifacts:
`recon.yaml`, `findings.yaml`.
</sub>
