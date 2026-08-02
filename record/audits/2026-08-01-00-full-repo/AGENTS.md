# Agent Briefing — Full repository audit at 8fd83b3 — all 19 cargo features, 24 src modules, 19 integration test files, 9 examples

You are in a `cased` audit output directory. This file exists to help you pick
up remediation work without thrashing. Read it once, then act.

**Audit:** `2026-08-01-00-full-repo`
**Date:** 2026-08-01
**Findings:** 63 total

## Files in this directory

- `README.md`        — authored narrative report (markdown, GitHub-rendered companion to report.html). Read-only for remediation work.
- `report.html`      — interactive rendered report (primary deliverable). Read-only.
- `findings.yaml`    — structured findings (source for the build). Read-only.
- `recon.yaml`       — structural model. Read-only.
- `assets/`          — generated sparkline SVGs. Don't edit.
- `actions-taken.md` — append-only remediation ledger. May not exist yet;
  create it the first time you log an action.
- `AGENTS.md`        — this file.

## The loop

For each finding you address:

1. Find it in `README.md` or `report.html` by its slug. Anchors match the slug
   exactly; every finding is pre-listed in the index below so you don't need
   to grep.
2. Read the concern, location, and remediation text.
3. Make the code change in the target repository.
4. Append one entry to `actions-taken.md`. **One entry per action**, even
   when a single action resolves multiple findings — put every slug it
   addresses in the `Addresses` field.

## `actions-taken.md` format

YAML front matter plus chronological markdown entries. Front matter is
mandatory; update `last_updated` and the `status` counts every time you
add an entry. The `open` count is `63 - (fixed + mitigated +
accepted + disputed + deferred)`.

```markdown
---
audit: 2026-08-01-00-full-repo
last_updated: YYYY-MM-DD
status:
  fixed: 0
  mitigated: 0
  accepted: 0
  disputed: 0
  deferred: 0
  open: 63
---

# Actions Taken: Full repository audit at 8fd83b3 — all 19 cargo features, 24 src modules, 19 integration test files, 9 examples

Summary of remediation status for the [2026-08-01 Full repository audit at 8fd83b3 — all 19 cargo features, 24 src modules, 19 integration test files, 9 examples audit](README.md).

---

## YYYY-MM-DD — brief description of the action

**Disposition:** fixed
**Addresses:** [finding-slug](README.md#finding-slug)
**Commit:** {SHA or PR link}
**Author:** {who did the work}

One to three paragraphs describing what changed, in which files, and why
this approach. If the disposition is `accepted` or `disputed`, the rationale
must be here. If `deferred`, include the target date or milestone.
```

## Dispositions

- `fixed` — code change deployed; commit SHA or PR link required
- `mitigated` — compensating control in place; root cause remains; explain
  the residual risk
- `accepted` — risk acknowledged; rationale mandatory (who decided, why).
  This is not a euphemism for "ignored"
- `disputed` — finding contested with evidence; not a dismissal. The
  original finding stays in `README.md`; this entry records the counterargument
- `deferred` — scheduled for later; target date or milestone reference
  required. A deferred finding without a target is an accepted finding in
  disguise

## What you must not do

- Do not edit `README.md`, `report.html`, `findings.yaml`, `recon.yaml`, or
  anything in `assets/`. They are the audit artifact and must stay immutable.
- Do not edit past `actions-taken.md` entries. The file is append-only. If
  a previous action is superseded, add a new entry referencing the old one.
- Do not invent finding slugs. Use the ones in the index below, verbatim.
- Do not create an empty `actions-taken.md` until you have at least one
  action to log.

## Finding index

Every finding in this audit. Use these exact slugs in the `Addresses` field
of your `actions-taken.md` entries.

### The Release Boundary Surface

- `ci-builds-only-all-features` (significant) — `.github/workflows/ci.yml:68-72`
- `docs-rs-publishes-only-default-features` (significant) — `Cargo.toml:144-163`
- `no-attested-publish-path` (moderate) — `.config/scrat.toml:1-8`
- `readme-code-blocks-outside-the-doc-test-gate` (advisory) — `.justfile:35-40`
- `ci-reimplements-justfile-recipes` (note) — `.github/workflows/ci.yml:82-92`
- `unresolved-intra-doc-error-links` (note) — `src/cache.rs:66-71`
- `missing-contributing-and-status-badges` (note) — `README.md:1-3`

### The Diagnostics and Disclosure Surface

- `debug-bundle-ships-unredacted-content-world-readable` (significant) — `src/diagnostics.rs:211-215`
- `request-uri-with-credentials-recorded-in-log-spans` (significant) — `src/http.rs:639-643`
- `crash-dump-world-readable-and-unbounded` (moderate) — `src/crash.rs:120-133`
- `debug-bundle-buffers-entire-archive-in-memory` (moderate) — `src/diagnostics.rs:192-221`
- `response-debug-impl-exposes-body-and-set-cookie` (advisory) — `src/http/response.rs:57-63`
- `crash-dumps-documented-as-json-are-free-text` (advisory) — `src/crash.rs:50-61`
- `debug-bundle-builder-cannot-be-chained` (moderate) — `src/diagnostics.rs:210-231`

### The HTTP Cache Surface

- `http-cache-entry-body-amplification` (significant) — `src/http/cache.rs:561-584`
- `blocking-fsync-on-async-cache-paths` (significant) — `src/http/cache.rs:421-444`
- `http-cache-persists-unrecognized-credential-headers` (moderate) — `src/http/cache.rs:17-18`
- `cache-has-no-eviction-outside-per-key-reads` (moderate) — `src/cache.rs:99-126`
- `cache-set-fsync-per-write` (moderate) — `src/cache.rs:178-190`
- `http-cache-eviction-results-discarded` (advisory) — `src/http/cache.rs:337-347`
- `cache-expiry-unlink-races-concurrent-write` (note) — `src/cache.rs:109-119`

### The Transport and Cookie Surface

- `cookie-jar-never-installs-public-suffix-list` (significant) — `src/http/cookies.rs:20-23`
- `cross-origin-redirect-forwards-non-blocklisted-credentials` (significant) — `src/http.rs:365-371`
- `cookie-jar-failures-are-silent` (moderate) — `src/http/cookies.rs:77-91`
- `cookie-jar-accepts-unbounded-cookie-count` (advisory) — `src/http/cookies.rs:104-115`
- `webpki-root-store-is-compiled-in` (note) — `Cargo.toml:65`

### The Process Lifecycle Surface

- `crash-hook-print-turns-panics-into-aborts` (significant) — `src/crash.rs:101-113`
- `signal-task-exits-after-first-signal` (significant) — `src/shutdown.rs:69-96`
- `ctrl-c-registration-error-triggers-shutdown` (moderate) — `src/shutdown.rs:79-93`
- `print-macros-panic-where-errors-cannot-propagate` (moderate) — `src/otel.rs:100-106`
- `retry-counter-decrement-relies-on-caller-invariant` (note) — `src/http.rs:896-902`

### The Telemetry Surface

- `otel-batch-processor-cannot-drive-hyper-exporter` (significant) — `src/otel.rs:139-153`
- `otel-http-json-protocol-not-buildable` (moderate) — `src/otel.rs:156-175`
- `log-event-clones-span-field-map` (moderate) — `src/logging.rs:399-409`
- `otel-config-env-var-name-fields-unread` (advisory) — `src/otel.rs:45-51`
- `otel-grpc-feature-has-no-test` (note) — `src/otel.rs:160-166`
- `log-writability-probe-creates-unused-file` (note) — `src/logging.rs:309-322`

### The Public API Surface

- `unreachable-dependency-types-in-public-api` (significant) — `src/lib.rs:198-206`
- `dependency-error-payloads-are-unwrappable` (significant) — `src/error.rs:191-208`
- `growable-public-structs-lack-non-exhaustive` (significant) — `src/http.rs:130-148`
- `update-checker-hardcodes-github-and-its-collaborators` (significant) — `src/update.rs:91-109`
- `environment-source-trait-over-constrains-implementors` (moderate) — `src/config/environment.rs:9-13`
- `doctor-check-registration-forces-caller-boxing` (advisory) — `src/diagnostics.rs:96-110`
- `schema-wire-types-are-serialize-only` (advisory) — `src/cli/schema.rs:133-145`

### The Error Architecture Surface

- `error-variants-drop-the-source-chain` (moderate) — `src/error.rs:73-106`
- `lock-error-message-misreports-the-cause` (moderate) — `src/lockfile.rs:107-112`
- `error-display-duplicates-its-source` (advisory) — `src/error.rs:88-96`
- `update-check-drops-errors-it-documents-as-logged` (advisory) — `src/update.rs:106-117`
- `lockfile-exclusion-guarantee-unqualified` (advisory) — `src/lockfile.rs:95-120`
- `lockfile-falls-back-to-shared-tmp-on-linux` (advisory) — `src/lockfile.rs:30-36`

### The Supply Chain Surface

- `serde-saphyr-exact-pin-on-default-path` (significant) — `Cargo.toml:38-40`
- `ring-is-the-sole-c-asm-island` (note) — `Cargo.toml:64-66`
- `bans-multiple-versions-warn-only` (advisory) — `.config/deny.toml:57-62`
- `license-allowlist-stale-entries` (note) — `.config/deny.toml:14-24`
- `base64-simd-unsafe-optout-holds` (note) — `Cargo.toml:79-83`
- `advisory-suppressions-removed-after-cause-cleared` (note) — `.config/deny.toml:42-51`

### The Configuration Discovery Surface

- `git-boundary-marker-inert-when-search-root-is-repo-root` (moderate) — `src/config.rs:466-472`
- `config-discovery-stat-fanout` (advisory) — `src/config.rs:442-478`

### The Dispatch and Self-Update Surface

- `dispatch-resolves-binary-from-current-directory` (significant) — `src/dispatch.rs:36-39`
- `cached-version-string-interpolated-into-release-url` (advisory) — `src/update.rs:143-146`

### The Verification Apparatus Surface

- `bench-apparatus-measures-nothing` (moderate) — `tests/bench_test.rs:1-8`
- `unsafe-escape-hatch-rationale-does-not-match-use` (advisory) — `Cargo.toml:165-168`
- `cargo-profiles-do-not-reach-consumers` (note) — `Cargo.toml:188-217`

## If you have the `cased` skill loaded

Invoke it. The skill's Phase 5 covers remediation tracking with the full
schema reference and worked examples. This briefing exists for the case
where you land in the directory without the skill available.
