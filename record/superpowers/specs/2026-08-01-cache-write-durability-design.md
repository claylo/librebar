# Cache Write Durability Design

## Summary

Librebar's generic filesystem cache stores disposable response data. Each write
currently pays for a durability sync even though cache loss after a crash is
safe: the next request can rebuild the entry. The credential-bearing cookie jar
has a different contract and should remain durable.

This change separates those policies. Cache entries remain private and
atomically replaceable but are not explicitly synchronized to stable storage.
Cookie writes retain one durability sync.

## Decision

Use `tempfile::NamedTempFile` for cache writes. Create the temporary file in the
target directory, write the complete cache entry, and atomically persist it over
the destination. `tempfile` already provides collision handling, cleanup, and
cross-platform replacement semantics without synchronizing the file or parent
directory.

Keep `atomic-write-file` for the cookie jar. Remove its explicit `sync_all()`
call because `AtomicWriteFile::commit()` already performs the required sync.

Two alternatives were rejected:

- Hand-written temporary-file naming and renaming would duplicate solved
  collision, cleanup, permission, and platform behavior.
- Retaining the cache sync would preserve unnecessary latency and leave
  `cache-set-fsync-per-write` unresolved.

## Feature boundaries

The optional `cache` feature enables `tempfile`. The optional `http-cookies`
feature continues to enable `atomic-write-file`. `tempfile` remains a
development dependency for tests that use it without the `cache` feature.

No public APIs, cache keys, cache entry formats, or default features change.

## Cache write path

`Cache::set` keeps its existing serialization and locking behavior. Its private
writer:

1. creates a `NamedTempFile` in the destination's parent directory;
2. applies owner-only permissions on Unix;
3. writes the header and body parts directly to the temporary file; and
4. calls `persist` to atomically replace the destination.

There is no explicit file or directory sync. A crash may lose the newest cache
entry, which is acceptable for disposable data. Atomic replacement still
prevents readers from observing a partially written entry. Existing symlinks
are replaced rather than followed, and directories remain invalid targets.

Temporary-file creation, writes, and persistence return their underlying I/O
errors. A failed persistence leaves the previous entry intact where the
platform permits and lets `NamedTempFile` clean up the temporary file.

## Cookie write path

The cookie jar continues using `AtomicWriteFile`. Buffered data is flushed, and
`commit()` performs the single durability sync before replacement. Removing the
earlier explicit `sync_all()` eliminates the duplicate barrier without weakening
the cookie durability contract.

## Verification

Focused tests retain the existing cache guarantees:

- cache entries round-trip and replace existing files atomically;
- Unix cache files remain owner-only;
- symlink targets are not followed;
- directory destinations fail;
- cookie persistence still round-trips.

Manifest checks confirm that `cache` selects `tempfile` and `http-cookies`
selects `atomic-write-file`. The full `just check` gate verifies all feature
combinations, tests, documentation, linting, and dependency policy.
