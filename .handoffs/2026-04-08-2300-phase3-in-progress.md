# Handoff: Phase 3 In Progress

**Date:** 2026-04-08
**Branch:** feat/phase3
**State:** Green

> Green = tests pass, safe to continue.

## Where things stand

Phase 3 is 4 of 10 tasks complete on `feat/phase3`. The branch has stubs for all 7 new features, plus working implementations of `lockfile`, `http`, and builder `.with_version()`. 65 tests pass across 10 test binaries, clippy clean with all features. The audit remediation from earlier this session was merged to `main` at `0836e26` before branching.

## Decisions made

- **fs4 instead of fd-lock for lockfile** -- fd-lock's `RwLockWriteGuard` borrows the `RwLock`, forcing unsafe transmute for self-referential struct. fs4's `try_lock_exclusive()` locks at the fd level; OS releases when File drops. Zero unsafe.
- **hyper-util client (wrapped) for HTTP** -- hyper 1.x's connection-level API requires manual h2/h1 fallback logic that reimplements what `hyper_util::client::legacy::Client` already does. Using it behind `rebar::http::HttpClient` so the "legacy" naming never leaks to consumers.
- **Builder `.with_version()`** -- Crash dumps and OTEL resource attributes now use the consumer's version when set. Defaults to rebar's `CARGO_PKG_VERSION` for backward compatibility. Stored in `App` for later use by `update` and `diagnostics`.
- **Dropped direct `http` crate dep** -- hyper re-exports all needed HTTP types. Removed dead dependency.

## What's next

1. **Resume at Task 5: Cache module** -- `src/cache.rs`, XDG cache with TTL. Plan at `record/superpowers/plans/2026-04-08-rebar-phase3.md`, Task 5.
2. **Tasks 6-9** -- `update`, `dispatch`, `diagnostics`, `bench` in that order. All have plan text with tests and implementation.
3. **Task 10: Verification** -- Full feature isolation check, all-features test suite, fmt, clippy, doc.
4. **After Phase 3** -- Merge to main, then Phase 4 (template migration to claylo-rs).

## Landmines

- **Plan is partially stale** -- The Phase 3 plan (`record/superpowers/plans/2026-04-08-rebar-phase3.md`) specifies fd-lock and manual hyper connection management. Actual implementation uses fs4 and hyper-util client. Read the code, not the plan, for Tasks 3-4.
- **`cache` module needs base64** -- Plan uses `base64` crate (transitive via opentelemetry-otlp). If `cache` is enabled without `otel`, base64 may not be available. May need to add as explicit dep or use hex encoding instead.
- **HTTP is cleartext only** -- No TLS connector wired in. The `update` module needs HTTPS for GitHub API. TLS support (`hyper-rustls` or similar) must be added before `update` can make real network calls. Tests are offline/local only.
- **`tracing_subscriber::registry().init()` is once-per-process** -- Use `cargo nextest run` (process-per-test). Do not switch to `cargo test`.
- **Test attribute ordering** -- `#![allow(missing_docs)]` must come BEFORE `#![cfg(feature = "...")]` in integration test files or clippy errors on the empty crate.
- **Subagent-driven execution** -- This session used `superpowers:subagent-driven-development`. Tasks 5-10 should continue with the same pattern. Mechanical tasks (cache, dispatch, bench) work well with sonnet subagents. Diagnostics (Task 8) may need a more capable model.
