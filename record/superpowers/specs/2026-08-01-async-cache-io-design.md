# Non-Blocking Async Cache I/O Design

**Finding:** `blocking-fsync-on-async-cache-paths`

## Decision

Keep the public `Cache` API synchronous. Move every cache operation reached
from `HttpClient::get_cached` and `UpdateChecker::check` onto Tokio's blocking
pool through one crate-private helper. This preserves the standalone `cache`
feature, adds no public API, and requires no new dependency or Tokio feature.

`Cache::get`, `Cache::set`, `Cache::remove`, and `Cache::clear` remain ordinary
synchronous filesystem methods for non-async callers.

## Execution Boundary

Add a crate-private helper in `src/cache.rs`, compiled only when `http-cache`
or `update` is enabled:

```rust
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

The feature gate is valid because both `http-cache` and `update` include the
existing `http` feature, which already enables Tokio with `rt`. The helper
does not compile for the standalone `cache` feature, so that feature's
dependency boundary is unchanged.

Callers create an owned cache handle with `Cache::new(cache.dir())` before
moving it into a `'static` blocking closure. This uses the existing public
constructor and avoids adding `Clone` to the public `Cache` type.

## HTTP Cache Flow

All filesystem work under `get_cached` crosses the private helper:

- Cache loading, binary decoding, validation, and corrupt-entry cleanup run
  together in one blocking closure.
- Eviction and non-storable-entry removal use a private async removal helper.
- HTTP metadata serialization and multipart persistence run together in one
  blocking closure. The owned `CachedResponse` moves into that closure, so the
  body remains single-copy after the preceding bytes-native remediation.
- The caller awaits persistence. Request latency still includes the durable
  write, but the runtime worker remains available to timers, signal handling,
  and unrelated tasks.

The blocking persistence function returns its owned `CachedResponse` even
when serialization or filesystem persistence fails, preserving the existing
best-effort write behavior. A blocking-task join failure is different: it
means the task panicked or could not be joined, so the async wrapper returns
the existing crate error type. On an origin response this is logged and the
network response is still returned; on a 304 revalidation it propagates
because the cached response moved into the failed task.

## Update Check Flow

`UpdateChecker::check` keeps its public signature and best-effort contract.
The default cache is created as before, but reads and writes run through
`run_io` with owned keys and values. Cache failures continue to fall through
to the network or leave the result uncached; this remediation does not absorb
the separate audit finding about which update-check failures are logged.

The existing statement that update checking is non-blocking becomes accurate
for filesystem work: network I/O remains async and cache I/O runs on Tokio's
blocking pool.

## Durability

Remove the explicit `file.sync_all()` immediately before
`AtomicWriteFile::commit()`. Version 0.3.0 of `atomic-write-file` calls
`sync_all()` inside `commit()` before the atomic rename. Retaining both calls
issues two durability barriers for the same payload; relying on `commit()`
keeps one barrier and the same rename ordering.

Existing owner-only permissions, atomic replacement, and error propagation
remain unchanged.

## Error Handling

The private helper converts `tokio::task::JoinError` into
`CacheError::Io(std::io::Error::other(error))`. `io::Error::other` retains the
join error as its source, so the existing `Error::Cache` chain remains useful
without adding a public error variant solely for an internal task boundary.

HTTP cache reads still propagate ordinary filesystem failures. Best-effort
writes and removals still log failures at their existing call sites. Update
cache failures retain their current best-effort behavior.

## Verification

Add a deterministic regression test around `run_io` using a Tokio
current-thread runtime. The blocking closure signals that it started and waits
on a standard channel; a Tokio timer must fire before the test releases the
closure. Without `spawn_blocking`, the timer cannot be polled and the test
fails after a bounded outer-thread timeout. With the helper, the timer fires
while the blocking worker waits.

Retain and rerun the generic-cache, HTTP-cache, and update integration suites.
Run the complete repository gates, all 21 feature configurations, and the
Rust 1.89 MSRV check. No timing benchmark is required: the regression proves
runtime responsiveness, while the removed duplicate `sync_all()` follows the
dependency's documented implementation contract.

## Non-Goals

- No public async cache methods or async cache trait.
- No fire-and-forget cache writes.
- No change to cache file framing, expiry, or eviction policy.
- No change to update-check logging policy beyond what this finding requires.
- No dependency or feature additions.
