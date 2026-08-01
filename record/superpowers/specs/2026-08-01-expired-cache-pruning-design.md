# Expired Cache Entry Pruning Design

**Finding:** `cache-has-no-eviction-outside-per-key-reads`

## Decision

Add an explicit `Cache::prune()` operation and invoke it opportunistically
from the existing write path. A cache handle sweeps before its first write and
no more than once per hour afterward. This removes expired entries whose keys
are never read again without adding a background task or an O(n) directory
walk to every write.

This remediation is deliberately time-bounded rather than size-bounded. It
does not impose entry-count or byte ceilings, because silently evicting live
entries would change the public cache's retention contract without an existing
configuration API or application-specific capacity requirements.

## Public API

Add the following synchronous method:

```rust
pub fn prune(&self) -> Result<usize>
```

The return value is the number of expired entries removed. A missing cache
directory is an empty cache and returns `Ok(0)`. Failure to open the cache
directory is returned through the existing cache error. Individual entries
that cannot be inspected or removed are logged and skipped so one damaged or
permission-constrained file cannot prevent the rest of the sweep.

Document on `Cache` that entries are removed when read after expiry, during an
explicit prune, and opportunistically from active write paths. Also document
that the cache has no count or byte ceiling.

## Sweep Scope

`prune()` considers only files owned by the current framing convention:
filenames beginning with `v2-` and ending in `.cache`. Unrelated files and
directories are untouched.

The sweep opens each candidate and reads only the fixed 16-byte cache header.
It validates the v2 magic and decodes the expiry timestamp without allocating
or reading the cached value. A valid entry is removed when the supplied current
Unix time is greater than or equal to its expiry. Live entries remain in place.
Malformed entries are logged and left for the existing read-path error policy;
corrupt-entry eviction is outside this finding.

Refactor the existing full-entry reader to share the header decoder so the
framing rules have one implementation.

## Opportunistic Maintenance

`Cache` gains a shared atomic Unix timestamp recording the last successful
sweep. The type becomes cheaply `Clone`; clones share the timestamp through an
`Arc`. `Cache::new` and `Cache::default_for` keep their signatures and initialize
the timestamp so the first write attempts a sweep.

Before `set_parts` writes an entry, it compares the current time with the last
sweep. One writer claims a due sweep with an atomic compare-and-exchange. Other
writers proceed without duplicating the scan. A successful explicit or
automatic prune updates the shared timestamp. If an automatic sweep fails, it
restores the prior timestamp so a later write may retry.

Automatic pruning is maintenance, not part of storing the requested value.
Its errors are logged and do not fail `set` or `set_parts`; the subsequent
write retains its existing error behavior. The one-hour cadence is a private
constant, not a new tuning surface.

The HTTP cache's three blocking-I/O adapters clone the caller's `Cache`
instead of reconstructing one from `cache.dir()`. This preserves the shared
sweep cadence across asynchronous reads, removals, and writes. Update checks
already move one cache handle through each blocking operation and need no API
change.

## Concurrency

The shared timestamp prevents redundant opportunistic scans within one cache
handle family. It is intentionally process-local: a newly started process
attempts one sweep on its first write, which also cleans entries left by prior
processes.

This remediation does not introduce a lock file, sidecar index, or background
thread. It retains the cache's existing path-based best-effort unlink behavior;
the separately audited concurrent replacement race remains scoped to
`cache-expiry-unlink-races-concurrent-write`.

## Verification

Add regression coverage proving that:

- explicit pruning removes an expired entry that was never read;
- explicit pruning retains live entries and unrelated files;
- pruning a missing directory returns zero;
- the first write through a new cache handle prunes an older expired entry;
- cloned handles share the sweep timestamp and do not rescan within the
  one-hour interval;
- malformed candidates are retained without blocking removal of other expired
  entries.

Retain the existing framing, permissions, symlink replacement, and HTTP cache
tests. Run the focused cache tests, the complete repository checks, all feature
configurations, and the Rust 1.89 MSRV check.

## Non-Goals

- No maximum entry count or maximum byte size.
- No eviction of live entries.
- No least-recently-used or oldest-expiry ordering.
- No persistent index, expiry-bearing filename, or sidecar sweep marker.
- No background thread or Tokio task.
- No new dependency, Cargo feature, or cache format version.
- No remediation of concurrent path replacement in this change.
