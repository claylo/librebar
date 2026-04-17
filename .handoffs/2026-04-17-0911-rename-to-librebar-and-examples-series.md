# Handoff: rename to librebar, examples series launched

**Date:** 2026-04-17
**Branch:** main
**State:** Green

> Green = `just check` passes (fmt → clippy → deny → 88/88 nextest → doc-test). CI on main matches after PR#3 fix commit.

## Where things stand

The crate is renamed to `librebar`. GitHub repo is `claylo/librebar`. Three PRs landed this session: #2 (rename + launch scaffolding), #3 (examples foundation — `minimal.rs` + README + index), plus a CI fix commit on top of #3 to align local and CI deny/test coverage. CI is fully green on main. One of four planned examples is done; three remain, sequenced into their own PRs.

## Decisions made

- **Rename to `librebar` (package, lib, repo, URLs).** crates.io `rebar` name is held by an inactive 1-release crate with unreachable maintainer. Package + library + repo + URLs all flipped to avoid a package/import split.
- **Scaffolding ported from `../ah-ah-ah` (not scrat).** ah-ah-ah is library-shaped; scrat is CLI-shaped. Ported: composite actions, CI + dependabot-issues + lint-pr workflows, issue/PR templates (adapted for library), SECURITY.md. `cliff.toml` initially ported then removed — scrat provides it.
- **Examples design: scenario-based flagships, not per-feature micros.** Tests prove "feature compiles in isolation"; examples prove "features compose for a real job." Four total: `minimal`, `service`, `updater`, `plugin-cli`. Each lands as its own PR.
- **Example app-name is hardcoded inside example binaries.** `env!("CARGO_PKG_NAME")` resolves to `librebar` (the hosting crate) in cargo examples, not the example name. Hardcoding inside the example + comment explaining why keeps config discovery working against `examples/<name>.toml`.
- **Local `just deny` and CI deny both use `--all-features`.** Asymmetric feature coverage is what let PR#3 ship "green" locally and fail CI. Fixed in the PR#3 follow-up commit.
- **`RUSTSEC-2025-0141` (bincode unmaintained) is now a runtime `ignore` entry.** Previously a prose-only acknowledgement under the assumption the scan wouldn't reach `bench-gungraun`. Flipping to `--all-features` inverts that premise — runtime ignore is now correct.

## What's next

1. **PR#4 — `examples/service.rs`** (cli + config + logging + shutdown + crash + otel). Biggest example: async `main`, SIGINT handling via `app.shutdown_token()`, panic-hook crash dumps, OTEL layer with console fallback when `OTEL_EXPORTER_OTLP_ENDPOINT` is unset. Branch off main as `examples-service`.
2. **PR#5 — `examples/updater.rs`** (cli + http + cache + update). Real call to the GitHub releases API for a public crate, 24h cache behavior, `{APP}_NO_UPDATE_CHECK=1` gate. Branch `examples-updater`.
3. **PR#6 — `examples/plugin-cli/`** (cli + dispatch). Multi-binary layout: main binary + `hello-greet/` fake PATH-resolved subcommand. Branch `examples-plugin-cli`.
4. **Deferred**: `examples/doctor-bundle/` and `examples/mcp-server.rs`. Not in the committed design; add when there's appetite.
5. **Unrelated, still on the pre-1.0 punch list from `2026-04-11-2141`**: let tests soak 2+ weeks, compile the `ignore` doc examples in `src/lib.rs:83` and `src/mcp.rs`/`src/dispatch.rs`/`src/diagnostics.rs`, write a semver policy in README, dogfood with one non-Clay consumer.

## Landmines

- **Local `just deny` only catches `--all-features` issues now.** The recipe has `--all-features` baked in (`.justfile:22`) — good. But if anyone reverts it "because local is slower," CI-green-local-red reappears. Don't.
- **cargo example hosting crate name leaks.** `env!("CARGO_PKG_NAME")` inside `examples/*.rs` resolves to `librebar`. Any future example that wants per-example config discovery must hardcode its own name. See `examples/minimal.rs:78-83` for the pattern.
- **`cli.common.apply_chdir()?` must be called manually before `librebar::init()`.** The builder does not auto-apply `-C`. Without the explicit call, config discovery runs from whatever CWD the process started in. `examples/minimal.rs:73` is the reference.
- **`CDLA-Permissive-2.0` is now allowed in `.config/deny.toml`.** One entry. Reached through `hyper-rustls → webpki-roots`. If you audit the license list, don't drop it without checking whether webpki-roots is still in the tree.
- **Release-note trailers only attach to `feat` and `fix` commits.** The `ci:`-typed CI fix on PR#3 deliberately skipped trailers. If you rewrite that commit as `fix(ci):`, the lint hook will demand trailers.
- **`.handoffs/` and `record/` are frozen in time.** The rename swept live source and config but left historical docs alone. Any reference to "rebar" in those directories is intentional — don't bulk-rename.
- **PR#3 landed as two commits** (the feat + the ci follow-up) because CI failed post-merge-to-branch. If Clay squashed on merge, only the feat subject made main; the `ci:` context lives only in the PR.
