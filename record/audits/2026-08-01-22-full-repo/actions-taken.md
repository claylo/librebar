---
audit: 2026-08-01-22-full-repo
last_updated: 2026-08-02
status:
  fixed: 13
  mitigated: 0
  accepted: 0
  disputed: 0
  deferred: 0
  open: 0
---

# Actions Taken: Full tracked repository: source, tests, examples, dependencies, configuration, CI/tooling, and documented public contracts

Summary of remediation status for the [2026-08-01 Full tracked repository: source, tests, examples, dependencies, configuration, CI/tooling, and documented public contracts audit](README.md).

---

## 2026-08-02 — Logrotate-style daily appender and config-driven log settings

**Disposition:** fixed
**Addresses:** [builder-ignores-configured-log-settings](README.md#builder-ignores-configured-log-settings), [log-path-override-is-not-exact](README.md#log-path-override-is-not-exact)
**Commit:** pending
**Author:** Claude Code

`PrivateDailyAppender` in `src/logging.rs` now writes to a stable filename (`myapp.jsonl`) and on date change renames the current file to `myapp.{date}.jsonl` before opening a fresh one. The `ensure_writable` probe no longer creates date-suffixed files. `BuilderInner` in `src/lib.rs` gained a `log_level: Option<String>` field. `ConfiguredBuilder::start()` now extracts `log_level` and `log_dir` from the serialized config before calling `init_subsystems()`, respecting the documented precedence. Tests updated in `tests/logging_test.rs` and `tests/builder_test.rs`.

---

## 2026-08-02 — Guard signal handler registration with OnceLock

**Disposition:** fixed
**Addresses:** [detached-signal-task-outlives-app-lifecycle](README.md#detached-signal-task-outlives-app-lifecycle)
**Commit:** pending
**Author:** Claude Code

`register_signals()` in `src/shutdown.rs` now guards with a `std::sync::OnceLock<()>`. First call proceeds normally; subsequent calls log a warning and return `Ok(())`. Test added in `tests/shutdown_test.rs`.

---

## 2026-08-02 — Advisory lock for cache set and prune

**Disposition:** fixed
**Addresses:** [cache-prune-can-delete-concurrent-fresh-write](README.md#cache-prune-can-delete-concurrent-fresh-write)
**Commit:** pending
**Author:** Claude Code

Added per-cache-directory advisory lock file (`.cache.lock`) in `src/cache.rs`. Both `set_parts()` and `prune_at()` acquire the lock before filesystem mutations. The lock is non-fatal — callers degrade gracefully on filesystems without advisory locking support.

---

## 2026-08-02 — Typed dispatch resolution errors

**Disposition:** fixed
**Addresses:** [dispatch-resolution-errors-collapse-to-not-found](README.md#dispatch-resolution-errors-collapse-to-not-found)
**Commit:** pending
**Author:** Claude Code

Added `DispatchError` enum to `src/dispatch.rs` with `PathNotSet`, `PathJoinFailed`, `CurrentDirFailed`, and `NotFound` variants. Added `try_resolve()` returning typed errors; `resolve()` delegates to it. `run()` now reports specific errors for non-`NotFound` failures. Test added in `tests/dispatch_test.rs`.

---

## 2026-08-02 — Typed crash dump errors

**Disposition:** fixed
**Addresses:** [crash-dump-errors-erased](README.md#crash-dump-errors-erased)
**Commit:** pending
**Author:** Claude Code

Added `CrashDumpError` enum to `src/crash.rs` with `CreateDir`, `OpenFile`, `Serialize`, and `Prune` variants. Added `try_write_crash_dump_to()` returning typed errors; `write_crash_dump_to()` wraps it with `.ok()`. Tests added in `tests/crash_test.rs`.

---

## 2026-08-02 — Cookie enforce_limits optimization

**Disposition:** fixed
**Addresses:** [cookie-limit-enforcement-scans-full-jar-on-every-response](README.md#cookie-limit-enforcement-scans-full-jar-on-every-response)
**Commit:** pending
**Author:** Claude Code

`store_response()` in `src/http/cookies.rs` now returns early when no `Set-Cookie` headers are present. `enforce_limits()` builds one inventory via `stored_cookie_keys()` instead of three, using `retain` for oversized removal and draining survivors for domain/total checks.

---

## 2026-08-02 — Cache clear_report with structured results

**Disposition:** fixed
**Addresses:** [cache-clear-reports-partial-success](README.md#cache-clear-reports-partial-success)
**Commit:** pending
**Author:** Claude Code

Added `clear_report()` to `src/cache.rs` returning `ClearReport { removed, failed }`. `clear()` now delegates to `clear_report()` discarding the report. Test added in `tests/cache_test.rs`.

---

## 2026-08-02 — Typed logging resolution errors

**Disposition:** fixed
**Addresses:** [logging-resolution-uses-string-errors](README.md#logging-resolution-uses-string-errors)
**Commit:** pending
**Author:** Claude Code

Added `LogTargetError` enum to `src/logging.rs` with `NoFileName`, `InvalidUtf8`, `CreateDirFailed`, `OpenFailed`, and `NoWritableDir` variants. Added `try_resolve_log_target_with()` wrapping the existing string-based API with typed errors. Tests added in `tests/logging_test.rs`.

---

## 2026-08-02 — ReleaseInfo validation newtypes

**Disposition:** fixed
**Addresses:** [release-info-allows-invalid-metadata](README.md#release-info-allows-invalid-metadata)
**Commit:** pending
**Author:** Claude Code

Added `ReleaseVersion` and `ReleaseUrl` newtypes to `src/update.rs` with validation in constructors. Added `ReleaseInfo::try_new()` that validates both before constructing. Existing `new()` and public fields preserved for compatibility. Tests added in `tests/update_test.rs`.

---

## 2026-08-02 — HttpClient and HttpClientConfig are Clone

**Disposition:** fixed
**Addresses:** [http-client-is-not-cloneable](README.md#http-client-is-not-cloneable)
**Commit:** pending
**Author:** Claude Code

Derived `Clone` on `HttpClientConfig` and implemented `Clone` for `HttpClient` in `src/http.rs`. The inner `BoxCloneSyncService` and `Arc`-wrapped cookie jar both support shared semantics. Test added in `tests/http_test.rs`.

---

## 2026-08-02 — Disable unused default features for otel and tar dependencies

**Disposition:** fixed
**Addresses:** [otel-enables-unused-default-integrations](README.md#otel-enables-unused-default-integrations), [diagnostics-tar-enables-unused-xattr](README.md#diagnostics-tar-enables-unused-xattr)
**Commit:** pending
**Author:** Claude Code

In `Cargo.toml`: added `default-features = false` to `opentelemetry`, `opentelemetry_sdk` (also removed `rt-tokio` from features), `tracing-opentelemetry`, and `tar`. Verified with `cargo check --features otel`, `cargo check --features otel-http-json`, `cargo check --features otel-grpc`, and `cargo check --features diagnostics`.
