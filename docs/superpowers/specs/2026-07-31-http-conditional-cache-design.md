# HTTP Response Metadata and Conditional Cache Design

## Summary

librebar's HTTP client currently returns a status number and buffered body. It
throws away the response version, headers, and trailers. That makes it
unsuitable for serious API work and prevents correct conditional requests.

This change makes response metadata lossless, adds first-class ETag and
Last-Modified validators, and composes the existing HTTP client with the
filesystem cache through `http-cache-semantics`. It supports three related jobs:

```rust
client.get_if_modified(url, &validator)
client.check_modified(url, &validator)
client.get_cached(&cache, key, url)
```

The first two APIs remain in the `http` feature. Persistent HTTP caching lives
behind a new `http-cache` feature.

## Decision

Three approaches were considered:

1. Hand-write freshness and revalidation rules. Rejected—HTTP caching is full
   of protocol edge cases, and librebar shouldn't become their newest author.
2. Adopt a complete Tower caching middleware. Rejected—available middleware
   owns storage and request flow in ways that fight librebar's explicit
   per-call `Cache` API and lossless response contract.
3. Compose `http-cache-semantics` with librebar's HTTP and cache layers.
   Selected—the dependency owns RFC decisions while librebar owns persistence,
   credentials, and caller-visible response fidelity.

This is an **extend** decision. The semantics engine is worth adopting, but its
response reconstruction is lossy, so using it untouched would violate the core
requirement.

## Goals

- Preserve the final response's status, HTTP version, headers, repeated header
  values, trailers, and body.
- Treat ETag as the primary validator and Last-Modified as the fallback.
- Let callers distinguish modified, not modified, and indeterminate responses
  without guessing from a status integer.
- Make a zero-body conditional `HEAD` check available when the caller only
  needs to know whether something changed.
- Honor HTTP cache rules instead of inventing another TTL cache protocol.
- Reuse the existing explicit `Cache` object and caller-supplied cache key.
- Keep cached API data private, atomic, and resilient to corrupt entries.

## Non-goals

- Automatically cache every request made by `HttpClient`.
- Cache POST, PUT, PATCH, DELETE, or partial `206` responses.
- Support multiple stored representations beneath one caller cache key.
- Add a cached escape hatch for arbitrary custom requests in this slice.
- Add response-body streaming.
- Add rate limiting or upstream `Retry-After` handling. That work remains in
  `scratch/TODO.txt`.
- Implement a second HTTP caching standard library inside librebar.

## Feature boundary and dependencies

The new feature is explicit:

```toml
http-cache = [
    "http",
    "cache",
    "dep:http-cache-semantics",
    "dep:sha2",
]
```

`http-cache-semantics` provides freshness calculation, cacheability,
`Vary` matching, conditional request construction, and revalidation decisions.
It runs with `CacheOptions { shared: false, ..Default::default() }`, because
librebar is building a private user-agent cache rather than a shared proxy.

`sha2` provides deterministic credential fingerprints for serialized policy
state. `atomic-write-file`, already used by `http-cookies`, is also enabled by
the `cache` feature so every cache entry gets the same atomic private-write
behavior. Both features can enable the same optional dependency independently.
The `http-cache` feature is not part of librebar's default features.

## Response model

`Response` becomes a body plus private metadata. The public API returns Hyper's
HTTP types rather than lossy local substitutes.

```rust
pub struct ResponseMetadata {
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    trailers: Option<HeaderMap>,
}

pub struct Response {
    metadata: ResponseMetadata,
    body: Vec<u8>,
    #[cfg(feature = "http-cache")]
    cache_status: Option<CacheStatus>,
}
```

The module re-exports `HeaderMap`, `HeaderValue`, `StatusCode`, and `Version`
alongside its existing `Method` and `Request` re-exports.

`Response` exposes:

```rust
response.status()       // StatusCode
response.version()      // Version
response.headers()      // &HeaderMap
response.header(ETAG)   // Option<&HeaderValue>
response.trailers()     // Option<&HeaderMap>
response.into_parts()   // (ResponseMetadata, Vec<u8>)

// Available with `http-cache`:
response.cache_status() // Option<CacheStatus>
```

`ResponseMetadata` exposes the same status, version, header, and trailer
accessors. Conditional outcomes can therefore inspect metadata without
reconstructing a body-bearing `Response`.

The existing `bytes`, `text`, `text_ref`, `into_text`, `json`, and
`is_success` helpers remain. Body collection records trailer frames instead of
discarding them. If more than one trailer frame arrives, values are appended in
wire order.

This is an intentional breaking change: `response.status` becomes
`response.status()` and returns `StatusCode` instead of `u16`. Public fields
stay private so librebar can extend response metadata without another break.

## Validators and conditional requests

`Validator` stores the server's opaque header values exactly:

```rust
pub struct Validator {
    etag: Option<HeaderValue>,
    last_modified: Option<HeaderValue>,
}
```

It provides constructors for an ETag or Last-Modified value, accessors for both,
and `Validator::from_headers`. A validator cannot be empty.
`Response::validator()` returns `Option<Validator>` and retains both values when
the response supplies both.

Standalone conditional helpers choose `If-None-Match` when an ETag exists and
fall back to `If-Modified-Since` only when it does not. The cache path delegates
validator construction to `http-cache-semantics`, which may combine validators
when the RFC permits it.

The GET outcome preserves the body whenever one exists:

```rust
pub enum ConditionalResponse {
    Modified(Response),
    NotModified(ResponseMetadata),
    Indeterminate(Response),
}
```

The HEAD outcome never exposes a body:

```rust
pub enum ModificationCheck {
    Modified(ResponseMetadata),
    NotModified(ResponseMetadata),
    Indeterminate(ResponseMetadata),
}
```

The mapping is deliberately conservative:

- `304 Not Modified` becomes `NotModified`.
- Any `2xx` response becomes `Modified`.
- Every other HTTP status becomes `Indeterminate`.
- Transport, timeout, TLS, redirect, and body-read failures remain errors.

`check_modified` sends a conditional `HEAD`. It does not silently retry with
`GET` when the server returns `405` or `501`; those responses are
`Indeterminate`. The entire operation remains inside the client's existing
whole-request timeout and retry rules.

## Persistent cache entry

The cache stores one versioned envelope per caller key:

```rust
struct CachedHttpEntry {
    policy: CachePolicy,
    response: CachedResponse,
}

struct CachedResponse {
    status: StatusCode,
    version: Version,
    headers: LosslessHeaders,
    trailers: Option<LosslessHeaders>,
    body: Vec<u8>,
}
```

`LosslessHeaders` serializes every header value as opaque bytes. It preserves
the order of repeated values for a field and round-trips values that aren't
UTF-8. The key is namespaced and versioned internally so a generic cache entry
cannot be mistaken for an HTTP envelope.

There is one representation per caller key. The policy still enforces `Vary`.
When a request doesn't match the stored representation, librebar fetches the
new representation and replaces the entry. Callers that intentionally need
coarse variants—tenant, locale, media type—put that distinction in the key.

## Lossless adapter around `http-cache-semantics`

The dependency gets the protocol decisions right, but its generated response
parts aren't lossless. Version 3.0.0 rebuilds cached headers with
`HeaderMap::insert`, which collapses repeated fields. librebar must never expose
those generated parts directly.

The adapter keeps the complete `CachedResponse` as the source of caller-visible
metadata:

- On a fresh hit, it starts with the stored multi-value headers, removes
  hop-by-hop fields, and applies the policy's calculated `Age`, `Date`, and
  warning updates without collapsing unrelated fields.
- On `304`, it uses the policy's revalidation decision, performs the required
  field replacement against the lossless stored headers, retains the cached
  body and trailers, and rebuilds the next policy from the merged response.
  The rebuilt response keeps the stored representation status—normally
  `200`—rather than persisting `304` as the representation's status.
- On a modified response, it stores the complete new response and constructs a
  new policy from it.

This adapter doesn't decide freshness, cacheability, validator strength, or
`Vary` matching. Those remain the dependency's job. It only prevents the
dependency's internal representation from throwing away caller data.

## `get_cached` flow

`get_cached(&cache, key, url)` performs a GET with an empty caller body and the
same user-agent, cookie, redirect, decompression, retry, and timeout behavior as
`get`.

1. **Missing entry:** send GET. Return the response with `CacheStatus::Miss`.
   Store it only when the policy says it is storable.
2. **Fresh matching entry:** make no network request. Return the stored body and
   losslessly updated metadata with `CacheStatus::Hit`.
3. **Stale matching entry:** send the policy's conditional GET.
4. **Valid `304`:** merge metadata, retain the stored body and trailers, update
   the policy, and return `CacheStatus::Revalidated`.
5. **Modified response:** replace the response and policy when storable. Return
   `CacheStatus::Miss` because the caller received a network representation.
6. **Non-matching representation:** fetch normally and replace the old entry if
   the new response is storable.
7. **Non-storable response:** remove an obsolete entry, return the network
   response with `CacheStatus::Miss`, and persist nothing.

Responses from `get`, `get_if_modified`, `check_modified`, and `send` have no
cache status. `CacheStatus` therefore needs only `Hit`, `Miss`, and
`Revalidated`; absence means the cache wasn't involved.

## Freshness and disk retention

The policy owns HTTP freshness. The filesystem cache's TTL controls how long a
stale representation remains available for revalidation.

By default, an HTTP entry is retained until:

```text
policy time-to-live + 7 days
```

A successful revalidation writes the entry with a newly calculated retention
deadline. With `http-cache`, `HttpClientBuilder` exposes
`http_cache_stale_retention(Duration)`. Its default is seven days; zero removes
the entry as soon as it becomes stale and therefore disables conditional
revalidation from disk. TTL addition is saturating.

Retention never authorizes serving stale content by itself. A stale entry is
only useful as a validator and saved body after a valid `304`, unless
`http-cache-semantics` says the request explicitly permits a stale response.

## Cookies and policy request views

Cache policy must see the request that would actually reach the origin. Before
consulting the policy, librebar finalizes the user-agent and cookie headers.
Cookie changes therefore participate in `Vary: Cookie` matching.

Policy serialization creates a security trap: `CachePolicy` clones the request
header map. librebar maintains two request views:

- The **wire request** contains the real credentials and is the only request
  sent to the network.
- The **policy request** replaces each `Authorization`, `Proxy-Authorization`,
  and `Cookie` value with a domain-separated SHA-256 fingerprint.

Header presence, multiplicity, and equality remain available to the policy,
but credential values never enter the serialized policy. When the dependency
returns revalidation request parts, librebar restores sensitive fields from the
wire request before sending it. A fingerprint must never cross the network.

Response metadata—including `Set-Cookie`—remains intact. Cached response bodies,
headers, and full URIs may contain sensitive application data, so cache files
are private rather than pretending the cache contains no secrets.

## Filesystem safety and failure behavior

`Cache::set` switches from direct `std::fs::write` to atomic replacement. On
Unix, new cache files use mode `0600` and don't preserve a destination's broader
mode. The behavior matches cookie-jar persistence:

- A symlink at the destination is replaced rather than followed.
- A directory at the destination is an error.
- The temporary file is flushed and committed atomically.

The generic `Cache::set` API still reports write failures. `get_cached` treats a
write failure after a successful network request as non-fatal: it logs a
warning and returns the response. Losing the response because its optimization
failed would be perverse.

A malformed or unsupported HTTP envelope is treated as a corrupt cache entry:
librebar warns, removes it best-effort, and fetches normally. Ordinary cache I/O
errors such as permission failures remain errors instead of being silently
hidden. Missing and retention-expired entries remain normal misses.

## Interaction with the existing HTTP stack

Caching wraps a complete logical GET, not an individual transport attempt.
Redirects and retries remain real upstream calls governed by the existing
client. The cached response represents the final response after redirect and
decompression processing. Validators and cache policy therefore operate on the
headers exposed by that final service response.

Cookies received during an actual network response continue to update the
client's cookie jar. Returning a fresh cached response does not replay its
`Set-Cookie` fields into the jar.

## Verification

Tests use deterministic local TCP servers except for the repository's existing
ignored network smoke tests.

### Response metadata

- Preserve status, HTTP version, ordinary headers, non-UTF-8 values, repeated
  values, body bytes, and trailers.
- Append values from multiple trailer frames without overwriting them.
- Preserve final response metadata through redirects and decompression.
- Keep all existing body helpers and success classification working.

### Conditional requests

- Extract both ETag and Last-Modified without normalizing either value.
- Prefer `If-None-Match`; fall back to `If-Modified-Since`.
- Map `200`, `304`, `404`, and `500` into the three explicit outcomes.
- Prove `check_modified` sends HEAD, transfers no response body, and never
  falls back to GET on `405` or `501`.

### Persistent caching

- Cover miss, fresh hit, stale revalidation, valid `304` merge, changed body
  replacement, `Vary` mismatch, and non-storable response removal.
- Preserve repeated headers across fresh hits and `304` merges—the dependency
  bug that motivated the lossless adapter gets a permanent regression test.
- Verify seven-day stale retention, configurable retention, and zero retention.
- Exercise redirect, retry, decompression, and cookie behavior around cached
  requests.
- Confirm raw request `Authorization`, `Proxy-Authorization`, and `Cookie`
  values never appear in serialized policy state and a fingerprint never
  appears on the wire.
- Confirm direct requests have no cache status and cached requests report Hit,
  Miss, or Revalidated correctly.

### Cache persistence and feature gates

- Verify atomic replacement, Unix `0600`, symlink replacement, directory
  refusal, corrupt-entry recovery, and non-fatal HTTP-cache write failure.
- Run feature combinations for `http`, `cache`, `http-cache`, and
  `http-cookies,http-cache`.
- Update API compile tests for the intentional `status()` migration.

## Documentation and example

The HTTP module docs show raw metadata, validator extraction, conditional GET,
conditional HEAD, and persistent caching. They call out that cached content may
contain secrets and that a caller key identifies one stored representation.

A focused `http-cache` example performs a cached API GET, prints the resulting
cache status and validator, and demonstrates origin-controlled freshness plus
validator-based revalidation. `examples/README.md` documents the required
feature and the first-run/miss, fresh-hit, and revalidated states.

The updater keeps its explicit 24-hour application policy. Update checks and
general HTTP caching solve different freshness problems; changing one into the
other would be a behavioral regression disguised as an example cleanup. It is
updated only for the `status()` API migration.

## Acceptance criteria

The work is complete when:

- Direct and conditional network responses preserve every received metadata
  field. Cached hits preserve every end-to-end field and omit only hop-by-hop
  fields that can't be replayed without their original connection.
- A repeated header survives serialization, a fresh hit, and a `304` merge.
- Conditional helpers never claim that an error response proves modification.
- `check_modified` never downloads the representation body.
- Fresh cached responses avoid the network; stale responses revalidate using
  server validators; valid `304` responses reuse the stored body.
- Serialized policy state contains no raw request `Authorization`,
  `Proxy-Authorization`, or `Cookie` value.
- Cache writes are atomic and private, and cache-write failure can't destroy a
  successful network result.
- The complete repository check passes with all features enabled.
