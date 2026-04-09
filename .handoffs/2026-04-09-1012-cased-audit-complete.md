# Handoff: Full crate audit (cased + crustoleum)

**Date:** 2026-04-09
**Branch:** main
**State:** Yellow

> Yellow = tests pass, known issues exist. 13 findings from audit, 2 already fixed in working tree.

## Where things stand

A full cased audit of rebar ran the crustoleum rubric (84 criteria, 13 surfaces, 6 parallel agents). 13 findings produced: 0 critical, 1 significant, 4 moderate, 6 advisory, 2 note. Two are already fixed in the working tree (not committed): `opentelemetry-otlp` default-features and the stale `deny.toml` comment. The audit artifacts live at `record/audits/2026-04-09-full-crate/` with an interactive HTML report, markdown index, and machine-readable YAML.

## Decisions made

- Rated `http-feature-missing-serde-json` as moderate (not advisory) because the crate's own `update` module demonstrates the gap — this is a real API completeness issue, not theoretical.
- Clean verdicts on memory safety and concurrency surfaces — `#![deny(unsafe_code)]` and absence of `std::sync::Mutex` in async code structurally prevent entire bug categories.
- Error type erasure (`Box<dyn Error>` in 11/13 variants) rated moderate — pragmatic for 0.1.0 but should be addressed before 1.0 for the most actionable variants (Http, Cache).

## What's next

Resolve remaining 11 audit findings. Suggested priority order:

1. **Add `serde_json` to `http` feature + `Response::json::<T>()` method** — `Cargo.toml:100`, `src/http.rs:166-185`. Highest value: simplifies `update.rs` and every downstream consumer.
2. **Comment the 5 `let _ =` discards** — `src/logging.rs:448`, `src/shutdown.rs:41`, `src/cache.rs:114,152`, `src/update.rs:129`. Add eprintln fallback in logging.rs. Trivial effort.
3. **Add `#[derive(Debug)]` to public types** — `HttpClientConfig`, `Cache`, `UpdateChecker`, `DebugBundle`. Trivial.
4. **Add `tracing::debug!` before `.ok()?` in `update.rs:121-122`** — consistent with the logging pattern on line 111. Trivial.
5. **Level string optimization in logging hot path** — `src/logging.rs:426`. Replace `.to_lowercase()` with a match returning `&'static str`. Small.
6. **Add `Response::into_text(self)` and optionally `text_ref(&self)`** — `src/http.rs:172-174`. Trivial.
7. **Builder/ConfiguredBuilder deduplication** — `src/lib.rs:300-462,586-648`. Extract `BuilderInner` or use `macro_rules!`. Medium effort.
8. **DebugBundle infallible Result** — `src/diagnostics.rs:209-218`. Change return to `&mut Self`. Trivial.
9. **DoctorCheck: add Send supertrait** — `src/diagnostics.rs:35`. Trivial.
10. **deep_merge depth limit** — `src/config.rs:111-120`. Add depth counter. Small.
11. **Error type refinement** — `src/error.rs`. Per-module enums for Http and Cache. Medium, defer if needed.

## Landmines

- **Working tree has uncommitted fixes.** `Cargo.toml` (otel default-features) and `.config/deny.toml` (comment fix) are modified but not committed. The audit report still references the pre-fix state.
- **`record/audits/` is new and untracked.** The entire audit directory needs to be committed. It includes a 361KB `report.html` — verify this is wanted in the repo before staging.
- **`.crustoleum/` tool output directory.** Created by the tool run, contains clippy/audit/deny/machete/geiger/udeps text files. Likely should be gitignored, not committed.
- **`findings.yaml` references pre-fix line numbers.** After the otel fix and deny.toml fix are committed, line numbers in `Cargo.toml` and `.config/deny.toml` may shift. The audit captures state at commit `f7a028c`.
