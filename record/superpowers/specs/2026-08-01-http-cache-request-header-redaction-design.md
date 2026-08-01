# HTTP Cache Request-Header Redaction Design

**Finding:** `http-cache-persists-unrecognized-credential-headers`

## Decision

Persist request-header values by exception, not by guessing which names may
carry credentials. Before a request enters `http_cache_semantics::CachePolicy`,
retain cleartext values only for the small set of fields that the policy engine
must interpret. Replace every other request-header value with the existing
domain-separated SHA-256 fingerprint.

This keeps `CachePolicy`'s RFC behavior intact, preserves deterministic `Vary`
comparisons, and prevents present or future cached request paths from writing
arbitrary credential-bearing header values to disk. It adds no dependency and
does not change the public API.

## Cleartext Policy Fields

The following request fields remain cleartext because
`http-cache-semantics` parses, compares, removes, or combines their values:

- `host`
- `cache-control`
- `pragma`
- `if-none-match`
- `if-modified-since`

Every other request field is fingerprinted, including `authorization`,
`proxy-authorization`, `cookie`, `x-api-key`, `private-token`, vendor-specific
security tokens, and names that do not look credential-shaped at all. The
allowlist is deliberately about cache-policy semantics, not an attempted
catalog of secret names.

The existing fingerprint remains:

```text
sha256:<lowercase-header-name>:<hex-digest>
```

The digest input remains the domain separator, lowercase header name, a zero
byte, and the exact header-value bytes. Repeated fields remain repeated and in
their original order.

## Request and Persistence Flow

`policy_request` continues to clone the prepared wire request. It then applies
`fingerprint_request_headers`, which walks the cloned header map and replaces
every value whose name is not accepted by `is_cleartext_policy_header`.

`CachePolicy::new_options` and `CachePolicy::before_request` therefore see the
same deterministic policy view. A response that varies on a fingerprinted
field still behaves correctly: equal wire values produce equal fingerprints,
and different wire values produce different fingerprints. The serialized
policy contains the fingerprint rather than the original field value.

Request `cache-control` and `pragma` remain parseable, `host` matching remains
literal, and validators that the dependency combines or creates remain
available to its revalidation logic. Other conditional and range fields retain
their names and are restored before sending, so presence checks and wire
behavior remain intact without persisting their values.

## Revalidation

Rename `restore_wire_credentials` to `restore_wire_headers`. Before sending a
stale request, replace only fingerprinted header names with the corresponding
values from the prepared wire request. Cleartext policy fields are left alone
so validators added or combined by `http-cache-semantics` are not overwritten.

The restore step uses the same `is_cleartext_policy_header` predicate as the
fingerprinting step. That single predicate defines both sides of the boundary
and prevents the two operations from drifting apart.

## Public Contract

Extend `HttpClient::get_cached` documentation to state that callers must include
the requesting identity in the explicit cache key when credentials can select
different representations and the origin does not send an appropriate `Vary`
header. This applies today to cookie-jar identities and to any future cached
custom-request API.

The public method signature does not change. No configurable redaction list is
introduced; allowing callers to weaken the persistence invariant would defeat
the remediation.

## Compatibility

The serialized `CachePolicy` shape and cache framing do not change, so the HTTP
cache format remains version 2. There are no deployed legacy entries requiring
migration. A version or namespace bump would not delete old files and would add
no protection in this repository's current lifecycle.

## Error Handling

Fingerprint generation remains infallible for valid `HeaderName` and
`HeaderValue` inputs. A SHA-256 digest formatted as lowercase ASCII always
constructs a valid replacement `HeaderValue`; the existing invariant-enforcing
`expect` remains appropriate.

No new cache errors or best-effort paths are introduced.

## Test Strategy

Implementation follows RED-GREEN-REFACTOR:

1. Expand the policy-view unit test with unrecognized credential names and
   prove their literal values appear in serialized `CachePolicy` before the
   fix.
2. Add a unit test proving cleartext policy fields retain their exact values.
3. Add a `Vary` test proving equal fingerprinted values match and different
   values miss.
4. Expand the revalidation restoration test to prove fingerprinted fields are
   restored from the wire while a policy-produced conditional validator is
   preserved.
5. Run focused HTTP-cache unit and integration suites, the isolated feature
   check, `just check`, the 21-configuration feature matrix, and the Rust 1.89
   MSRV check.

## Non-Goals

- No cached custom-request public API.
- No configurable secret-name regex or new redaction dependency.
- No response-header changes; response persistence is a separate concern.
- No cache-format migration or broad cache cleanup.
- No change to cache-key construction beyond documenting the caller's identity
  responsibility.
