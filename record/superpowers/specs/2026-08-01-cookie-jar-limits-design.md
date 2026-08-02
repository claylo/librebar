# Cookie Jar Limits Design

## Goal

Bound live and persisted cookie-jar growth while preserving Librebar's existing
`cookie_store` integration and allowing callers to raise each ceiling
deliberately.

## Public API

Under `http-cookies`, expose a `CookieLimits` value type with three private
ceilings and fluent setters:

- 4,096 bytes across a cookie's name and value
- 50 live cookies per effective cookie domain
- 3,000 live cookies in total

`HttpClientBuilder::cookie_limits(CookieLimits)` applies the policy to either
`with_cookie_jar()` or `with_cookie_jar_from()`. Keeping the policy on the
builder avoids adding another field to the currently exhaustive public
`HttpClientConfig` structure. A zero ceiling rejects or evicts every cookie in
that category; callers can explicitly raise a ceiling as needed.

## Enforcement

`CookieJar` stores an immutable copy of the limits beside its shared store.
Response cookies whose name plus value exceeds the byte ceiling are rejected
before insertion. After each response, and immediately after loading a
persistent jar, the store removes oversized existing cookies, then applies the
per-domain and total count ceilings.

`cookie_store` has expiry metadata but no last-access timestamps. Eviction
therefore uses nearest expiry, which the audit explicitly permits. Session
cookies sort after dated cookies because they have no known expiry; ties use
domain, path, and name for deterministic behavior. Each rejection or eviction
emits a warning containing the cookie name, domain where available, reason,
and ceiling, but never the cookie value.

## Verification

Focused unit tests cover oversized response rejection, per-domain eviction,
total eviction, persisted-jar pruning, and custom raised limits. The existing
cookie integration suite verifies builder wiring, followed by the repository's
each-feature and `just check` gates.
