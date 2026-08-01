# Cookie Jar Limits Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bound live and persisted cookie jars with configurable RFC-aligned ceilings.

**Architecture:** Add an immutable `CookieLimits` policy to each `CookieJar`, reject oversized response cookies before storage, and deterministically prune loaded or updated stores by nearest expiry. Wire the policy through `HttpClientBuilder` without changing `HttpClientConfig`.

**Tech Stack:** Rust 2024, `cookie_store` 0.22, Hyper, tracing, Cargo Hack, nextest/Just.

---

### Task 1: Define and wire the public policy

**Files:**
- Modify: `src/http.rs`
- Modify: `src/http/cookies.rs`

- [x] **Step 1: Write compile-time and behavior tests**

Add unit coverage constructing `CookieLimits::default()` and raising each
ceiling through its fluent setters. Add an integration client build using:

```rust
HttpClient::builder("test", "0.0.0")
    .cookie_limits(CookieLimits::default().max_cookies_per_domain(75))
    .with_cookie_jar()
    .build()?;
```

- [x] **Step 2: Run the focused tests and verify failure**

Run: `cargo test --features http-cookies http::cookies::tests`

Expected: compilation fails because `CookieLimits` and `cookie_limits` do not
exist.

- [x] **Step 3: Add the policy type and builder wiring**

Define `CookieLimits` under `http-cookies` with private fields, `Default`,
fluent setters, and read-only accessors. Store it in `HttpClientBuilder` and
pass it to both fresh and loaded jar constructors.

- [x] **Step 4: Run the focused tests**

Run: `cargo test --features http-cookies http::cookies::tests`

Expected: policy construction and client builder tests pass.

### Task 2: Enforce size and count ceilings

**Files:**
- Modify: `src/http/cookies.rs`

- [x] **Step 1: Write failing limit regressions**

Add tests demonstrating that the current jar retains an oversized cookie,
more than the configured number for one domain, more than the configured total
across domains, and excess cookies loaded from JSON.

- [x] **Step 2: Run the focused tests and verify failure**

Run: `cargo test --features http-cookies http::cookies::tests`

Expected: every limit regression fails because all cookies remain present.

- [x] **Step 3: Implement minimal deterministic pruning**

Reject incoming cookies when `name.len() + value.len()` exceeds the configured
byte ceiling. Collect removable keys from `iter_unexpired()`, sort dated
cookies before session cookies by expiry and then domain/path/name, and use
`CookieStore::remove` until per-domain and total counts meet their ceilings.
Run the same policy immediately after loading a jar. Emit warning fields that
identify the cookie and reason without its value.

- [x] **Step 4: Run focused and integration tests**

Run:

```bash
cargo test --features http-cookies http::cookies::tests
cargo test --test http_cookies_test --features http-cookies
```

Expected: all cookie-limit and existing cookie tests pass.

### Task 3: Verify and prepare the remediation

**Files:**
- Modify: `record/audits/2026-08-01-00-full-repo/actions-taken.md`
- Create: `commit.txt`

- [x] **Step 1: Run feature and repository gates**

Run:

```bash
cargo hack check --each-feature --no-dev-deps
just check
```

Expected: every feature configuration, test, lint, format, dependency-policy,
and documentation gate passes.

- [x] **Step 2: Record and stage the remediation**

Append one fixed entry for `cookie-jar-accepts-unbounded-cookie-count`, update
the ledger status to 25 fixed and 38 open, stage implementation, tests, design,
and plan files, leave `actions-taken.md` unstaged, and write the conventional
commit message to `commit.txt`.
