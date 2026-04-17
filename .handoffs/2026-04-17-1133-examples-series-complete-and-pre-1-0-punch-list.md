# Handoff: examples series complete, pre-1.0 punch list drawn down

**Date:** 2026-04-17
**Branch:** examples-mcp-server
**State:** Green

> Green = `just check` passes (fmt → clippy --all-features → deny --all-features → 86/86 nextest → 6/17 doc-tests). Manual MCP protocol round-trip verified end-to-end.

## Where things stand

The committed examples series is complete: `minimal` → `service` → `updater` → `plugin-cli` → `doctor-bundle` → `mcp-server`, each exercising a distinct feature-set shape. Eight PRs landed this session (PRs #4 through #11 plus this staged-and-ready #12 for mcp-server), closing every item from the 2026-04-17-0911 handoff and most of the pre-1.0 punch list from 2026-04-11-2141.

## Decisions made

- **Semver policy landed (PR #9, README "Versioning" section).** Pre-1.0 convention: minor bumps may contain breaking changes, patch bumps are additive. 1.0 gate is a readiness call — not contingent on external consumers. Friendly "using it? let me know" invitation is decoupled from the trigger.
- **Test hygiene: `#[ignore]` over env-var early-return (PR #8).** Network tests now report SKIP honestly instead of silent PASS. Env-var tests serialize with a file-level `static Mutex<()> = Mutex::new(())` so they work under `cargo test` too, not just nextest's per-process isolation.
- **OTEL end-to-end recipe uses Aspire Dashboard in README.** Clay's preferred local receiver. Jaeger / OTel Collector / Honeycomb / Grafana Cloud work the same — just swap `OTEL_EXPORTER_OTLP_ENDPOINT`.
- **`just doc-test` now passes `--all-features` (PR #10).** Without it, feature-gated modules' doc-blocks silently never compile. This surfaced that cache/lockfile blocks were already passing while cli/http/otel/shutdown/crash/update/bench/second-lib.rs blocks are still `ignore`.
- **mcp-server example uses manual `ServerHandler` impl, not rmcp `#[tool]` macros.** librebar's `Cargo.toml` keeps rmcp's `macros` feature disabled so consumers don't pay the proc-macro cost if they're not using it. Example documents the tradeoff and tells readers how to opt in.
- **MCP stdio discipline is explicit in the example doc-comment.** librebar's `logging` layer writes JSONL to file, never to stdout/stderr, which is why `.logging()` is safe to wire in an MCP server. Any added fmt layer would desync the JSON-RPC framing on stdout.

## What's next

1. **Merge PR #12 (staged).** `commit.txt` ready on the `examples-mcp-server` branch. `gtxt` + `git pm`.
2. **Add `#[non_exhaustive]` to the `Error` enum in `src/error.rs`.** The semver policy explicitly promises this is on the roadmap; code doesn't reflect it yet. Once added, the policy's "adding Error variants is not breaking" clause becomes true.
3. **Unignore the remaining 11 rustdoc blocks.** Scope for PR #10 was the four flagged in the 2026-04-11 handoff. Still ignored: `src/lib.rs:466` (a second builder example), `src/cli.rs:39`, `src/cli.rs:98`, `src/crash.rs:9`, `src/crash.rs:17`, `src/http.rs:12`, `src/otel.rs:9`, `src/shutdown.rs:9`, `src/update.rs:10`, `src/bench.rs:11`, `src/bench.rs:22`. Pattern established in PR #10 applies (hidden `#`-prefixed scaffolding + `no_run`).
4. **Publish 0.1.0 to crates.io.** All the scaffolding is in place (package metadata, Cargo.toml exclude list, cliff.toml via scrat, SECURITY.md, README with installation snippet). Biggest unknown: `cargo publish --dry-run` to confirm nothing else blocks.
5. **Second-half of the mcp-server example (optional).** Only `Run` and `Info` subcommands ship today; a `call` subcommand that spawns the server as a subprocess and exercises the tool round-trip would give this example the same self-contained smoke loop as the others. Not in current scope.

## Landmines

- **rmcp minor-version resolution.** `Cargo.toml` pins `rmcp = "1.3"`, Cargo resolved to 1.5.0. Semver-compatible, but if rmcp makes surface changes within 1.x (they did between 1.3 and 1.5 — `CallToolRequestParam` got deprecated in favor of `CallToolRequestParams`), the mcp-server example may need adjustments. Pin harder if this bites.
- **`just doc-test` must keep `--all-features`.** If someone strips the flag "to speed up local runs," every `#[cfg(feature = "…")]` module's doc-block stops being compiled and drift sneaks back in. The `.justfile` has an explanatory comment. Don't delete it.
- **11 rustdoc `ignore` blocks remain as rot surface.** They compile-check against nothing. If the public API they describe changes, rustdoc will render stale code happily. Item #3 above.
- **Error enum isn't `#[non_exhaustive]` yet but the README says it will be.** The semver policy is written against a promised future state. Until item #2 lands, the policy is slightly aspirational — adding a variant today is technically breaking.
- **MCP stdout is sacred.** librebar's logging goes to file only, which is why the example works. Any future `.fmt_layer()` or `println!` added to the server path will desync the JSON-RPC protocol on stdout. Module doc calls this out; enforcement is vigilance.
- **`.handoffs/` and `record/` are frozen in time.** Consistent with prior handoffs. References to "rebar" or old structure in those dirs are intentional — don't bulk-rename.
- **Smoke-testing MCP without a real client.** The verification in PR #12's commit piped handcrafted JSON-RPC through stdin. `mcp-inspector` or Claude Desktop would be a more robust loop if you're making changes to the server behavior.
