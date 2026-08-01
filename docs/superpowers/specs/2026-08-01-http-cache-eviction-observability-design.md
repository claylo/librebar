# HTTP Cache Eviction Observability Design

## Summary

Librebar treats HTTP cache eviction as best-effort so cleanup failures never
turn a successful network response into an application error. Several eviction
paths currently discard `Cache::remove` failures, while two equivalent paths
warn. That inconsistency can hide the reason a corrupt or obsolete entry keeps
surviving and being reprocessed.

This change makes every HTTP-cache eviction failure observable without changing
request outcomes.

## Decision

Add private synchronous and asynchronous eviction helpers in
`src/http/cache.rs`. Each helper removes the namespaced entry and emits the same
warning with the caller-visible cache key and underlying error if removal fails.
Every corrupt-entry, representation-mismatch, unexpected-304, and non-storable
response path uses those helpers.

The helpers return `()` deliberately. Eviction remains best-effort, but the
discard moves behind a named boundary that records failure rather than leaving
bare `let _ =` expressions at call sites.

Two alternatives were rejected:

- Propagating removal errors would let disposable-cache cleanup override a
  usable network response, contradicting the established non-fatal cache policy.
- Adding comments beside each discard would document intent but leave operators
  unable to diagnose persistent corrupt entries.

## Components and flow

The synchronous helper serves `load_entry_blocking`, where malformed framing or
decoded metadata is discovered on the blocking worker. The asynchronous helper
wraps the existing `run_io` boundary for request-time eviction paths. Both use a
shared private warning function so message text and fields cannot drift.

The flow remains:

1. determine that a stored representation must be removed;
2. attempt removal off the async runtime when necessary;
3. warn with `key` and `error` if removal fails; and
4. continue the existing miss, refetch, or response-return path.

Missing entries remain successful because `Cache::remove` already treats
`NotFound` as `Ok(())`.

## Error behavior

The warning message is `failed to evict HTTP cache entry`. It records the
original caller key rather than the internal namespaced key and includes the
complete display chain supplied by Librebar's error type. No new public error
variant or API is introduced.

Worker join failures and filesystem removal failures receive the same treatment:
both are observable and neither changes the request result.

## Testing

A focused unit test creates a directory at the exact cache-entry path so
`remove_file` reliably fails without platform-specific permission changes. A
small in-test tracing layer captures warning events. The test first demonstrates
the missing observable eviction boundary, then verifies that the helper records
the warning message, caller key, and error field.

Existing HTTP-cache integration tests continue to cover corrupt-entry recovery,
non-storable responses, cache-write failures, and successful request outcomes.
The final verification runs the focused unit tests, all feature configurations,
and `just check`.
