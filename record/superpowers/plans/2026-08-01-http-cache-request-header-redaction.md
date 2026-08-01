# HTTP Cache Request-Header Redaction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent arbitrary request-header values from entering serialized HTTP cache metadata while preserving cache-policy, `Vary`, and revalidation behavior.

**Architecture:** Replace the fixed sensitive-name list with a private cleartext-policy predicate. Deterministically fingerprint every other request value before constructing `CachePolicy`, then restore only fingerprinted fields from the prepared wire request before revalidation is sent. Keep the public API and v2 cache representation unchanged.

**Tech Stack:** Rust 2024, Hyper header types, `http-cache-semantics` 3, SHA-256 through the existing `sha2` dependency, Cargo tests, Just, cargo-hack

---

### Task 1: Prove unknown request credentials currently persist, then fingerprint every request field

**Files:**
- Modify: `src/http/cache.rs:5-18`
- Modify: `src/http/cache.rs:210-230`
- Modify: `src/http/cache.rs:406-417`
- Test: `src/http/cache.rs:700-735`

- [x] **Step 1: Extend the existing policy serialization test with unknown credential names**

Keep the existing function and test names for the RED step. Add three fields
that the fixed array does not recognize and assertions that their literal
values are absent:

```rust
#[test]
fn policy_view_fingerprints_request_credentials() {
    let mut request = Request::builder()
        .uri("https://example.test/private")
        .header(AUTHORIZATION, "Bearer super-secret")
        .header(PROXY_AUTHORIZATION, "Basic proxy-secret")
        .header(COOKIE, "session=also-secret")
        .header("x-api-key", "api-secret")
        .header("private-token", "private-secret")
        .header("x-opaque-credential", "opaque-secret")
        .body(())
        .unwrap();
    let response = hyper::Response::builder()
        .status(StatusCode::OK)
        .header(CACHE_CONTROL, "private, max-age=60")
        .body(())
        .unwrap();

    fingerprint_credentials(request.headers_mut());
    let policy = CachePolicy::new_options(
        &request,
        &response,
        SystemTime::now(),
        CacheOptions {
            shared: false,
            ..CacheOptions::default()
        },
    );
    let serialized = serde_json::to_string(&policy).unwrap();

    for secret in [
        "super-secret",
        "proxy-secret",
        "also-secret",
        "api-secret",
        "private-secret",
        "opaque-secret",
    ] {
        assert!(!serialized.contains(secret), "persisted request secret: {secret}");
    }
}
```

- [x] **Step 2: Run the exact unit test and verify behavioral RED**

Run:

```bash
cargo test --features http-cache --lib http::http_cache::tests::policy_view_fingerprints_request_credentials -- --exact
```

Expected: FAIL at `api-secret`; the current three-name loop leaves the unknown
field value in serialized `CachePolicy`.

- [x] **Step 3: Generalize fingerprinting with the smallest implementation**

Rename the helper to `fingerprint_request_headers` and initially fingerprint
every request field. Keep `SENSITIVE_REQUEST_HEADERS` temporarily because the
old restore helper still uses it; Task 3 removes it.

Replace `fingerprint_credentials` with:

```rust
fn fingerprint_request_headers(headers: &mut HeaderMap) {
    let names = headers.keys().cloned().collect::<Vec<_>>();
    for name in names {
        let values = headers.get_all(&name).iter().cloned().collect::<Vec<_>>();
        headers.remove(&name);
        for value in values {
            let mut hasher = Sha256::new();
            hasher.update(b"librebar-http-cache-credential\0");
            hasher.update(name.as_str().as_bytes());
            hasher.update(b"\0");
            hasher.update(value.as_bytes());
            let fingerprint = format!("sha256:{}:{:x}", name.as_str(), hasher.finalize());
            headers.append(
                name.clone(),
                HeaderValue::from_str(&fingerprint)
                    .expect("a SHA-256 fingerprint is a valid header value"),
            );
        }
    }
}
```

Update `policy_request`:

```rust
*request.headers_mut() = wire.headers().clone();
fingerprint_request_headers(request.headers_mut());
```

Rename the test to
`policy_view_fingerprints_all_non_policy_request_headers` and update its call
to `fingerprint_request_headers`.

- [x] **Step 4: Run the exact test and verify GREEN**

Run:

```bash
cargo test --features http-cache --lib http::http_cache::tests::policy_view_fingerprints_all_non_policy_request_headers -- --exact
```

Expected: PASS. All six literal credential values are absent from the
serialized policy.

---

### Task 2: Preserve only fields the cache policy must interpret

**Files:**
- Modify: `src/http/cache.rs:210-235`
- Test: `src/http/cache.rs` unit-test module

- [x] **Step 1: Add a failing test for cleartext policy fields**

Add:

```rust
#[test]
fn policy_view_keeps_cache_semantics_fields_cleartext() {
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("example.test"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("max-age=30"));
    headers.insert("pragma", HeaderValue::from_static("no-cache"));
    headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"client-v1\""));
    headers.insert(
        IF_MODIFIED_SINCE,
        HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
    );

    fingerprint_request_headers(&mut headers);

    assert_eq!(headers["host"], "example.test");
    assert_eq!(headers[CACHE_CONTROL], "max-age=30");
    assert_eq!(headers["pragma"], "no-cache");
    assert_eq!(headers[IF_NONE_MATCH], "\"client-v1\"");
    assert_eq!(
        headers[IF_MODIFIED_SINCE],
        "Wed, 21 Oct 2015 07:28:00 GMT"
    );
}
```

Extend the unit-test import:

```rust
use hyper::header::{
    CACHE_CONTROL, COOKIE, IF_MODIFIED_SINCE, IF_NONE_MATCH, LINK, SET_COOKIE,
};
```

- [x] **Step 2: Run the exact test and verify RED**

Run:

```bash
cargo test --features http-cache --lib http::http_cache::tests::policy_view_keeps_cache_semantics_fields_cleartext -- --exact
```

Expected: FAIL because the minimal Task 1 implementation fingerprints `host`
and every other field.

- [x] **Step 3: Add the cleartext-policy predicate**

Add immediately before `fingerprint_request_headers`:

```rust
fn is_cleartext_policy_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host" | "cache-control" | "pragma" | "if-none-match" | "if-modified-since"
    )
}
```

Filter the collected names:

```rust
let names = headers
    .keys()
    .filter(|name| !is_cleartext_policy_header(name))
    .cloned()
    .collect::<Vec<_>>();
```

- [x] **Step 4: Run the cleartext and persistence tests and verify GREEN**

Run:

```bash
cargo test --features http-cache --lib http::http_cache::tests::policy_view_keeps_cache_semantics_fields_cleartext -- --exact
cargo test --features http-cache --lib http::http_cache::tests::policy_view_fingerprints_all_non_policy_request_headers -- --exact
```

Expected: both PASS. Policy-critical fields remain literal; arbitrary fields
remain fingerprinted.

---

### Task 3: Restore every fingerprinted field without clobbering policy validators

**Files:**
- Modify: `src/http/cache.rs:5-18`
- Modify: `src/http/cache.rs:233-240`
- Modify: `src/http/cache.rs:495-505`
- Test: `src/http/cache.rs` unit-test module

- [x] **Step 1: Expand the restoration test to expose the fixed-name gap**

Replace `wire_credentials_replace_policy_fingerprints_before_send` with:

```rust
#[test]
fn wire_headers_replace_fingerprints_without_clobbering_policy_validators() {
    let wire = Request::builder()
        .uri("https://example.test/private")
        .header(AUTHORIZATION, "Bearer real")
        .header(PROXY_AUTHORIZATION, "Basic proxy-real")
        .header(COOKIE, "session=real")
        .header("x-api-key", "api-real")
        .header(IF_NONE_MATCH, "\"wire-validator\"")
        .body(())
        .unwrap();
    let mut policy_headers = wire.headers().clone();
    fingerprint_request_headers(&mut policy_headers);
    policy_headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"policy-validator\""));

    restore_wire_credentials(&mut policy_headers, wire.headers());

    assert_eq!(policy_headers[AUTHORIZATION], "Bearer real");
    assert_eq!(policy_headers[PROXY_AUTHORIZATION], "Basic proxy-real");
    assert_eq!(policy_headers[COOKIE], "session=real");
    assert_eq!(policy_headers["x-api-key"], "api-real");
    assert_eq!(policy_headers[IF_NONE_MATCH], "\"policy-validator\"");
    assert!(!format!("{policy_headers:?}").contains("sha256:"));
}
```

- [x] **Step 2: Run the exact test and verify RED**

Run:

```bash
cargo test --features http-cache --lib http::http_cache::tests::wire_headers_replace_fingerprints_without_clobbering_policy_validators -- --exact
```

Expected: FAIL because the fixed three-name restore helper leaves `x-api-key`
fingerprinted.

- [x] **Step 3: Generalize wire restoration through the same predicate**

Replace `restore_wire_credentials` with:

```rust
fn restore_wire_headers(policy_headers: &mut HeaderMap, wire_headers: &HeaderMap) {
    for name in wire_headers
        .keys()
        .filter(|name| !is_cleartext_policy_header(name))
    {
        policy_headers.remove(name);
        for value in wire_headers.get_all(name) {
            policy_headers.append(name.clone(), value.clone());
        }
    }
}
```

Update the production call in `revalidate`:

```rust
restore_wire_headers(wire_revalidation.headers_mut(), wire_request.headers());
```

Update the unit-test call to `restore_wire_headers`.

Delete `SENSITIVE_REQUEST_HEADERS` and remove `AUTHORIZATION` and
`PROXY_AUTHORIZATION` from the production import. Add those two constants to
the unit-test-only import instead:

```rust
use hyper::header::{
    AUTHORIZATION, CACHE_CONTROL, COOKIE, IF_MODIFIED_SINCE, IF_NONE_MATCH, LINK,
    PROXY_AUTHORIZATION, SET_COOKIE,
};
```

- [x] **Step 4: Run the exact restoration test and verify GREEN**

Run:

```bash
cargo test --features http-cache --lib http::http_cache::tests::wire_headers_replace_fingerprints_without_clobbering_policy_validators -- --exact
```

Expected: PASS. All fingerprinted values are restored, while the policy's
`If-None-Match` value survives.

---

### Task 4: Lock down deterministic `Vary` behavior

**Files:**
- Test: `src/http/cache.rs` unit-test module

- [x] **Step 1: Add a regression test for fingerprinted `Vary` fields**

This is a preservation test: deterministic equality should remain unchanged by
the redaction boundary.

```rust
#[test]
fn vary_matches_fingerprinted_request_headers() {
    let now = SystemTime::now();
    let mut stored_request = Request::builder()
        .uri("https://example.test/private")
        .header("x-api-key", "profile-a")
        .body(())
        .unwrap();
    fingerprint_request_headers(stored_request.headers_mut());
    let response = hyper::Response::builder()
        .status(StatusCode::OK)
        .header(CACHE_CONTROL, "private, max-age=60")
        .header("vary", "x-api-key")
        .body(())
        .unwrap();
    let policy = CachePolicy::new_options(
        &stored_request,
        &response,
        now,
        CacheOptions {
            shared: false,
            ..CacheOptions::default()
        },
    );

    let mut same = Request::builder()
        .uri("https://example.test/private")
        .header("x-api-key", "profile-a")
        .body(())
        .unwrap();
    fingerprint_request_headers(same.headers_mut());
    let mut different = Request::builder()
        .uri("https://example.test/private")
        .header("x-api-key", "profile-b")
        .body(())
        .unwrap();
    fingerprint_request_headers(different.headers_mut());

    assert!(matches!(
        policy.before_request(&same, now),
        BeforeRequest::Fresh(_)
    ));
    assert!(matches!(
        policy.before_request(&different, now),
        BeforeRequest::Stale { matches: false, .. }
    ));
}
```

- [x] **Step 2: Run the new unit test and the existing end-to-end `Vary` test**

Run:

```bash
cargo test --features http-cache --lib http::http_cache::tests::vary_matches_fingerprinted_request_headers -- --exact
cargo test --features http-cache --test http_cache_test vary_mismatch_fetches_and_replaces_the_entry -- --exact
```

Expected: both PASS. Equal values reuse the stored representation; different
values force a miss even though neither literal value is persisted.

---

### Task 5: Document identity-aware cache keys and run focused verification

**Files:**
- Modify: `src/http.rs:523-530`
- Verify: `src/http/cache.rs`
- Verify: `tests/http_cache_test.rs`

- [x] **Step 1: Clarify the public cache-key contract**

Replace the opening `get_cached` documentation paragraph with:

```rust
/// A key identifies one stored representation. Include tenant, locale,
/// media-type, and requesting-identity distinctions whenever credentials may
/// select a different representation and the origin does not declare that
/// distinction with `Vary`. Cache files can contain complete API responses
/// and are therefore written as private data.
```

- [x] **Step 2: Format and run focused HTTP-cache verification**

Run:

```bash
just fmt
cargo test --features http-cache --lib http::http_cache::tests
cargo test --features http-cache --test http_cache_test
cargo check --no-default-features --features http-cache
```

Expected: formatting succeeds; all HTTP-cache unit and integration tests pass;
the isolated feature compiles without warnings.

- [x] **Step 3: Check that production request values cross only the new boundary**

Run:

```bash
rg -n "SENSITIVE_REQUEST_HEADERS|fingerprint_credentials|restore_wire_credentials" src/http/cache.rs
rg -n "fingerprint_request_headers|restore_wire_headers|is_cleartext_policy_header" src/http/cache.rs
```

Expected: the first command returns no matches. The second reports the three
helpers and their production/test call sites.

---

### Task 6: Run repository gates and prepare the remediation handoff

**Files:**
- Modify: `record/audits/2026-08-01-00-full-repo/actions-taken.md`
- Include: `record/superpowers/specs/2026-08-01-http-cache-request-header-redaction-design.md`
- Include: `record/superpowers/plans/2026-08-01-http-cache-request-header-redaction.md`
- Create: `commit.txt` (gitignored)

- [x] **Step 1: Run every repository gate**

Run:

```bash
just check
just feature-matrix
RUSTUP_TOOLCHAIN=1.89.0 just msrv-check
```

Expected: formatting, Clippy, cargo-deny, nextest, doctests, docs, all 21
feature configurations, and Rust 1.89 compilation pass.

- [x] **Step 2: Record the Cased action without staging the ledger**

Update the front matter to `fixed: 17` and `open: 46`. Append a `fixed` entry
for `http-cache-persists-unrecognized-credential-headers` with
`Commit: pending (working tree)`. Record the behavioral RED, cleartext-policy
RED, restoration RED, focused test counts, and full verification results. Keep
every preceding ledger entry intact.

- [x] **Step 3: Stage only implementation, design, and plan artifacts**

Run:

```bash
git --no-pager add src/http/cache.rs src/http.rs record/superpowers/specs/2026-08-01-http-cache-request-header-redaction-design.md record/superpowers/plans/2026-08-01-http-cache-request-header-redaction.md
git --no-pager diff --cached --check
git --no-pager diff --cached --name-only
```

Expected: exactly the four listed files are staged. The audit ledger remains
unstaged.

- [x] **Step 4: Write the `gtxt` commit message**

Create gitignored `commit.txt` with:

```text
fix(http): fingerprint cached request headers

Persist only the request fields needed by cache policy semantics and replace
all other values with deterministic SHA-256 fingerprints. Preserve Vary and
revalidation behavior while documenting identity-aware cache keys.

Release-Note: Prevent request credentials entering HTTP cache metadata
Release-Impact: medium
```

- [x] **Step 5: Review the handoff**

Run:

```bash
git --no-pager diff --cached
git --no-pager status --short
git check-ignore -v commit.txt
```

Expected: only the implementation, spec, and plan are staged; the audit
directory remains unstaged; `commit.txt` is ignored and ready for `gtxt`.
