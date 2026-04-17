# Agent Briefing — Fresh full Library audit — src/, tests/, Cargo.toml, README.md, and feature Gated API surface

You are in a `cased` audit output directory. This file exists to help you pick
up remediation work without thrashing. Read it once, then act.

**Audit:** `2026-04-12-Fresh full-library audit — src/, tests/, Cargo.toml, README.md, and feature-gated API surface`
**Date:** 2026-04-12
**Findings:** 11 total

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
add an entry. The `open` count is `11 - (fixed + mitigated +
accepted + disputed + deferred)`.

```markdown
---
audit: 2026-04-12-Fresh full-library audit — src/, tests/, Cargo.toml, README.md, and feature-gated API surface
last_updated: YYYY-MM-DD
status:
  fixed: 0
  mitigated: 0
  accepted: 0
  disputed: 0
  deferred: 0
  open: 11
---

# Actions Taken: Fresh full Library audit — src/, tests/, Cargo.toml, README.md, and feature Gated API surface

Summary of remediation status for the [2026-04-12 Fresh full-library audit — src/, tests/, Cargo.toml, README.md, and feature-gated API surface audit](README.md).

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

### The Completeness Surface

- `shutdown-entry-point-is-not-usable-as-documented` (significant) — `src/shutdown.rs:9-16`
- `logging-config-log-dir-is-never-read` (significant) — `README.md:182-186`
- `common-args-json-and-version-flags-are-no-ops` (significant) — `README.md:90-97`
- `otel-http-json-protocol-is-documented-but-not-supported` (moderate) — `src/otel.rs:17-20`

### The Builder Contract Surface

- `preloaded-config-builder-requires-unused-serde-bounds` (significant) — `src/lib.rs:555-626`

### The Error Handling Surface

- `cache-set-ttl-overflow` (significant) — `src/cache.rs:69-80`
- `cache-clear-drops-delete-errors` (moderate) — `src/cache.rs:145-159`
- `update-check-hides-check-failures` (moderate) — `src/update.rs:89-138`

### The Performance Surface

- `logging-layer-clones-span-fields-per-event` (significant) — `src/logging.rs:437-515`
- `http-response-body-is-copied-once-and-often-cloned-again` (moderate) — `src/http.rs:124-174`

### The Supply-Chain Surface

- `bench-gungraun-pulls-unmaintained-bincode` (advisory) — `Cargo.toml:82-83`

## If you have the `cased` skill loaded

Invoke it. The skill's Phase 5 covers remediation tracking with the full
schema reference and worked examples. This briefing exists for the case
where you land in the directory without the skill available.
