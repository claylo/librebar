---
audit: 2026-04-11-full-crate
last_updated: 2026-04-11
status:
  fixed: 3
  mitigated: 0
  accepted: 1
  disputed: 0
  deferred: 0
  open: 0
---

# Actions Taken: rebar Full Crate Audit

Summary of remediation status for the [2026-04-11 full crate audit](report.html).

---

## 2026-04-11 — Restore warning-free build on the full feature matrix

**Disposition:** fixed
**Addresses:** [diagnostics-test-api-drift](report.html#diagnostics-test-api-drift), [config-tests-ignore-deep-merge-errors](report.html#config-tests-ignore-deep-merge-errors)
**Commit:** pending (`fix/audit-2026-04-11-remediation` branch)
**Author:** Clay Loveless

Both test-drift findings were cleared in the same pass because they gate the same validation command.

In `tests/diagnostics_test.rs`, the stale `.unwrap()` on `bundle.add_text("info.txt", "test content")` was removed — the method now returns `&mut Self` for builder-style chaining rather than `Result<()>`. That one trailing `.unwrap()` was enough to stop the entire all-features test compile with `error[E0599]: no method named 'unwrap' found for mutable reference '&mut DebugBundle'`, which is why the finding was rated significant.

In `tests/config_test.rs`, all six `deep_merge()` call sites were updated to `.unwrap()` the `Result<()>`, clearing the six `unused Result that must be used` warnings. A new `merge_rejects_excessive_depth` negative test was added that constructs 65 matching object layers in **both** base and overlay and asserts `rebar::Error::ConfigMergeDepth`. The symmetry matters: `merge_inner` only increments depth through the `(Object, Object)` match arm. An asymmetric test (empty base, deep overlay) would short-circuit via the scalar-replace branch at depth 1 and never trip the guard, so naming the new-contract behavior required matching structure on both sides.

**Verification:** `just check` end-to-end — `fmt` → `clippy` (`--all-features -D warnings`) → `deny` → `test` (88 passed / 0 failed under nextest with all features) → `doc-test` — is clean for the first time in rebar's history. See the fourth entry below for why "for the first time in rebar's history" is literally true rather than a figure of speech.

**Note on `tests/update_test.rs::suppressed_by_env_var`.** The test would fail under plain threaded `cargo test --workspace --all-features` because two sibling tests in the same integration binary share process state: one calls `std::env::set_var("TEST_APP_NO_UPDATE_CHECK", "1")` while another calls `remove_var` on the same name, and threaded parallelism races them. The test's own `// SAFETY: nextest runs each test in its own process` comment declares reliance on process isolation. Under the canonical runner (`cargo nextest run`), each test binary gets its own process and the assertion holds. This note is preserved for anyone considering migrating to plain `cargo test` — don't, without first moving the env probe off shared state (`serial_test` guard or a non-process-env lookup).

---

## 2026-04-11 — Bump rand to patched 0.9.3 via `cargo update`

**Disposition:** fixed
**Addresses:** [otel-feature-pulls-rand-rustsec-warning](report.html#otel-feature-pulls-rand-rustsec-warning)
**Commit:** pending (`fix/audit-2026-04-11-remediation` branch)
**Author:** Clay Loveless

RUSTSEC-2026-0097 was fixed upstream in `rand 0.9.3` (the patch release on the `0.9` line). rebar had `rand 0.9.2` in `Cargo.lock` via `opentelemetry_sdk 0.31.0`. `cargo update -p rand` bumped the lockfile entry to `rand 0.9.3` without touching any `Cargo.toml` specifier — `opentelemetry_sdk`'s transitive requirement was already `^0.9`, so the patched version resolved inside the existing constraint.

After the bump, `cargo audit` reports only the bincode warning; the rand advisory is gone. The full nextest matrix still passes.

Worth noting per the audit's root-cause reading: RUSTSEC-2026-0097's unsoundness (aliased `&mut BlockRng<ReseedingCore>`) only manifests when (a) both the `log` and `thread_rng` features are enabled, (b) a custom logger implementation calls `rand::rng()` from a log emission path, and (c) `ThreadRng` reseeds mid-generation. rebar's `otel` feature forwards tracing through `tracing-opentelemetry`'s non-custom pipeline, so none of those preconditions applied — which is why `cargo deny` treated this as a warning, not a failure. Fixing it anyway was a free lockfile bump and the right call: a foundation crate's advertised `otel` feature should not ship "warning-free build" overpromises.

---

## 2026-04-11 — Accept bincode 1.3.3 advisory for the `bench-gungraun` feature

**Disposition:** accepted
**Addresses:** [bench-gungraun-pulls-unmaintained-bincode](report.html#bench-gungraun-pulls-unmaintained-bincode)
**Commit:** pending (`fix/audit-2026-04-11-remediation` branch)
**Author:** Clay Loveless

The audit's remediation suggestion was: "check whether a newer `gungraun` release removes the `bincode` edge. If not, document the feature as carrying an accepted benchmark-only warning..." Checked: the newer release (`gungraun 0.18.1`) does **not** remove the edge — its workspace `Cargo.toml` on `main` still pins `bincode = { version = "1" }`, confirmed by inspecting the public repository. bincode 1.3.3 was permanently abandoned after a doxxing incident; no patched 1.x version exists and bincode 2.x is a separate project, not a drop-in replacement. The migration path is upstream-only.

Accepted for these reasons:

1. **No runtime impact on default crate consumers.** `bench-gungraun` is an opt-in dev feature (instruction-count benchmarks via Valgrind/Callgrind). It is not in any default feature set; rebar's library users never reach this path.
2. **The usage is benign.** gungraun uses bincode to serialize benchmark result data between its runner and harness, not to parse untrusted input.
3. **Advisory category is "unmaintained," not "vulnerability."** RUSTSEC-2025-0141 flags abandonment, not an exploit chain.
4. **`just deny` does not even reach this edge.** cargo-deny's default scan is feature-gated; `bench-gungraun` is optional, so the gate never encounters the bincode advisory in its resolved graph. A runtime `ignore` entry would therefore produce a spurious `advisory-not-detected` warning against the default gate, which is why the acceptance is documented as a prose block under `[advisories]` in `.config/deny.toml` rather than as a runtime suppression.

The rationale block in `.config/deny.toml` is the authoritative record for when to revisit: gungraun migrating off bincode 1.x, or rebar swapping benchmark harnesses entirely.

**Decision:** Clay Loveless, library maintainer. Accepted at the 2026-04-11 review; revisit at the next dependency review or immediately if any of the revisit conditions are met.

---

## 2026-04-11 — Fix `just test` to actually run the feature-gated matrix

**Disposition:** fixed
**Addresses:** none (latent verification gap, exposed during audit remediation; in spirit with the Feature-Matrix Verification Surface narrative)
**Commit:** pending (`fix/audit-2026-04-11-remediation` branch)
**Author:** Clay Loveless

Uncovered while verifying the above findings: `just test` ran `cargo nextest run` with **no feature flags and no workspace flag**. Because rebar has no default features (`Cargo.toml` has no `default = [...]` entry) and every integration test file is gated with `#![cfg(feature = "…")]`, the recipe compiled and ran **zero tests** and exited with nextest's "no tests to run" error (exit code 4). `just check` — which runs `fmt clippy deny test doc-test` in sequence — therefore silently skipped all test verification. The only gate that was actually walking the test surface was `just clippy`, because its recipe explicitly passes `--all-features`.

This is a latent gap that has been in place since `70b945d` (the initial crate commit), not a regression introduced by recent changes. It was invisible because nobody was reading the "0 tests run" summary as a failure — and until this audit surfaced the `cargo test --workspace --all-features` drift, there was no reason to reach for the broader invocation. The result is that the audit's significance rating on `diagnostics-test-api-drift` was actually an **understatement** — the project had been shipping without continuous test verification for its entire lifetime, not just since the DebugBundle API change.

Updated both recipes to run the full matrix:

```just .justfile
test:
  cargo nextest run --workspace --all-features

test-ci:
  cargo nextest run --workspace --all-features --profile ci
```

**Verification:** `just test` now compiles and runs **88 integration tests** (0 failed, 0 skipped). `just check` end-to-end passes cleanly: fmt → clippy → deny → test (88/88) → doc-test. The update_test env-var probe passes because nextest provides process-per-test isolation.

This change is folded into the audit remediation commit because the audit's thesis — *restoring trustworthy all-features verification* — applies directly. Without this fix, the audit's "fixed" findings on the other three slugs would still not be continuously verified by the project's own gate. With it, they are.
