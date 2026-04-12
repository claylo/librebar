# Handoff: 2026-04-11 audit closed, pre-1.0 readiness read

**Date:** 2026-04-11
**Branch:** main
**State:** Green (with caveats — read Landmines first)

> Green = `just check` end-to-end passes (fmt → clippy → deny → test 88/88 → doc-test). Caveats: the test gate only started *actually* running tests in this commit; `.github/workflows/` still does not exist.

## Where things stand

The 2026-04-11-full-crate cased audit is closed: 3 fixed, 1 accepted. Commit `e8c56e4` bundles the audit artifacts and the remediation (tests, `rand` lockfile bump, `.config/deny.toml` rationale, `.justfile` test-gate fix) and is merged to main. The crate compiles cleanly on `--all-features`, `cargo audit` reports only the one accepted bincode warning, and `just check` is clean for the first time in rebar's history.

## Decisions made

- **`rand 0.9.2` → `0.9.3` via `cargo update -p rand`.** `opentelemetry_sdk 0.31.0`'s `^0.9` requirement resolved the patched version without a `Cargo.toml` change. RUSTSEC-2026-0097 cleared.
- **bincode advisory documented, not runtime-ignored.** Prose block in `.config/deny.toml` under `[advisories]` — cargo-deny's default scan never reaches `bench-gungraun`, so a runtime ignore produces spurious `advisory-not-detected` warnings. Revisit conditions in `actions-taken.md`.
- **`just test` / `just test-ci` now pass `--workspace --all-features`.** Previously ran zero tests because rebar has no default features and every integration test file is `#![cfg(feature = "…")]`. See Landmine 1 for full context.
- **`merge_rejects_excessive_depth` uses symmetric 65-layer objects.** Asymmetric test would short-circuit through scalar-replace at depth 1 and never trip `MERGE_DEPTH_LIMIT`.

## What's next

Pre-1.0 punch list, ordered by impact on downstream trust.

1. **Ship `.github/workflows/ci.yml` running `just check` on push and PR.** Until this exists, every "passing" claim is local-only. Also wire `dependabot-issues.yml` that `dependabot.yml` references but does not exist.
2. **Reconcile `README.md` feature table with `Cargo.toml`.** README lines 71-72 list ten features as "coming in later phases" — all ten shipped. Mark them present or note "implemented, unstable."
3. **Let `just test` soak 2+ weeks with no breaking API changes.** Historical green pre-`e8c56e4` is retroactively unverified.
4. **Compile the doc examples.** `src/lib.rs:83` (builder), `src/mcp.rs`, `src/dispatch.rs`, `src/diagnostics.rs` use ```` ```ignore ````. Start with the top-level builder example.
5. **Write a semver policy in `README.md`.** Two API-breaking changes shipped in 72 hours on Apr 8-9 (see Landmine 6).
6. **Fill `keywords` and `categories` in `Cargo.toml:12-13`.** Empty values block crates.io discoverability.
7. **Dogfood with one non-Clay consumer for 30 days.** Writing both sides of the interface hides API ergonomics issues that audits miss.

## Landmines

- **`just check` verified zero tests from `70b945d` through `77836b5`.** Recipe was `cargo nextest run` with no feature flags; rebar has no default features, every test is `#![cfg(feature = "…")]`, so the runner compiled nothing and exited cleanly. Every prior `fix: remediate` commit landed on a green gate that wasn't testing anything. Fixed in `e8c56e4`; prior history cannot be retroactively validated.
- **No CI exists.** `.github/workflows/` does not exist. `dependabot.yml` references a `dependabot-issues.yml` workflow that also does not exist. Dependency alerts go nowhere.
- **`update_test::suppressed_by_env_var` is nextest-dependent.** Under threaded `cargo test`, `std::env::set_var` races a sibling test's `remove_var`. Do not revert `just test` to `cargo test` without moving the env probe off shared process state.
- **`.config/deny.toml` documents RUSTSEC-2025-0141 acceptance as comments, not a runtime `ignore`.** The rationale block explains why: cargo-deny's feature-gated scan doesn't reach `bench-gungraun`, so an ignore produces `advisory-not-detected` noise.
- **README lists ten features as "coming in later phases" that all shipped.** Until reconciled, verify feature claims from `src/` and `tests/`.
- **API breakage is recent; no `CHANGELOG.md` exists.** `DebugBundle::add_text()` and `deep_merge()` changed return types on 2026-04-09 — listed as landmines in handoff `2026-04-09-1802`, not caught until the 2026-04-11 audit.
- **Bus factor 1.** 34 commits, 1 author, ~6 weeks, no external downstream users.
