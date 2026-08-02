# Agent Briefing — Full tracked repository: source, tests, examples, dependencies, configuration, CI/tooling, and documented public contracts

You are in a `cased` audit output directory. This file exists to help you pick
up remediation work without thrashing. Read it once, then act.

**Audit:** `2026-08-01-22-full-repo`
**Date:** 2026-08-01
**Findings:** 13 total

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
add an entry. The `open` count is `13 - (fixed + mitigated +
accepted + disputed + deferred)`.

```markdown
---
audit: 2026-08-01-22-full-repo
last_updated: YYYY-MM-DD
status:
  fixed: 0
  mitigated: 0
  accepted: 0
  disputed: 0
  deferred: 0
  open: 13
---

# Actions Taken: Full tracked repository: source, tests, examples, dependencies, configuration, CI/tooling, and documented public contracts

Summary of remediation status for the [2026-08-01 Full tracked repository: source, tests, examples, dependencies, configuration, CI/tooling, and documented public contracts audit](README.md).

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

### The Documented Logging Contract Surface

- `builder-ignores-configured-log-settings` (significant) — `README.md:583-589`
- `log-path-override-is-not-exact` (significant) — `README.md:543-549`

### The Signal Lifecycle Surface

- `detached-signal-task-outlives-app-lifecycle` (moderate) — `src/shutdown.rs:142-175`

### The Cache Concurrency Surface

- `cache-prune-can-delete-concurrent-fresh-write` (moderate) — `src/cache.rs:188-204`

### The Error Boundary Surface

- `dispatch-resolution-errors-collapse-to-not-found` (moderate) — `src/dispatch.rs:38-53`
- `crash-dump-errors-erased` (moderate) — `src/crash.rs:162-194`
- `cache-clear-reports-partial-success` (advisory) — `src/cache.rs:234-248`
- `logging-resolution-uses-string-errors` (advisory) — `src/logging.rs:188-217`

### The Public Type Contract Surface

- `release-info-allows-invalid-metadata` (advisory) — `src/update.rs:35-49`
- `http-client-is-not-cloneable` (advisory) — `src/http.rs:506-514`

### The Cookie Hot-Path Surface

- `cookie-limit-enforcement-scans-full-jar-on-every-response` (moderate) — `src/http/cookies.rs:212-255`

### The Dependency Fitness Surface

- `otel-enables-unused-default-integrations` (advisory) — `Cargo.toml:59-63`
- `diagnostics-tar-enables-unused-xattr` (advisory) — `Cargo.toml:106-108`

## If you have the `cased` skill loaded

Invoke it. The skill's Phase 5 covers remediation tracking with the full
schema reference and worked examples. This briefing exists for the case
where you land in the directory without the skill available.
