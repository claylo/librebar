# Non-Blocking Async Cache I/O Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep every filesystem cache operation reached from librebar's async HTTP-cache and update-check paths off Tokio runtime workers, while retaining the synchronous public `Cache` API and one durability sync per write.

**Architecture:** Add one feature-gated, crate-private `run_io` helper that delegates owned closures to `tokio::task::spawn_blocking`. Route HTTP-cache reads, writes, removals, decoding, and serialization through that boundary; route update-check reads and writes through it as well. Preserve existing best-effort behavior and use `AtomicWriteFile::commit()` as the single sync-and-rename operation.

**Tech Stack:** Rust standard library channels and filesystem APIs, Tokio `spawn_blocking` and current-thread runtime, `atomic-write-file` 0.3.0, existing integration tests, Just, cargo-hack.

---

### Task 1: Establish the private blocking-pool boundary

**Files:**
- Modify: `src/cache.rs`

- [x] **Step 1: Add the current-thread runtime regression test**

Append this unit-test module after `default_cache_dir` in `src/cache.rs`:

```rust
#[cfg(all(test, any(feature = "http-cache", feature = "update")))]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn blocking_io_does_not_stall_current_thread_runtime() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (timer_tx, timer_rx) = mpsc::sync_channel(1);

        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let blocking = super::run_io(move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                });
                let timer = async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    timer_tx.send(()).unwrap();
                };

                let (result, ()) = tokio::join!(blocking, timer);
                result.unwrap();
            });
        });

        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let timer_ran = timer_rx.recv_timeout(Duration::from_millis(250)).is_ok();
        release_tx.send(()).unwrap();
        worker.join().unwrap();

        assert!(timer_ran, "cache I/O stalled the current-thread runtime");
    }
}
```

The outer test thread owns the timeout and releases the simulated filesystem
operation even on failure. A synchronous implementation therefore fails
cleanly instead of hanging the test process.

- [x] **Step 2: Run the regression test and verify RED**

Run:

```bash
cargo test --features http-cache --lib cache::tests::blocking_io_does_not_stall_current_thread_runtime -- --exact
```

Expected: compilation fails with `E0425` because `run_io` does not exist yet.

- [x] **Step 3: Add the deliberately synchronous baseline helper**

Insert this minimal helper after the `impl Cache` block and before
`read_entry`:

```rust
#[cfg(any(feature = "http-cache", feature = "update"))]
pub(crate) async fn run_io<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    operation()
}
```

This is intentionally the current synchronous behavior behind the wished-for
API. It exists only long enough to prove the test detects runtime starvation.

- [x] **Step 4: Run the regression test and verify behavioral RED**

Run:

```bash
cargo test --features http-cache --lib cache::tests::blocking_io_does_not_stall_current_thread_runtime -- --exact
```

Expected: the test fails after the bounded outer-thread wait with
`cache I/O stalled the current-thread runtime`.

- [x] **Step 5: Replace the baseline with the blocking-pool helper**

Replace the synchronous `run_io` body with:

```rust
#[cfg(any(feature = "http-cache", feature = "update"))]
pub(crate) async fn run_io<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| CacheError::Io(std::io::Error::other(error)))?
}
```

Do not add `Clone` to `Cache` and do not change any public method. The helper
must remain absent when only the standalone `cache` feature is enabled.

- [x] **Step 6: Run the regression test and verify GREEN**

Run:

```bash
cargo test --features http-cache --lib cache::tests::blocking_io_does_not_stall_current_thread_runtime -- --exact
```

Expected: the single test passes. The timer reports progress before the outer
thread releases the blocking closure.

- [x] **Step 7: Verify the standalone cache feature remains independent**

Run:

```bash
cargo check --no-default-features --features cache
```

Expected: compilation succeeds without compiling the Tokio-backed helper.

---

### Task 2: Offload the complete HTTP-cache filesystem path

**Files:**
- Modify: `src/http/cache.rs`
- Test: `tests/http_cache_test.rs` (existing coverage)

- [x] **Step 1: Move cache loading and removal behind async wrappers**

Change the time import at the top of `src/http/cache.rs`:

```rust
use std::time::{Duration, SystemTime};
```

Replace the synchronous `load_entry` with this async wrapper and synchronous
blocking implementation:

```rust
async fn load_entry(cache: &Cache, key: &str) -> Result<Option<CachedHttpEntry>> {
    let cache = Cache::new(cache.dir());
    let key = key.to_owned();
    crate::cache::run_io(move || load_entry_blocking(&cache, &key)).await
}

fn load_entry_blocking(cache: &Cache, key: &str) -> Result<Option<CachedHttpEntry>> {
    let namespaced = namespaced_key(key);
    let bytes = match cache.get(&namespaced) {
        Ok(bytes) => bytes,
        Err(Error::Cache(CacheError::Format(error))) => {
            tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
            let _ = cache.remove(&namespaced);
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let Some(bytes) = bytes else {
        return Ok(None);
    };

    match decode_entry(bytes) {
        Ok(entry) => Ok(Some(entry)),
        Err(error) => {
            tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
            let _ = cache.remove(&namespaced);
            Ok(None)
        }
    }
}

async fn remove_entry(cache: &Cache, key: &str) -> Result<()> {
    let cache = Cache::new(cache.dir());
    let namespaced = namespaced_key(key);
    crate::cache::run_io(move || cache.remove(&namespaced)).await
}
```

Update the initial read in `get_cached`:

```rust
    let Some(entry) = load_entry(cache, key).await? else {
        return fetch_and_maybe_store(client, cache, key, wire_request).await;
    };
```

Replace both direct removals in `get_cached` with awaited private removals:

```rust
                    let _ = remove_entry(cache, key).await;
```

```rust
            let _ = remove_entry(cache, key).await;
```

- [x] **Step 2: Move serialization and multipart writes behind the boundary**

Replace `persist_entry` and `persist_cached_response` with these three
functions:

```rust
async fn persist_entry(
    client: &HttpClient,
    cache: &Cache,
    key: &str,
    policy: &CachePolicy,
    response: &Response,
    now: SystemTime,
) {
    match CachedResponse::try_from(response) {
        Ok(cached) => {
            if let Err(error) = persist_cached_response(
                cache,
                key,
                policy,
                cached,
                now,
                client.config().http_cache_stale_retention,
            )
            .await
            {
                tracing::warn!(key, error = %error, "failed to persist HTTP cache entry");
            }
        }
        Err(error) => tracing::warn!(key, error = %error, "failed to encode HTTP cache response"),
    }
}

async fn persist_cached_response(
    cache: &Cache,
    key: &str,
    policy: &CachePolicy,
    response: CachedResponse,
    now: SystemTime,
    stale_retention: Duration,
) -> Result<CachedResponse> {
    let cache = Cache::new(cache.dir());
    let key = key.to_owned();
    let policy = policy.clone();
    crate::cache::run_io(move || {
        Ok(persist_cached_response_blocking(
            &cache,
            &key,
            &policy,
            response,
            now,
            stale_retention,
        ))
    })
    .await
}

fn persist_cached_response_blocking(
    cache: &Cache,
    key: &str,
    policy: &CachePolicy,
    response: CachedResponse,
    now: SystemTime,
    stale_retention: Duration,
) -> CachedResponse {
    let entry = CachedHttpEntry {
        format_version: HTTP_CACHE_FORMAT_VERSION,
        policy: policy.clone(),
        response,
    };
    let metadata = match serde_json::to_vec(&entry) {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::warn!(key, error = %error, "failed to serialize HTTP cache entry");
            return entry.response;
        }
    };
    let metadata_len = (metadata.len() as u64).to_be_bytes();
    let ttl = policy.time_to_live(now).saturating_add(stale_retention);
    let parts: [&[u8]; 4] = [
        entry.response.body.as_slice(),
        metadata.as_slice(),
        &metadata_len,
        HTTP_CACHE_MAGIC,
    ];
    if let Err(error) = cache.set_parts(&namespaced_key(key), &parts, ttl) {
        tracing::warn!(key, error = %error, "failed to persist HTTP cache entry");
    }
    entry.response
}
```

This keeps JSON serialization, raw-body writes, and the durability barrier on
the blocking worker. Ordinary serialization/write failures still hand the
owned cached response back to the caller.

- [x] **Step 3: Await every HTTP persistence and eviction call site**

In `fetch_and_maybe_store`, replace the storage branch with:

```rust
    if policy.is_storable() {
        persist_entry(client, cache, key, &policy, &response, response_time).await;
    } else if let Err(error) = remove_entry(cache, key).await {
        tracing::warn!(key, error = %error, "failed to remove non-storable HTTP cache entry");
    }
```

In the `AfterResponse::NotModified` branch, replace persistence with:

```rust
            let cached = persist_cached_response(
                cache,
                key,
                &policy,
                cached,
                response_time,
                client.config().http_cache_stale_retention,
            )
            .await?;
```

In the `AfterResponse::Modified` branch, replace the unexpected-304 removal:

```rust
                let _ = remove_entry(cache, key).await;
```

Replace the final storage/removal branch with:

```rust
            if policy.is_storable() {
                persist_entry(client, cache, key, &policy, &response, response_time).await;
            } else if let Err(error) = remove_entry(cache, key).await {
                tracing::warn!(key, error = %error, "failed to remove non-storable HTTP cache entry");
            }
```

Use `rg` to prove no synchronous cache operation remains in an async HTTP
function outside the explicitly blocking implementations:

```bash
rg -n "cache\.(get|set|set_parts|remove)" src/http/cache.rs
```

Expected: matches occur only in `load_entry_blocking`, `remove_entry`'s
`run_io` closure, and `persist_cached_response_blocking`.

- [x] **Step 4: Run focused HTTP-cache verification**

Run:

```bash
cargo test --features http-cache --test http_cache_test
cargo test --features http-cache --lib http::http_cache::tests
cargo check --no-default-features --features http-cache
```

Expected: all 15 HTTP-cache integration tests and 6 HTTP-cache unit tests
pass; the isolated `http-cache` feature compiles without warnings.

---

### Task 3: Offload update-check cache operations

**Files:**
- Modify: `src/update.rs`
- Test: `tests/update_test.rs` (existing coverage)

- [x] **Step 1: Route the cache read through `run_io`**

Replace the cache-read chain in `UpdateChecker::check` with:

```rust
        if let Some(cache) = crate::cache::Cache::default_for(&self.app_name)
            && let Ok(Some(cached)) =
                crate::cache::run_io(move || cache.get(CACHE_KEY)).await
            && let Ok(version) = String::from_utf8(cached)
        {
            tracing::debug!(cached_version = %version, "using cached version check");
            return self.compare_versions(&version);
        }
```

The owned `Cache` moves directly into the closure; no public clone operation
is needed.

- [x] **Step 2: Route the cache write through `run_io`**

Replace the best-effort cache write with:

```rust
        if let Some(cache) = crate::cache::Cache::default_for(&self.app_name) {
            let latest = latest.as_bytes().to_vec();
            let _ = crate::cache::run_io(move || cache.set(CACHE_KEY, &latest, CACHE_TTL)).await;
        }
```

Keep the result ignored. Logging discarded update-check failures is tracked by
the separate `update-check-drops-errors-it-documents-as-logged` finding.

- [x] **Step 3: Clarify the non-blocking documentation**

Replace the `check` method's behavior paragraph with:

```rust
    /// This does not block the async runtime and is best-effort. Network
    /// errors, GitHub rate limits, and parse failures are logged at debug
    /// level and return `None`.
```

- [x] **Step 4: Run focused update verification**

Run:

```bash
cargo test --features update --test update_test
cargo check --no-default-features --features update
```

Expected: all 5 update integration tests pass and the isolated `update`
feature compiles without warnings.

---

### Task 4: Remove the redundant durability barrier and verify the repository

**Files:**
- Modify: `src/cache.rs`

- [x] **Step 1: Remove the explicit pre-commit sync**

Change the end of `write_entry` from:

```rust
    file.sync_all()?;
    file.commit()
```

to:

```rust
    file.commit()
```

`AtomicWriteFile::commit()` in version 0.3.0 calls `sync_all()` before the
atomic rename. Do not replace it with `flush()` and do not alter owner-only
mode handling.

- [x] **Step 2: Format and rerun cache-focused tests**

Run:

```bash
just fmt
cargo test --features cache --test cache_test
cargo test --features http-cache --lib cache::tests::blocking_io_does_not_stall_current_thread_runtime -- --exact
```

Expected: all 13 generic-cache integration tests and the current-thread
runtime regression pass after formatting.

- [x] **Step 3: Run every repository gate**

Run:

```bash
just check
just feature-matrix
RUSTUP_TOOLCHAIN=1.89.0 just msrv-check
```

Expected: 230 nextest tests and 37 doctests pass, all 21 cargo-hack feature
configurations pass, and all targets compile on Rust 1.89.

---

### Task 5: Record and prepare the remediation handoff

**Files:**
- Modify: `record/audits/2026-08-01-00-full-repo/actions-taken.md`
- Include: `record/superpowers/specs/2026-08-01-async-cache-io-design.md`
- Include: `record/superpowers/plans/2026-08-01-async-cache-io.md`
- Create: `commit.txt` (gitignored)

- [x] **Step 1: Record the Cased action without staging the ledger**

Update the front matter to `fixed: 16` and `open: 47`. Append a `fixed`
entry for `blocking-fsync-on-async-cache-paths` with `Commit: pending
(working tree)`. Record the current-thread RED/GREEN result, the removal of
the duplicate sync, focused suite counts, and the full verification results.
Keep every preceding ledger entry intact.

- [x] **Step 2: Stage only implementation and design artifacts**

Run:

```bash
git --no-pager add src/cache.rs src/http/cache.rs src/update.rs record/superpowers/specs/2026-08-01-async-cache-io-design.md record/superpowers/plans/2026-08-01-async-cache-io.md
git --no-pager diff --cached --check
git --no-pager diff --cached --name-only
```

Expected: exactly the five listed files are staged. The audit ledger remains
unstaged.

- [x] **Step 3: Write the `gtxt` commit message**

Create gitignored `commit.txt` with:

```text
fix(cache): keep filesystem I/O off async workers

Run HTTP-cache and update-check filesystem operations on Tokio's blocking
pool. Preserve the synchronous public cache API and rely on the atomic writer
for the single durability sync before rename.

Release-Note: Keep async cache operations from blocking runtime tasks
Release-Impact: medium
```

- [x] **Step 4: Review the handoff**

Run:

```bash
git --no-pager diff --cached
git --no-pager status --short
git check-ignore -v commit.txt
```

Expected: only the implementation, spec, and plan are staged; the audit
directory remains unstaged; `commit.txt` is ignored and ready for `gtxt`.
