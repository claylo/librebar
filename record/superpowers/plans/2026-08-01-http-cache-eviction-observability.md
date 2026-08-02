# HTTP Cache Eviction Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every best-effort HTTP-cache eviction failure observable without changing request outcomes.

**Architecture:** Private synchronous and asynchronous eviction helpers own removal and uniform warning emission. Existing corrupt-entry, mismatch, unexpected-304, and non-storable paths call those helpers and continue their current response flow.

**Tech Stack:** Rust 2024, `tracing`, existing optional `tracing-subscriber`, Tokio, Nextest, Just.

---

### Task 1: Capture an eviction-failure warning

**Files:**
- Modify: `src/http/cache.rs:683-938`

- [x] **Step 1: Add a warning-event capture to the unit-test module**

Inside `mod tests`, add the feature-gated imports and capture types:

```rust
#[cfg(feature = "logging")]
use std::collections::BTreeMap;
#[cfg(feature = "logging")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "logging")]
use tracing::field::{Field, Visit};
#[cfg(feature = "logging")]
use tracing_subscriber::layer::SubscriberExt as _;

#[cfg(feature = "logging")]
#[derive(Clone, Default)]
struct WarningCapture(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

#[cfg(feature = "logging")]
impl<S> tracing_subscriber::Layer<S> for WarningCapture
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if *event.metadata().level() != tracing::Level::WARN {
            return;
        }
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);
        self.0.lock().unwrap().push(visitor.fields);
    }
}

#[cfg(feature = "logging")]
#[derive(Default)]
struct EventVisitor {
    fields: BTreeMap<String, String>,
}

#[cfg(feature = "logging")]
impl Visit for EventVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_owned(), format!("{value:?}"));
    }
}
```

- [x] **Step 2: Write the failing helper-contract test**

Add this test to the same module:

```rust
#[cfg(feature = "logging")]
#[test]
fn eviction_failure_is_logged_with_key_and_error() {
    use base64::Engine as _;

    let directory = tempfile::tempdir().unwrap();
    let cache = Cache::new(directory.path());
    let key = "locked";
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(namespaced_key(key));
    let entry_path = directory.path().join(format!("v2-{encoded}.cache"));
    std::fs::create_dir(&entry_path).unwrap();

    let capture = WarningCapture::default();
    let events = capture.0.clone();
    let subscriber = tracing_subscriber::registry().with(capture);
    let _subscriber_guard = tracing::subscriber::set_default(subscriber);

    evict_entry_blocking(&cache, key);

    let events = events.lock().unwrap();
    let warning = events
        .iter()
        .find(|fields| {
            fields.get("message").is_some_and(|message| {
                message.contains("failed to evict HTTP cache entry")
            })
        })
        .expect("eviction failure should emit a warning");
    assert_eq!(warning.get("key").map(String::as_str), Some(key));
    assert!(warning.get("error").is_some_and(|error| !error.is_empty()));
}
```

Using a directory as the cache entry makes `remove_file` fail on every supported platform without permission or timing assumptions.

- [x] **Step 3: Run the test and confirm RED**

Run:

```bash
cargo test --lib --features http-cache,logging http::http_cache::tests::eviction_failure_is_logged_with_key_and_error -- --exact
```

Expected: compilation fails because `evict_entry_blocking` does not exist.

### Task 2: Centralize observable best-effort eviction

**Files:**
- Modify: `src/http/cache.rs:386-555`

- [x] **Step 1: Replace the removal helper with sync and async eviction helpers**

Replace `remove_entry` with:

```rust
fn warn_eviction_failure(key: &str, error: &Error) {
    tracing::warn!(key, error = %error, "failed to evict HTTP cache entry");
}

fn evict_entry_blocking(cache: &Cache, key: &str) {
    if let Err(error) = cache.remove(&namespaced_key(key)) {
        warn_eviction_failure(key, &error);
    }
}

async fn evict_entry(cache: &Cache, key: &str) {
    let cache = cache.clone();
    let owned_key = key.to_owned();
    let result = crate::cache::run_io(move || cache.remove(&namespaced_key(&owned_key))).await;
    if let Err(error) = result {
        warn_eviction_failure(key, &error);
    }
}
```

The async helper keeps filesystem work on the blocking pool and also logs worker join failures.

- [x] **Step 2: Route every eviction through the helpers**

Replace the five bare discards with `evict_entry(cache, key).await` or `evict_entry_blocking(cache, key)` according to context. Replace both existing `if let Err(error) = remove_entry(...)` blocks with unconditional `evict_entry(cache, key).await` calls so all seven eviction paths share one warning contract.

The resulting non-storable branches are:

```rust
if policy.is_storable() {
    persist_entry(client, cache, key, &policy, &response, response_time).await;
} else {
    evict_entry(cache, key).await;
}
```

The corrupt blocking paths call:

```rust
evict_entry_blocking(cache, key);
```

No call site propagates eviction failure or changes its subsequent fetch/return behavior.

- [x] **Step 3: Run the focused tests and confirm GREEN**

Run:

```bash
cargo test --lib --features http-cache,logging http::http_cache::tests::eviction_failure_is_logged_with_key_and_error -- --exact
cargo test --test http_cache_test --features http-cache
cargo hack check --each-feature --no-dev-deps
```

Expected: the warning test, all HTTP-cache integration tests, and all 21 feature configurations pass.

### Task 3: Verify and record the remediation

**Files:**
- Modify: `record/audits/2026-08-01-00-full-repo/actions-taken.md` (never stage)
- Create: `commit.txt` (gitignored)

- [x] **Step 1: Run the repository gate**

Run:

```bash
just check
```

Expected: formatting, Clippy, dependency policy, all Nextest tests, doctests, and API documentation pass.

- [x] **Step 2: Append the audit action**

Update the front matter to `fixed: 20` and `open: 43`, then append an entry addressing `http-cache-eviction-results-discarded`. Record `pending (working tree)` until Clay commits it. State that every eviction now uses the observable best-effort helpers and include focused plus full verification results.

- [x] **Step 3: Stage the remediation, excluding the ledger**

Run the approved staging command by itself:

```bash
git --no-pager add src/http/cache.rs docs/superpowers/plans/2026-08-01-http-cache-eviction-observability.md
```

Then run read-only checks separately:

```bash
git --no-pager diff --cached --check
git --no-pager diff --cached --name-only
```

Expected: only the implementation/test module and implementation plan are staged. `record/audits/2026-08-01-00-full-repo/actions-taken.md` remains unstaged.

- [x] **Step 4: Write `commit.txt`**

Write:

```text
fix(cache): log HTTP cache eviction failures

Route every best-effort eviction through shared sync and async
helpers that preserve request outcomes while warning on failure.

Release-Note: Log HTTP cache eviction failures
Release-Impact: low
```

Clay commits with `gtxt`.
