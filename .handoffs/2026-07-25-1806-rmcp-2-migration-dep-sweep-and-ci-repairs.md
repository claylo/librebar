# Handoff: rmcp 2 migration, dep sweep, and CI repairs

**Date:** 2026-07-25
**Branch:** main (clean)
**State:** Green — v0.2.0 published

> Green = `cargo fmt --check` + `cargo clippy --all-features -D warnings` + 87/87 nextest + 17/17 doc-tests + `cargo deny` (advisories/bans/licenses/sources ok) + `actionlint` + MSRV `cargo check` on 1.89.0, all passing on `main`.

## Where things stand

v0.2.0 is live on crates.io (confirmed via the sparse index, not yanked). Three commits landed: the dependency sweep and rmcp 2.2 migration (#22), `chore: release 0.2.0`, and CI repairs (#23). The `v0.2.0` tag was moved onto #23 (`2c8cd3b`) after the cargo-deny fix landed, so the tagged commit is fully green — all four CI jobs pass. The earlier red runs on #22 and on the release commit were the `--config` arg placement alone, not the code.

Every dependency is current — `just outdated` reports "All dependencies are up to date." The headline change is **rmcp 1.7 → 2.2**, which is breaking for `mcp` feature consumers: rmcp 2.0 collapsed `Annotated<T>` and the parallel `Raw*` structs into a unified `ContentBlock`, removing `Content`. `src/mcp.rs` needed no edit because it re-exports modules rather than individual types, so the rename passes straight through to consumers.

Two build-tooling bugs surfaced along the way, both silent for months. `just update` had never done anything — `cargo update --workspace` is shorthand for `-p <each workspace member>`, which on a single-crate workspace re-resolves nothing. And cargo-deny 0.20 moved `--config` from the `check` subcommand to a global flag, breaking `just deny` locally.

The most serious find came from actionista's `workflow-analyzer`: **the `msrv` CI job had never tested the MSRV.** `rust-toolchain.toml` (`channel = "1.97.1"`) outranks `rustup default`, which is how `dtolnay/rust-toolchain` selects a toolchain, so the bare `cargo check` in that job silently resolved to 1.97.1. The crate does compile clean on 1.89.0 — verified two ways — so this closed a coverage gap rather than exposing a break.

## Decisions made

- **`feat(mcp)!` not `chore(deps)` for the dependency sweep.** scrat uses git-cliff conventional-commits, and `chore` produces no version bump — the rmcp break would have shipped silently at 0.1.0. Verified empirically in a throwaway clone: `chore(deps)` → 0.1.0, `feat(mcp)!` → 0.2.0.
- **base64 0.23 taken with `default-features = false, features = ["std"]`.** 0.23 added `simd-unsafe` to its *default* feature set. The cache module only needs STANDARD encode/decode, which the safe path provides. Verified `simd-unsafe` is absent from the resolved graph.
- **Removed the `RUSTSEC-2025-0141` ignore from `.config/deny.toml`.** gungraun 0.19.4 migrated to `bincode-next` 2.x, so bincode 1.x left the graph entirely — the suppression's own documented revisit condition. cargo-deny was already flagging it as unmatched.
- **Pinned `cargo-deny-action` to v2.1.1 (bundles cargo-deny 0.20.2) rather than reverting the flag position.** The two `--config` placements are mutually exclusive — `<=0.19.x` accepts it only after the subcommand, `0.20+` only before — so there is no invocation that works on both. Matching the action's bundled version to the local install means one form instead of two that drift.
- **`RUSTUP_TOOLCHAIN` on the msrv step, not a `+toolchain` arg.** Env var outranks `rust-toolchain.toml`; dtolnay has already installed the toolchain by that point.
- **Added top-level `permissions: {}` to `lint-pr.yml`.** Its one job already scopes to `pull-requests: read`, but `pull_request_target` runs with a write-capable token, so a job added later that omits its own block should inherit nothing.

## What's next

1. **Replace both composite actions with proven ones.** `Swatinem/rust-cache` + `taiki-e/install-action` retire ~130 lines and roughly seven defects at once (see Landmines). This is the "borrow proven protocols" rule pointed at code we wrote.
2. **Decide what `lint`/`test` should actually compile with.** Both request `toolchain: stable` and get 1.97.1 via the same override that broke `msrv`. Harmless today because 1.97.x *is* stable; it diverges the day stable rolls to 1.98. Either drop the `toolchain:` inputs and let `rust-toolchain.toml` drive (matches `just clippy`, which runs `cargo +1.97.1`), or set `RUSTUP_TOOLCHAIN: stable` explicitly.
3. **Fix `dependabot-issues.yml` dedupe.** It searches `is:open`, so closed alert issues get recreated every Monday, and it issues one search per alert against a 30 req/min secondary limit. One paginated `listForRepo` with `state: "all"`, matched client-side on GHSA id, fixes both.
4. **Fix actionista so `workflow-analyzer` registers again** (separate repo, `~/source/claylo/actionista`): `git mv skills/actionista/agents agents`. Claude Code only registers agents from the plugin root.
5. **Consider a `just actions-outdated` recipe.** Dependabot is configured `open-pull-requests-limit: 0` on purpose, so action version drift is invisible until someone checks by hand — which is how these sat 3 months.

## Landmines

- **`hashFiles('~/.cargo/bin/*')` always returns the empty string** (`.github/actions/setup-cargo-tools/action.yml:44`). `hashFiles` only resolves inside `GITHUB_WORKSPACE`, and `~` is not expanded. The nextest cache key is therefore constant, the first restore-key is byte-identical to the primary key, and `actions/cache` does not save on an exact hit — so **cargo-nextest is frozen at whatever version installed first, permanently**, with no version pin and no log line.
- **`target/` cache thrash.** Keyed on `hashFiles('src/**/*.rs')`, which changes on essentially every commit, across three jobs. Local `target/` is ~13 GB; that blows GitHub's 10 GB per-repo budget in a handful of pushes, and LRU eviction then takes out the *registry* cache — the one that actually pays for itself.
- **That same key misses what it needs to hash.** `src/**/*.rs` covers 16 of 39 `.rs` files — it excludes `tests/`, `examples/` (7 `[[example]]` targets), and `Cargo.toml` where `[features]` and `[profile.*]` live. All are built by `--all-targets --all-features`. A PR touching only `tests/` gets an exact hit, so the save is skipped and the build is discarded.
- **`|| echo "Failed to install $tool"` swallows binstall failures** (`setup-cargo-tools/action.yml:62,73`). The step goes green and the job dies two steps later with `no such command: nextest`, pointing at the wrong place. These blocks also lack `set -euo pipefail`.
- **`~/.cargo/bin` caching clobbers rustup's shims.** The cache captures the whole directory and restores it over what `dtolnay/rust-toolchain` just installed. Benign today because shims are version-agnostic dispatchers; a runner-image rustup bump turns it into a confusing broken toolchain.
- **The actionista index bundled in the plugin cache is only as fresh as the plugin release.** The daily updater runs in the actionista repo, not in the installed copy. The v1.0.1 cache was 90 days stale and matched every pin exactly, which reads as "all current" when it means "frozen." Verify against live GitHub for anything that matters.
- **`CARGO_REGISTRY_TOKEN` was exposed in plaintext shell env** and echoed into a session transcript. Rotated. Any shell started before the rotation still holds the revoked value.
