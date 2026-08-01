# Bytes-Native Cache Storage Design

**Finding:** `http-cache-entry-body-amplification`

## Decision

Replace both JSON encodings in the cache path with versioned, bytes-native
framing. There are no deployed cache entries, so the new implementation will
not read, migrate, or remove the former JSON/base64 format. Old paths simply
become cold misses.

No serialization dependency will be added. JSON remains appropriate for the
small HTTP metadata block; response bodies and general cache values remain raw
bytes throughout persistence.

## Generic Cache File Format

`Cache` writes files named `v2-{encoded-key}.cache`. Each file contains:

| Offset | Size | Value |
|---|---:|---|
| 0 | 8 bytes | Magic/version `LBRCA02\0` |
| 8 | 8 bytes | Expiry timestamp as big-endian Unix seconds |
| 16 | remaining bytes | Raw cached value |

`Cache::set` delegates to a crate-private multipart writer that accepts a slice
of byte slices. The writer emits the fixed header and each payload part through
the existing owner-only atomic file. HTTP caching can therefore write metadata
and a response body without concatenating them first.

`Cache::get` opens the file, reads and validates the 16-byte header, then reads
the remaining payload directly into the returned `Vec<u8>`. A truncated entry,
unknown magic, or malformed framing produces a new `CacheError::Format` error.
Expiry cleanup, private permissions, atomic replacement, key encoding, and TTL
saturation remain unchanged.

`remove` targets the v2 path. `clear` removes `.cache` files rather than the
obsolete `.json` extension.

## HTTP Cache Payload Format

The value passed to `Cache` uses body-first trailer framing:

| Region | Value |
|---|---|
| Body | Raw response bytes |
| Metadata | JSON containing format version 2, cache policy, status, HTTP version, headers, and trailers |
| Length | Metadata length as a big-endian `u64` |
| Trailer | Magic/version `LBRHT02\0` |

Body-first framing is deliberate. On a cache hit, the decoder validates the
16-byte footer, calculates the metadata range, deserializes that range, and
truncates the owned payload vector at the body boundary. The remaining vector
is the response body; no second body allocation or shift is required.

`CachedResponse.body` is skipped by metadata serialization and populated from
the truncated payload during decoding. The decoder rejects a missing trailer,
an impossible metadata length, a non-v2 metadata version, malformed JSON, and
invalid response fields. `load_entry` treats those failures as corrupt HTTP
cache entries, removes the file, and proceeds as a cache miss.

The HTTP key namespace changes from `http:v1:` to `http:v2:` so the format
transition is explicit even though no legacy entries need migration.

## Ownership and Write Flow

`persist_cached_response` takes an owned `CachedResponse`. It serializes
metadata by borrowing the response, passes the body and framing slices to the
multipart cache writer, and returns the same `CachedResponse` regardless of
whether persistence succeeds. The initial network path ignores the returned
value. The 304 revalidation path passes that returned value to
`fresh_response`.

This removes the unconditional second `body.clone()` while retaining the one
copy required to cache a response that must also be returned to the caller.
Errors remain best-effort and logged; a persistence failure never discards the
network response.

## Verification

Tests will establish the following contracts before implementation:

- arbitrary binary `Cache` values round-trip exactly;
- the v2 cache file is exactly 16 bytes larger than its raw value;
- invalid or truncated cache framing returns `CacheError::Format`;
- an end-to-end 1 MiB HTTP response produces a cache file no more than 16 KiB
  larger than the body, instead of the former 4.6x representation;
- cache hits, stale revalidation, headers, trailers, private permissions,
  symlink replacement, expiry, remove, and clear retain their current behavior.

Focused cache and HTTP-cache tests run first, followed by the all-feature
example/build checks, `just check`, the 21-configuration feature matrix, and
the Rust 1.89 MSRV check.
