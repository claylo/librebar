---
audit: 2026-04-08-full-crate
last_updated: 2026-04-08
status:
  fixed: 6
  mitigated: 0
  accepted: 0
  disputed: 0
  deferred: 0
  open: 0
---

# Actions Taken: Rebar Full Crate Audit

Summary of remediation status for the
[2026-04-08 full crate audit](index.md).

---

## 2026-04-08 — Fix all six audit findings

**Disposition:** fixed
**Addresses:** [config-loader-discards-discovered-config-errors](index.md#discovered-config-parse-failures-are-silently-ignored), [builder-ignores-configured-log-directory](index.md#loaded-config-never-reaches-logging-target-selection), [tracing-init-panics-instead-of-returning-error](index.md#logging-startup-panics-when-tracing-was-initialized-earlier), [shutdown-startup-assumes-a-tokio-runtime](index.md#shutdown-startup-assumes-an-active-tokio-runtime), [shutdown-token-treats-channel-close-as-cancelled](index.md#shutdowntokencancelled-returns-on-sender-drop-without-a-shutdown-signal), [unused-anyhow-direct-dependency](index.md#the-cli-feature-carries-an-unused-anyhow-dependency)
**Commit:** branch `fix/audit-remediation`, pending merge
**Author:** Clay Loveless + Claude

All six findings addressed in a single remediation pass. 57 tests pass, clippy clean, `cargo doc` builds without warnings. Each fix described below.

### config-loader-discards-discovered-config-errors

Removed the `let Ok(value) = parse_file(...)` pattern from the user-config and project-config let-chains in `ConfigLoader::load`. Discovery (`find_user_config`, `find_project_config`) remains optional, but once a file is found, `parse_file` now propagates errors via `?`. A discovered file that exists but fails to parse is a hard error, not a silent fallback.

Added `tracing::debug!` at each discovery point so operators can trace which config files are found and merged.

```rust src/config.rs:267-283
// User config (lowest precedence of file sources)
if self.include_user_config
    && let Some(user_config) = self.find_user_config()
{
    tracing::debug!(path = %user_config, "discovered user config");
    let value = parse_file(&user_config)?;
    deep_merge(&mut merged, value);
    sources.user_file = Some(user_config);
}

// Project config
if let Some(ref root) = self.project_search_root
    && let Some(project_config) = self.find_project_config(root)
{
    tracing::debug!(path = %project_config, "discovered project config");
    let value = parse_file(&project_config)?;
    deep_merge(&mut merged, value);
    sources.project_file = Some(project_config);
}
```

### builder-ignores-configured-log-directory

Added a `log_dir: Option<PathBuf>` field to both `Builder` and `ConfiguredBuilder`, with a public `with_log_dir()` method on each. The field carries through the config type-state transition and is threaded into `LoggingConfig` via `with_log_dir()` during `start()`. Consumers can now wire a config-sourced log directory into the builder:

```rust
let app = rebar::init("myapp")
    .logging()
    .with_log_dir(config.log_dir)
    .start()?;
```

### tracing-init-panics-instead-of-returning-error

Replaced all four `.init()` call sites with `.try_init().map_err(|e| Error::TracingInit(Box::new(e)))?` in `Builder::start()`, `ConfiguredBuilder::start()`, and `logging::init()`. Added `Error::TracingInit` variant to the error enum. Double-init now returns a typed error instead of panicking.

### shutdown-startup-assumes-a-tokio-runtime

Added `tokio::runtime::Handle::try_current()` check at the top of `register_signals()`. Returns `Error::NoRuntime` when no Tokio runtime is active instead of panicking inside `tokio::spawn`. The obtained handle is used for spawning, so the signal task runs on whatever runtime the caller entered.

```rust src/shutdown.rs:64-67
pub fn register_signals(&self) -> crate::Result<()> {
    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|e| crate::Error::NoRuntime(Box::new(e)))?;
```

### shutdown-token-treats-channel-close-as-cancelled

Replaced the `.ok()` call on `changed().await` with a loop that checks whether the value is actually `true`. When the sender drops without signaling, the method emits `tracing::warn!("shutdown handle dropped without triggering shutdown")` and enters `std::future::pending()` — it will never resolve spuriously.

```rust src/shutdown.rs:108-121
pub async fn cancelled(&mut self) {
    loop {
        if *self.receiver.borrow_and_update() {
            return;
        }
        if self.receiver.changed().await.is_err() {
            tracing::warn!("shutdown handle dropped without triggering shutdown");
            std::future::pending::<()>().await;
        }
    }
}
```

### unused-anyhow-direct-dependency

Removed `anyhow` from `[dependencies]` and from the `cli` feature gate in `Cargo.toml`.
