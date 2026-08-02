---
audit: 2026-08-01-00-full-repo
last_updated: 2026-08-01
status:
  fixed: 63
  mitigated: 0
  accepted: 0
  disputed: 0
  deferred: 0
  open: 0
---

# Actions Taken: Full repository audit at 8fd83b3

Summary of remediation status for the
[2026-08-01 full repository audit](README.md).

---

## 2026-08-01 — Gate every Cargo feature configuration in CI

**Disposition:** fixed
**Addresses:** [ci-builds-only-all-features](README.md#ci-builds-only-all-features)
**Commit:** a5abea0
**Author:** Codex

Added a `feature-matrix` recipe to `.justfile` and a dedicated CI job that
installs `cargo-hack` through the existing cached tool setup. Both use
`cargo hack check --each-feature --no-dev-deps`, covering all features, no
default features, default features, and every named feature independently.

Local verification with `cargo-hack 0.6.45` completed all 21 generated
configurations. `actionlint` also accepted the updated workflow.

---

## 2026-08-01 — Publish the complete feature surface on docs.rs

**Disposition:** fixed
**Addresses:** [docs-rs-publishes-only-default-features](README.md#docs-rs-publishes-only-default-features)
**Commit:** a5abea0
**Author:** Codex

Added `[package.metadata.docs.rs]` with `all-features = true` and the
`docsrs` rustdoc cfg, then enabled rustdoc's current `doc_cfg` gate at the
crate root. The audit's proposed `doc_auto_cfg` gate was removed in Rust
1.92 and merged into `doc_cfg`, so applying the original remediation
verbatim would have broken the docs.rs build.

Nightly rustdoc generated the full all-features documentation and rendered
the expected feature badge on `librebar::shutdown`. Stable and MSRV 1.89
all-features checks also passed. The docs build still reports five existing
broken intra-doc links tracked separately by
`unresolved-intra-doc-error-links`.

---

## 2026-08-01 — Add an attested Trusted Publishing path

**Disposition:** mitigated
**Addresses:** [no-attested-publish-path](README.md#no-attested-publish-path)
**Commit:** pending (working tree)
**Author:** Codex

Made the existing CI workflow reusable and added a `v*` tag-triggered publish
workflow that cannot publish until the full CI suite passes. The publish job
requires the tag to match the manifest version, uses a short-lived crates.io
OIDC token, grants identity and attestation permissions only to that job, pins
all third-party actions to immutable commit SHAs, and attests the packaged
`.crate` before upload.

`actionlint` accepted both workflows. `cargo package --locked` and
`cargo publish --dry-run --locked` completed successfully; the package SHA-256
remained identical across both commands.

The remaining cutover is external: register `claylo/librebar`'s
`.github/workflows/publish.yml` and `release` environment as a crates.io Trusted
Publisher, configure any desired GitHub environment protection, prove one
release, and then revoke the legacy long-lived crates.io token. Until that is
done, the repository no longer contains a token-based publish path, but the new
OIDC workflow cannot authenticate.

---

## 2026-08-01 — Complete the Trusted Publishing cutover

**Disposition:** fixed
**Addresses:** [no-attested-publish-path](README.md#no-attested-publish-path)
**Commit:** c46be6a
**Author:** Clay Loveless

Registered the repository, publish workflow, and release environment as the
crate's Trusted Publisher, then removed the legacy long-lived crates.io token
from the repository secrets. This supersedes the preceding mitigation: the
OIDC publish path can now authenticate without a stored crates.io credential.

---

## 2026-08-01 — Compile README examples as doctests

**Disposition:** fixed
**Addresses:** [readme-code-blocks-outside-the-doc-test-gate](README.md#readme-code-blocks-outside-the-doc-test-gate)
**Commit:** pending (working tree)
**Author:** Codex

Added a doctest-only module in `src/lib.rs` that includes `README.md` whenever
the example feature set is enabled. Converted the README's Rust fragments to
`no_run` doctests with the minimum hidden scaffolding needed to compile them,
and labeled the two prose diagrams as `text` rather than implicit Rust.

The initial bridge discovered all 16 previously unchecked snippets and failed
on every fragment. After remediation, `just check` passed: clippy, cargo-deny,
all 213 nextest tests, and all 37 doctests, including the README examples.

---

## 2026-08-01 — Land the README doctest gate

**Disposition:** fixed
**Addresses:** [readme-code-blocks-outside-the-doc-test-gate](README.md#readme-code-blocks-outside-the-doc-test-gate)
**Commit:** 29bfe39
**Author:** Clay Loveless

Committed the previously recorded README doctest remediation. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Make Just the CI command source

**Disposition:** fixed
**Addresses:** [ci-reimplements-justfile-recipes](README.md#ci-reimplements-justfile-recipes)
**Commit:** pending (working tree)
**Author:** Codex

Added non-mutating `fmt-check` and reusable `msrv-check` recipes, made
`just check` use the formatting check, and removed the redundant Taplo-based
toolchain lookup because Cargo already honors `rust-toolchain.toml`. Every CI
verification command now invokes the corresponding `.justfile` recipe.

CI installs `just` and each job-specific Cargo tool through the existing
cached setup action. The dependency job now installs the pinned
`cargo-deny@0.20.2` binary and runs `just deny`, eliminating the duplicated
flag ordering and its synchronization comment.

`actionlint` accepted the workflow. `just check` passed all 213 nextest tests
and 37 doctests, `just feature-matrix` passed all 21 configurations, and
`RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land the shared Just recipes

**Disposition:** fixed
**Addresses:** [ci-reimplements-justfile-recipes](README.md#ci-reimplements-justfile-recipes)
**Commit:** e5e4967
**Author:** Clay Loveless

Committed the previously recorded CI and Justfile remediation. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Resolve and enforce intra-doc links

**Disposition:** fixed
**Addresses:** [unresolved-intra-doc-error-links](README.md#unresolved-intra-doc-error-links)
**Commit:** pending (working tree)
**Author:** Codex

Replaced the five ambiguous `Error::Cache` and `Error::Http` links with
explicit `crate::Error` variant targets while preserving their concise labels.
Added `rustdoc::broken_intra_doc_links` as a denied crate lint, then added a
`doc` recipe to the shared check gate and the CI lint job.

A strict rustdoc build first failed on exactly the five audited links. After
the fix, `just doc`, `actionlint`, and the complete `just check` gate passed;
the latter ran all 213 nextest tests, 37 doctests, and the warning-free API
documentation build.

---

## 2026-08-01 — Land the intra-doc link gate

**Disposition:** fixed
**Addresses:** [unresolved-intra-doc-error-links](README.md#unresolved-intra-doc-error-links)
**Commit:** 01b4408
**Author:** Clay Loveless

Committed the previously recorded intra-doc link remediation. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Publish contributor guidance and project status

**Disposition:** fixed
**Addresses:** [missing-contributing-and-status-badges](README.md#missing-contributing-and-status-badges)
**Commit:** pending (working tree)
**Author:** Codex

Added CI, crates.io, docs.rs, and MSRV badges directly below the README title.
Added a contributor guide covering the actual local gates, test and
documentation expectations, all four pull request templates, and the
Conventional Commit title format enforced by CI. Linked the guide from both
the README and GitHub's issue chooser.

`just check` passed all 213 nextest tests, 37 doctests, dependency policy,
Clippy, formatting, and API documentation. The issue chooser remains valid
YAML, every new repository-relative link resolves, all four badge images
returned HTTP 200, and the crates.io API confirmed `librebar` 0.3.0 as the
current release.

---

## 2026-08-01 — Land the contributor guide and status badges

**Disposition:** fixed
**Addresses:** [missing-contributing-and-status-badges](README.md#missing-contributing-and-status-badges)
**Commit:** 04b9177
**Author:** Clay Loveless

Committed the previously recorded contributor guide and project-status
remediation. This entry supersedes the preceding working-tree commit reference
without changing the finding's disposition count.

---

## 2026-08-01 — Redact and privatize debug bundles

**Disposition:** fixed
**Addresses:** [debug-bundle-ships-unredacted-content-world-readable](README.md#debug-bundle-ships-unredacted-content-world-readable)
**Commit:** pending (working tree)
**Author:** Codex

Added a public `Redactor` interface and a secure `SecretRedactor` default that
scrubs secret-bearing keys from JSON, JSON Lines, TOML, YAML, dotenv, and other
assignment-shaped UTF-8 content. Every `add_text`, `add_bytes`, and
`add_doctor_results` path now passes through the installed redactor; consumers
with schema-specific or opaque binary formats can install their own
implementation without bypassing the bundle pipeline.

Archives are created with owner-only permissions on Unix, and every tar entry
now records mode `0600` so extraction preserves the restriction. Regression
tests first reproduced the verbatim JSON leak and the `0644` archive and entry
modes, then passed after the remediation. `just check` passed 217 nextest tests
and 37 doctests, `just feature-matrix` passed all 21 configurations, and the
all-features build passed on the declared Rust 1.89 MSRV.

---

## 2026-08-01 — Add value-level credential detection to debug bundles

**Disposition:** fixed
**Addresses:** [debug-bundle-ships-unredacted-content-world-readable](README.md#debug-bundle-ships-unredacted-content-world-readable)
**Commit:** pending (working tree)
**Author:** Codex

Extended the preceding key-aware remediation with `leakguard` 0.8 as an
optional, diagnostics-only dependency. The default redactor now performs a
second pass for recognizable provider tokens, JWTs, inline URL credentials,
private keys, and cloud connection strings. It deliberately excludes broad PII
detectors such as email, IP address, phone number, and financial identifiers so
debug bundles retain useful diagnostic context.

A regression test first demonstrated that inline URL credentials survived the
key matcher, then passed with the value-level detector. `just check` passed all
217 nextest tests and 37 doctests, `just feature-matrix` passed all 21
configurations, and `RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed. This
entry extends the preceding working-tree action without changing the finding's
disposition count.

---

## 2026-08-01 — Land private, redacted debug bundles

**Disposition:** fixed
**Addresses:** [debug-bundle-ships-unredacted-content-world-readable](README.md#debug-bundle-ships-unredacted-content-world-readable)
**Commit:** 25cf1f3
**Author:** Clay Loveless

Committed the previously recorded debug-bundle remediation, including the
`leakguard` value-level detector. This entry supersedes both preceding
working-tree commit references without changing the finding's disposition
count.

---

## 2026-08-01 — Keep request credentials out of application logs

**Disposition:** fixed
**Addresses:** [request-uri-with-credentials-recorded-in-log-spans](README.md#request-uri-with-credentials-recorded-in-log-spans)
**Commit:** pending (working tree)
**Author:** Codex

Changed HTTP request spans to record only the URI scheme, host, port, and path,
dropping authority userinfo and the complete query string. Replaced the rolling
sink's default-permission file creation with an owner-only daily appender that
creates and tightens log files to `0600` on Unix. Removed the implicit current
working directory fallback so failed platform and `/var/log` resolution falls
back to stderr rather than writing into a repository or CI workspace. Updated
the README's resolution order to match.

Regression tests first captured the complete credential-bearing URI and the
pre-existing `0644` log mode, then passed after the fixes; a candidate-list test
also locks out the working-directory fallback. `just check` passed all 220
nextest tests and 37 doctests, `just feature-matrix` passed all 21
configurations, and `RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land credential-safe request logging

**Disposition:** fixed
**Addresses:** [request-uri-with-credentials-recorded-in-log-spans](README.md#request-uri-with-credentials-recorded-in-log-spans)
**Commit:** f518c31
**Author:** Clay Loveless

Committed the previously recorded request-span sanitization, private daily log
writer, and safe fallback policy. This entry supersedes the preceding
working-tree commit reference without changing the finding's disposition
count.

---

## 2026-08-01 — Privatize and bound crash dumps

**Disposition:** fixed
**Addresses:** [crash-dump-world-readable-and-unbounded](README.md#crash-dump-world-readable-and-unbounded)
**Commit:** pending (working tree)
**Author:** Codex

Replaced `std::fs::write` with create-new file creation at mode `0600` on
Unix, preventing both world-readable panic data and same-timestamp clobbering.
After each successful write, the crash directory is pruned to its ten newest
`.crash` files; a pruning failure removes the new dump rather than allowing the
directory to grow further. Module and API documentation now warn that dumps
may contain panic payloads, backtraces, and source paths and describe the
retention policy.

Regression tests first reproduced the `0644` mode, same-timestamp overwrite,
and twelve-file growth, then passed with owner-only creation, collision refusal,
and ten-file retention. `just check` passed all 223 nextest tests and 37
doctests, `just feature-matrix` passed all 21 configurations, and
`RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land private, bounded crash dumps

**Disposition:** fixed
**Addresses:** [crash-dump-world-readable-and-unbounded](README.md#crash-dump-world-readable-and-unbounded)
**Commit:** 307474b
**Author:** Clay Loveless

Committed the previously recorded owner-only crash-file creation, collision
protection, and ten-dump retention policy. This entry supersedes the preceding
working-tree commit reference without changing the finding's disposition
count.

---

## 2026-08-01 — Stream pre-sanitized files into debug bundles

**Disposition:** fixed
**Addresses:** [debug-bundle-buffers-entire-archive-in-memory](README.md#debug-bundle-buffers-entire-archive-in-memory)
**Commit:** pending (working tree)
**Author:** Codex

Replaced the builder's uniform `(name, Vec<u8>)` storage with typed buffered
and file-backed entries. The new `add_sanitized_file` API retains only a path
and streams that file through the gzip encoder during `finish()`, while setting
the tar entry mode to `0600`. Its explicit name and documentation preserve the
mandatory redaction boundary: callers must use `add_text` or `add_bytes` for
content that has not already been sanitized. `add_bytes` now accepts
`impl Into<Vec<u8>>`, allowing callers to move an owned buffer across the API
boundary instead of forcing an additional clone.

Regression tests first failed because the streaming method and owned-buffer
signature did not exist, then passed after the implementation. The streaming
test also proves that content is read at `finish()` and that the resulting tar
entry remains owner-only. `just check` passed all 224 nextest tests (2 skipped)
and 37 doctests, `just feature-matrix` passed all 21 configurations, and
`RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land streamed debug-bundle file entries

**Disposition:** fixed
**Addresses:** [debug-bundle-buffers-entire-archive-in-memory](README.md#debug-bundle-buffers-entire-archive-in-memory)
**Commit:** 7f37291
**Author:** Clay Loveless

Committed the previously recorded file-backed streaming entry, explicit
pre-sanitization boundary, and owned-buffer input support. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Redact HTTP response debug output

**Disposition:** fixed
**Addresses:** [response-debug-impl-exposes-body-and-set-cookie](README.md#response-debug-impl-exposes-body-and-set-cookie)
**Commit:** pending (working tree)
**Author:** Codex

Replaced the derived `Debug` implementations for `Response` and
`ResponseMetadata` with explicit safe summaries. Debug output now includes the
status, HTTP version, header and trailer names, response-body length, and cache
status when enabled; it never formats header values, trailer values, or body
bytes. The derived `ConditionalResponse` and `ModificationCheck` output now
inherits those safe representations.

The regression test first reproduced the unsafe derived representation using
a public HTTP response containing `Set-Cookie`, another secret-bearing header,
a secret-bearing trailer, and body bytes. It then passed against all four
public debug surfaces after the fix. `just check` passed all 225 nextest tests
(2 skipped) and 37 doctests, `just feature-matrix` passed all 21
configurations, and `RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land redacted HTTP response debug output

**Disposition:** fixed
**Addresses:** [response-debug-impl-exposes-body-and-set-cookie](README.md#response-debug-impl-exposes-body-and-set-cookie)
**Commit:** 0c4b7e7
**Author:** Clay Loveless

Committed the previously recorded safe `Response` and `ResponseMetadata`
debug summaries and inherited protection for conditional response wrappers.
This entry supersedes the preceding working-tree commit reference without
changing the finding's disposition count.

---

## 2026-08-01 — Write structured JSON crash dumps

**Disposition:** fixed
**Addresses:** [crash-dumps-documented-as-json-are-free-text](README.md#crash-dumps-documented-as-json-are-free-text)
**Commit:** pending (working tree)
**Author:** Codex

Derived `Serialize` for `CrashInfo` and now stream the complete structure
directly into the existing owner-only crash file with `serde_json`. The
human-readable `CrashInfo::format()` remains available for terminal output,
while the `crash` feature now explicitly enables its JSON dependency. Existing
collision protection, cleanup on write failure, and ten-file retention are
unchanged.

The regression test first failed while parsing the prior free-text dump, then
passed after asserting every documented field and a multiline panic message in
the JSON output. `just check` passed all 226 nextest tests (2 skipped) and 37
doctests, `just feature-matrix` passed all 21 configurations, and
`RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land structured JSON crash dumps

**Disposition:** fixed
**Addresses:** [crash-dumps-documented-as-json-are-free-text](README.md#crash-dumps-documented-as-json-are-free-text)
**Commit:** 8172cc5
**Author:** Clay Loveless

Committed the previously recorded direct JSON serialization, explicit crash
feature dependency, and complete structured-output regression coverage. This
entry supersedes the preceding working-tree commit reference without changing
the finding's disposition count.

---

## 2026-08-01 — Make debug bundles fully chainable

**Disposition:** fixed
**Addresses:** [debug-bundle-builder-cannot-be-chained](README.md#debug-bundle-builder-cannot-be-chained)
**Commit:** pending (working tree)
**Author:** Codex

Changed all four `DebugBundle::add_*` methods to consume and return `Self`,
matching the ownership model of `finish(self)` and the crate's other fluent
builders. Each method is now `#[must_use]`; the example and integration tests
retain the returned builder, including explicit rebinding in the loop-based
redaction test. Redaction timing, file streaming, archive permissions, and
error propagation are unchanged.

The regression test first failed with `E0507` when the complete builder chain
tried to move out of the former `&mut Self` result, then passed after the API
correction. All 10 focused diagnostics tests and the all-feature
`doctor-bundle` example passed. `just check` passed all 226 nextest tests (2
skipped) and 37 doctests, `just feature-matrix` passed all 21 configurations,
and `RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land chainable debug bundles

**Disposition:** fixed
**Addresses:** [debug-bundle-builder-cannot-be-chained](README.md#debug-bundle-builder-cannot-be-chained)
**Commit:** 7c259d7
**Author:** Clay Loveless

Committed the previously recorded consuming `DebugBundle` entry methods,
`#[must_use]` safeguards, and migrated fluent and loop-based callers. This
entry supersedes the preceding working-tree commit reference without changing
the finding's disposition count.

---

## 2026-08-01 — Store HTTP cache bodies as raw bytes

**Disposition:** fixed
**Addresses:** [http-cache-entry-body-amplification](README.md#http-cache-entry-body-amplification)
**Commit:** pending (working tree)
**Author:** Codex

Replaced the cache's nested JSON and base64 envelopes with versioned binary
framing. Generic cache entries now store an owner-only 16-byte header followed
by the raw value, while HTTP entries store the raw response body followed by
JSON metadata and a fixed footer. Multipart atomic writes borrow the response
body directly, and revalidation reuses the owned cached response instead of
cloning its body a second time. HTTP keys moved to the `http:v2:` namespace;
the unused legacy format is intentionally treated as a cold miss because no
cache entries exist in the field.

The regression test first measured 4,195,011 bytes on disk for a 1 MiB body,
then passed the 1 MiB plus 16 KiB bound after the fix. The 13 generic-cache and
15 HTTP-cache integration tests passed, along with 6 focused HTTP-cache unit
tests and isolated `cache` and `http-cache` feature checks. `just check` passed
all 229 nextest tests (2 skipped) and 37 doctests, `just feature-matrix` passed
all 21 configurations, and `RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land bytes-native HTTP cache storage

**Disposition:** fixed
**Addresses:** [http-cache-entry-body-amplification](README.md#http-cache-entry-body-amplification)
**Commit:** b313919
**Author:** Clay Loveless

Committed the previously recorded v2 cache framing, multipart atomic writes,
body-first HTTP storage, and amplification regression coverage. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Keep async cache I/O off runtime workers

**Disposition:** fixed
**Addresses:** [blocking-fsync-on-async-cache-paths](README.md#blocking-fsync-on-async-cache-paths)
**Commit:** pending (working tree)
**Author:** Codex

Added a feature-gated, crate-private blocking-pool boundary while keeping the
public `Cache` API synchronous. HTTP-cache reads, decoding, corruption cleanup,
eviction, serialization, and multipart writes now run through
`tokio::task::spawn_blocking`; update-check cache reads and writes use the same
boundary. Cache task join failures retain their source through the existing
cache I/O error chain. Removed the explicit `sync_all()` before
`AtomicWriteFile::commit()`, which already performs the durability sync before
renaming.

The regression first failed to compile without the boundary, then reproduced
the runtime stall with a deliberately synchronous implementation: a blocked
cache operation prevented a timer on a current-thread runtime from firing.
The same test passed with `spawn_blocking`. The 13 generic-cache, 15 HTTP-cache,
6 focused HTTP-cache unit, and 5 update tests passed with isolated feature
checks. `just check` passed all 230 nextest tests (2 skipped) and 37 doctests,
`just feature-matrix` passed all 21 configurations, and
`RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land non-blocking async cache I/O

**Disposition:** fixed
**Addresses:** [blocking-fsync-on-async-cache-paths](README.md#blocking-fsync-on-async-cache-paths)
**Commit:** b8e8aab
**Author:** Clay Loveless

Committed the previously recorded blocking-pool boundary for HTTP-cache and
update-check filesystem operations, the current-thread runtime regression
test, and the redundant durability-barrier removal. This entry supersedes the
preceding working-tree commit reference without changing the finding's
disposition count.

---

## 2026-08-01 — Fingerprint non-policy HTTP request headers

**Disposition:** fixed
**Addresses:** [http-cache-persists-unrecognized-credential-headers](README.md#http-cache-persists-unrecognized-credential-headers)
**Commit:** pending (working tree)
**Author:** Codex

Replaced the fixed three-name credential list with a private cleartext-policy
predicate. Only `host`, request cache directives, and validators that
`http-cache-semantics` must interpret remain literal; every other request
header value is persisted as a domain-separated SHA-256 fingerprint. The same
predicate restores real wire values before revalidation without overwriting
policy-produced validators. Deterministic fingerprints preserve `Vary`
matching, and the public cache-key contract now calls out requesting identity
when an origin omits the corresponding `Vary` field. The public API, cache
format, and dependency set are unchanged.

The first regression reproduced the finding by serializing `api-secret` from
an unrecognized `x-api-key` field. A second RED proved that hashing every field
would break literal `Host` and cache-directive semantics, and a third proved
that fixed-name restoration left the new field fingerprinted on the wire. All
three passed after the policy and restoration boundary was generalized. The 8
focused HTTP-cache unit tests and 15 HTTP-cache integration tests passed, along
with the isolated feature check. `just check` passed all 232 nextest tests (2
skipped) and 37 doctests, `just feature-matrix` passed all 21 configurations,
and `RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land request-header cache redaction

**Disposition:** fixed
**Addresses:** [http-cache-persists-unrecognized-credential-headers](README.md#http-cache-persists-unrecognized-credential-headers)
**Commit:** 19c870d
**Author:** Clay Loveless

Committed the previously recorded cleartext-policy allowlist, deterministic
request-header fingerprints, generalized wire restoration, `Vary` regression
coverage, and identity-aware cache-key documentation. This entry supersedes
the preceding working-tree commit reference without changing the finding's
disposition count.

---

## 2026-08-01 — Prune expired cache entries during active writes

**Disposition:** fixed
**Addresses:** [cache-has-no-eviction-outside-per-key-reads](README.md#cache-has-no-eviction-outside-per-key-reads)
**Commit:** pending (working tree)
**Author:** Codex

Added a public, header-only `Cache::prune()` sweep that removes expired v2
entries without reading their values or touching live, malformed, or unrelated
files. The write path now attempts a sweep before its first write and at most
hourly afterward. Cloned cache handles share that cadence through an atomic
timestamp, and the async HTTP-cache adapters preserve it by cloning the
caller's handle instead of reconstructing one from its directory. The cache
remains free of live-entry count and byte ceilings.

The first RED failed because `Cache::prune` did not exist. The automatic-write
RED left an unread expired entry on disk, the clone RED failed because `Cache`
was not cloneable, and the HTTP integration RED proved reconstructed blocking
adapters triggered redundant first-write sweeps. All passed after the explicit
sweep, shared cadence, and adapter changes. The 17 generic-cache tests and 16
HTTP-cache tests passed, along with the standalone cache feature check. `just
check` passed 237 nextest tests (2 skipped) and 37 doctests, `just
feature-matrix` passed all 21 configurations, and
`RUSTUP_TOOLCHAIN=1.89.0 just msrv-check` passed.

---

## 2026-08-01 — Land expired-cache pruning

**Disposition:** fixed
**Addresses:** [cache-has-no-eviction-outside-per-key-reads](README.md#cache-has-no-eviction-outside-per-key-reads)
**Commit:** b6cdf9b
**Author:** Clay Loveless

Committed the previously recorded header-only `Cache::prune()` API,
first-write and hourly maintenance cadence, shared clone state, HTTP adapter
integration, and regression coverage. This entry supersedes the preceding
working-tree commit reference without changing the finding's disposition
count.

---

## 2026-08-01 — Remove durable syncs from disposable cache writes

**Disposition:** fixed
**Addresses:** [cache-set-fsync-per-write](README.md#cache-set-fsync-per-write)
**Commit:** pending (working tree)
**Author:** Codex

Replaced the generic cache's `AtomicWriteFile` path with a same-directory
`tempfile::NamedTempFile` and atomic `persist`, preserving private permissions,
symlink replacement, directory rejection, and complete-write visibility without
forcing the disposable entry to stable storage. The credential-bearing cookie
jar retains `AtomicWriteFile::commit()` as its single durability barrier; its
redundant explicit `sync_all()` was removed.

A manifest regression first failed because `cache` still selected
`atomic-write-file` instead of `tempfile`, then passed after the feature split.
All 17 cache tests and 7 cookie tests passed, `cargo hack` checked all 21 feature
configurations, and `just check` passed 238 tests (2 skipped), 37 doctests,
Clippy, dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land cache write durability split

**Disposition:** fixed
**Addresses:** [cache-set-fsync-per-write](README.md#cache-set-fsync-per-write)
**Commit:** 380f4f9
**Author:** Clay Loveless

Committed the previously recorded same-directory cache persistence, cookie
durability split, Cargo feature boundary, and regression coverage. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Log every HTTP cache eviction failure

**Disposition:** fixed
**Addresses:** [http-cache-eviction-results-discarded](README.md#http-cache-eviction-results-discarded)
**Commit:** pending (working tree)
**Author:** Codex

Routed all seven best-effort HTTP cache eviction paths through private sync and
async helpers. Removal failures remain non-fatal and preserve request outcomes,
but now emit a uniform warning containing the caller-visible cache key and the
underlying error. The paths cover corrupt entries, request mismatches,
unexpected 304 responses, and non-storable responses.

The regression test first failed to compile because the eviction helper did not
exist, then passed after the helpers and warning contract were implemented. All
16 HTTP cache integration tests and all 21 Cargo feature configurations passed.
`just check` passed 239 nextest tests (2 skipped), 37 doctests, Clippy,
dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land observable HTTP cache eviction

**Disposition:** fixed
**Addresses:** [http-cache-eviction-results-discarded](README.md#http-cache-eviction-results-discarded)
**Commit:** 795ff46
**Author:** Clay Loveless

Committed the previously recorded sync and async best-effort eviction helpers,
uniform key-and-error warning contract, and regression coverage. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Stop expired reads from unlinking replacements

**Disposition:** fixed
**Addresses:** [cache-expiry-unlink-races-concurrent-write](README.md#cache-expiry-unlink-races-concurrent-write)
**Commit:** pending (working tree)
**Author:** Codex

Removed opportunistic deletion from `Cache::get()`. Expired entries still
return a cache miss, but cleanup is now owned by explicit pruning and periodic
write-path maintenance, so a reader cannot unlink a fresh entry atomically
installed by a concurrent writer. Updated the cache documentation to match.

The regression first failed because the expired read removed its backing path,
then passed after the unlink was removed. All 17 cache integration tests passed.
`just check` passed 239 nextest tests (2 skipped), 37 doctests, Clippy,
dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land race-free expired cache reads

**Disposition:** fixed
**Addresses:** [cache-expiry-unlink-races-concurrent-write](README.md#cache-expiry-unlink-races-concurrent-write)
**Commit:** e1fafa1
**Author:** Clay Loveless

Committed the previously recorded change that leaves expired entries for
explicit or periodic pruning, along with the regression and corrected cache
documentation. This entry supersedes the preceding working-tree commit
reference without changing the finding's disposition count.

---

## 2026-08-01 — Reject public-suffix domain cookies

**Disposition:** fixed
**Addresses:** [cookie-jar-never-installs-public-suffix-list](README.md#cookie-jar-never-installs-public-suffix-list)
**Commit:** pending (working tree)
**Author:** Codex

Embedded the authoritative Public Suffix List snapshot dated 2026-07-25 and
installed it in both fresh and reloaded cookie stores. The `http-cookies`
feature now names the already-transitive `publicsuffix` crate directly, so
`cookie_store` rejects cross-tenant cookies such as a `.github.io` cookie from
`attacker.github.io` before it can be sent to `victim.github.io`. The bundled
asset retains its upstream MPL-2.0 header, version, and commit metadata.

Fresh-jar and reloaded-jar regressions first failed by returning the attacker
cookie for the victim URL, then passed after the list was installed. All 7 HTTP
cookie integration tests and all 21 Cargo feature configurations passed.
`cargo package --list --allow-dirty` includes the list asset. `just check`
passed 241 nextest tests (2 skipped), 37 doctests, Clippy, dependency policy,
formatting, and API documentation.

---

## 2026-08-01 — Land public-suffix cookie rejection

**Disposition:** fixed
**Addresses:** [cookie-jar-never-installs-public-suffix-list](README.md#cookie-jar-never-installs-public-suffix-list)
**Commit:** a3c709e
**Author:** Clay Loveless

Committed the embedded Public Suffix List, fresh and reloaded jar
initialization, direct feature dependency, and cross-tenant cookie regression
coverage. This entry supersedes the preceding working-tree commit reference
without changing the finding's disposition count.

---

## 2026-08-01 — Harden the redirect trust boundary

**Disposition:** fixed
**Addresses:** [cross-origin-redirect-forwards-non-blocklisted-credentials](README.md#cross-origin-redirect-forwards-non-blocklisted-credentials)
**Commit:** pending (working tree)
**Author:** Codex

Changed the redirect policy to refuse HTTPS-to-HTTP downgrades and to clear all
caller-supplied headers and request extensions before a cross-origin hop. The
policy restores only Librebar's configured user-agent; same-origin redirects
retain their existing header behavior. Downgrades surface as the typed
`HttpError::RedirectDowngrade` variant, and the public HTTP documentation now
states the trust-boundary contract.

The cross-origin regression first failed because `X-Api-Key: secret` reached
the second origin, while the downgrade regression first failed because no
target validator existed. Both passed after the policy change. All 37 local
HTTP integration tests passed with 2 intentional network tests ignored, all 21
Cargo feature configurations compiled, and `just check` passed 243 nextest
tests (2 skipped), 37 doctests, Clippy, dependency policy, formatting, and API
documentation.

---

## 2026-08-01 — Land redirect trust-boundary enforcement

**Disposition:** fixed
**Addresses:** [cross-origin-redirect-forwards-non-blocklisted-credentials](README.md#cross-origin-redirect-forwards-non-blocklisted-credentials)
**Commit:** d902ffc
**Author:** Clay Loveless

Committed cross-origin header and extension clearing, configured user-agent
restoration, HTTPS downgrade rejection, the typed redirect error, regression
coverage, and the documented public contract. This entry supersedes the
preceding working-tree commit reference without changing the finding's
disposition count.

---

## 2026-08-01 — Recover and report cookie-jar failures

**Disposition:** fixed
**Addresses:** [cookie-jar-failures-are-silent](README.md#cookie-jar-failures-are-silent)
**Commit:** pending (working tree)
**Author:** Codex

Centralized cookie-store locking behind private read and write helpers that
warn, recover the protected store, and clear lock poisoning instead of silently
dropping request or response cookies. Cookie header construction now rejects
only the malformed name-value pair and reports its name without logging the
value. URI conversion, non-text `Set-Cookie` fields, and malformed cookie syntax
also emit warnings rather than disappearing through `ok()` filters.

The poisoned read and response-write regressions first returned no cookie, the
malformed-pair regression first lacked an isolation helper, and the URI
regression first lacked an observable conversion path. All passed after the
change. Six focused cookie unit tests, all 7 cookie integration tests, and all
21 Cargo feature configurations passed. `just check` passed 247 nextest tests
(2 skipped), 37 doctests, Clippy, dependency policy, formatting, and API
documentation.

---

## 2026-08-01 — Land cookie-jar failure recovery

**Disposition:** fixed
**Addresses:** [cookie-jar-failures-are-silent](README.md#cookie-jar-failures-are-silent)
**Commit:** 40c05a2
**Author:** Clay Loveless

Committed poison recovery for cookie-jar reads and writes, pair-level header
encoding isolation, warning-level diagnostics for rejected response cookies and
unusable request URIs, and focused regression coverage. This entry supersedes
the preceding working-tree commit reference without changing the finding's
disposition count.

---

## 2026-08-01 — Bound live and persisted cookie jars

**Disposition:** fixed
**Addresses:** [cookie-jar-accepts-unbounded-cookie-count](README.md#cookie-jar-accepts-unbounded-cookie-count)
**Commit:** pending (working tree)
**Author:** Codex

Added a public `CookieLimits` policy with defaults of 4,096 bytes per cookie,
50 live cookies per domain, and 3,000 live cookies in total. Callers can raise
each ceiling explicitly through `HttpClientBuilder::cookie_limits`. Oversized
response cookies are rejected before storage, while live and reloaded jars are
pruned deterministically by nearest expiry; session-cookie and key ordering
provide stable fallback behavior. Limit warnings identify the cookie and
reason without recording its value.

Four focused regressions first retained oversized, per-domain, total, and
reloaded-jar excess cookies, then passed after enforcement. Ten cookie unit
tests, all 8 cookie integration tests, and all 21 Cargo feature configurations
passed. `just check` passed 252 nextest tests (2 skipped), 37 doctests, Clippy,
dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land cookie-jar resource limits

**Disposition:** fixed
**Addresses:** [cookie-jar-accepts-unbounded-cookie-count](README.md#cookie-jar-accepts-unbounded-cookie-count)
**Commit:** 46aa8d3
**Author:** Clay Loveless

Committed configurable per-cookie, per-domain, and total resource ceilings,
nearest-expiry eviction for live and reloaded jars, warning-level diagnostics,
and regression coverage. This entry supersedes the preceding working-tree
commit reference without changing the finding's disposition count.

---

## 2026-08-01 — Document compiled TLS trust anchors

**Disposition:** fixed
**Addresses:** [webpki-root-store-is-compiled-in](README.md#webpki-root-store-is-compiled-in)
**Commit:** pending (working tree)
**Author:** Codex

Added an explicit trust-anchor section to the HTTP module documentation. It
states that HTTPS certificate verification remains enabled, Mozilla's
`webpki-roots` set is compiled into each application binary, and OS trust-store
changes do not update an already-built artifact. The guidance tells consumers
with long-lived binaries to rebuild and re-release on root-program changes,
records Librebar's dependency-maintenance commitment, and makes the absence of
an OS-managed or enterprise-root connector explicit.

All 37 doctests and API documentation generation passed, all 21 Cargo feature
configurations compiled, and `just check` passed 252 nextest tests (2 skipped),
Clippy, dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land TLS trust-anchor documentation

**Disposition:** fixed
**Addresses:** [webpki-root-store-is-compiled-in](README.md#webpki-root-store-is-compiled-in)
**Commit:** 07beaa1
**Author:** Clay Loveless

Committed the compiled-root lifecycle, consumer rebuild guidance, Librebar
dependency-maintenance commitment, and current OS-managed trust limitation.
This entry supersedes the preceding working-tree commit reference without
changing the finding's disposition count.

---

## 2026-08-01 — Make panic-hook notices fallible

**Disposition:** fixed
**Addresses:** [crash-hook-print-turns-panics-into-aborts](README.md#crash-hook-print-turns-panics-into-aborts)
**Commit:** pending (working tree)
**Author:** Codex

Replaced both panic-hook `eprintln!` paths with a private helper that writes to
a locked stderr handle through `writeln!` and explicitly discards I/O errors.
A closed or broken stderr can no longer trigger a nested panic before the
previous hook runs; successful and failed crash-dump notices retain their
existing text, and the previous hook remains unconditionally chained.

The broken-writer regression first failed because the fallible notice helper
did not exist, then passed after the hook adopted it. The focused unit test and
all 7 crash integration tests passed, all 21 Cargo feature configurations
compiled, and `just check` passed 253 nextest tests (2 skipped), 37 doctests,
Clippy, dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land fallible panic-hook notices

**Disposition:** fixed
**Addresses:** [crash-hook-print-turns-panics-into-aborts](README.md#crash-hook-print-turns-panics-into-aborts)
**Commit:** 1633d8c
**Author:** Clay Loveless

Committed fallible stderr notices for both crash-dump outcomes, preservation of
the previous panic-hook chain, and broken-writer regression coverage. This
entry supersedes the preceding working-tree commit reference without changing
the finding's disposition count.

---

## 2026-08-01 — Keep shutdown signal handling interruptible

**Disposition:** fixed
**Addresses:** [signal-task-exits-after-first-signal](README.md#signal-task-exits-after-first-signal), [ctrl-c-registration-error-triggers-shutdown](README.md#ctrl-c-registration-error-triggers-shutdown)
**Commit:** pending (working tree)
**Author:** Codex

Kept the signal task alive after graceful shutdown begins and made any later
SIGINT or SIGTERM force an immediate exit with conventional status 130 or 143.
Unix SIGINT and SIGTERM listeners are now registered eagerly, so either
registration failure propagates from `register_signals`; non-Unix Ctrl-C
listener errors are logged and end the task without spuriously requesting
shutdown. The API documentation now records that signal registration
permanently replaces the process defaults and describes the escalation policy.

Three state-machine regressions first failed because the signal actions and
Ctrl-C result handling did not exist, then passed after implementation. All 6
shutdown integration tests and all 21 Cargo feature configurations passed.
`just check` passed 256 nextest tests (2 skipped), 37 doctests, Clippy,
dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land interruptible shutdown signal handling

**Disposition:** fixed
**Addresses:** [signal-task-exits-after-first-signal](README.md#signal-task-exits-after-first-signal), [ctrl-c-registration-error-triggers-shutdown](README.md#ctrl-c-registration-error-triggers-shutdown)
**Commit:** e3c6cde
**Author:** Clay Loveless

Committed persistent SIGINT and SIGTERM listeners, conventional forced-exit
escalation for repeated signals, eager Unix registration-error propagation,
non-Unix error handling, API documentation, and regression coverage. This
entry supersedes the preceding working-tree commit reference without changing
either finding's disposition count.

---

## 2026-08-01 — Make non-returning output paths fallible

**Disposition:** fixed
**Addresses:** [print-macros-panic-where-errors-cannot-propagate](README.md#print-macros-panic-where-errors-cannot-propagate)
**Commit:** pending (working tree)
**Author:** Codex

Replaced panic-prone print macros in `OtelGuard::drop` and
`JsonLogLayer::on_event` with fallible writes whose errors are explicitly
discarded. `CommonArgs::apply` now writes `--version-only` through its existing
`io::Result`, so a closed stdout returns the original I/O error instead of
panicking. The public error documentation now includes that output failure.

Three broken-writer regressions first failed because the fallible output paths
did not exist, then passed after implementation. All 40 CLI, 11 logging, and 4
OpenTelemetry integration tests passed, as did all 21 Cargo feature
configurations. `just check` passed 259 nextest tests (2 skipped), 37 doctests,
Clippy, dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land fallible non-returning output paths

**Disposition:** fixed
**Addresses:** [print-macros-panic-where-errors-cannot-propagate](README.md#print-macros-panic-where-errors-cannot-propagate)
**Commit:** cdbd9bb
**Author:** Clay Loveless

Committed best-effort stderr diagnostics for telemetry shutdown and log-sink
failures, propagated `--version-only` stdout errors, updated API documentation,
and broken-writer regressions. This entry supersedes the preceding
working-tree commit reference without changing the finding's disposition
count.

---

## 2026-08-01 — Make retry-budget consumption defensive

**Disposition:** fixed
**Addresses:** [retry-counter-decrement-relies-on-caller-invariant](README.md#retry-counter-decrement-relies-on-caller-invariant)
**Commit:** pending (working tree)
**Author:** Codex

Changed `wait_to_retry` to consume its unsigned retry budget with
`saturating_sub`, preserving zero if a future caller misses the existing
`remaining > 0` guard. A local doc comment now states the caller contract and
the defensive zero-budget behavior.

The zero-budget regression first reproduced the debug overflow panic, then
passed after the saturating decrement. All 37 HTTP integration tests and all
21 Cargo feature configurations passed. `just check` passed 260 nextest tests
(2 skipped), 37 doctests, Clippy, dependency policy, formatting, and API
documentation.

---

## 2026-08-01 — Land defensive retry-budget consumption

**Disposition:** fixed
**Addresses:** [retry-counter-decrement-relies-on-caller-invariant](README.md#retry-counter-decrement-relies-on-caller-invariant)
**Commit:** 9235305
**Author:** Clay Loveless

Committed saturating retry-budget consumption, the explicit caller contract,
and zero-budget regression coverage. This entry supersedes the preceding
working-tree commit reference without changing the finding's disposition
count.

---

## 2026-08-01 — Drive Hyper OTLP export on a private runtime

**Disposition:** fixed
**Addresses:** [otel-batch-processor-cannot-drive-hyper-exporter](README.md#otel-batch-processor-cannot-drive-hyper-exporter)
**Commit:** pending (working tree)
**Author:** Codex

Added a private `BlockingHyperClient` adapter that sends OTLP requests over a
bounded channel to an owned Tokio runtime thread, where Hyper's async client
drives them to completion. The default batch processor can now block on each
export from its ordinary worker thread without requiring an ambient runtime.
`build_exporter` supplies the adapter explicitly with `with_http_client`,
preserves the standard OTLP trace-timeout precedence, and joins the runtime
worker during teardown. Reqwest remains absent from the dependency graph.

The synchronous end-to-end regression first reproduced the batch processor's
"there is no reactor running" panic and received no HTTP request, then passed
with a protobuf span delivered to a local OTLP receiver. All 5 OTEL tests and
all 21 Cargo feature configurations passed. `just check` passed 261 nextest
tests (2 skipped), 37 doctests, Clippy, dependency policy, formatting, and API
documentation.

---

## 2026-08-01 — Land private-runtime Hyper OTLP export

**Disposition:** fixed
**Addresses:** [otel-batch-processor-cannot-drive-hyper-exporter](README.md#otel-batch-processor-cannot-drive-hyper-exporter)
**Commit:** 9062846
**Author:** Clay Loveless

Committed the explicit `BlockingHyperClient`, its owned Tokio runtime worker,
OTLP timeout preservation, and synchronous end-to-end export coverage. This
entry supersedes the preceding working-tree commit reference without changing
the finding's disposition count.

---

## 2026-08-01 — Make OTLP HTTP/JSON an explicit codec feature

**Disposition:** fixed
**Addresses:** [otel-http-json-protocol-not-buildable](README.md#otel-http-json-protocol-not-buildable)
**Commit:** pending (working tree)
**Author:** Codex

Added the opt-in `otel-http-json` feature and wired `http/json` to
OpenTelemetry's JSON codec explicitly. The protobuf path now also selects its
codec explicitly, preserving Librebar's default even though upstream prefers
JSON whenever both codec features are enabled. A request for `http/json`
without the feature returns a configuration error instead of silently sending
protobuf. The README, crate feature table, and OTEL module documentation now
state the feature requirement.

The JSON wire-format regression first received protobuf, and the disabled
feature regression first accepted the unsupported configuration; both passed
after implementation. The OTEL test binary passed all 6 tests with base `otel`
and all 6 with all features. All 22 Cargo feature configurations passed.
`just check` passed 262 nextest tests (2 skipped), 37 doctests, Clippy,
dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land explicit OTLP HTTP/JSON codec selection

**Disposition:** fixed
**Addresses:** [otel-http-json-protocol-not-buildable](README.md#otel-http-json-protocol-not-buildable)
**Commit:** cf1ceb1
**Author:** Clay Loveless

Committed the opt-in HTTP/JSON codec feature, explicit protobuf/JSON protocol
selection, unsupported-configuration error, documentation, and wire-format
regressions. This entry supersedes the preceding working-tree commit reference
without changing the finding's disposition count.

---

## 2026-08-01 — Merge span fields without a temporary map

**Disposition:** fixed
**Addresses:** [log-event-clones-span-field-map](README.md#log-event-clones-span-field-map)
**Commit:** pending (working tree)
**Author:** Codex

Replaced the whole-map clone in the JSON event hot path with direct insertion
from borrowed span fields. Each key and value is cloned only when ownership by
the output event requires it; no intermediate `serde_json::Map` is allocated or
destroyed. Removed `Clone` from `SpanFields` so the wasteful pattern cannot be
reintroduced through that type.

A focused red/green regression verifies root-to-leaf collision precedence. The
logging unit suite passed all 5 tests, all 22 Cargo feature configurations
passed, and `just check` passed 263 tests (2 skipped), 37 doctests, Clippy,
dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land borrowed span-field merging

**Disposition:** fixed
**Addresses:** [log-event-clones-span-field-map](README.md#log-event-clones-span-field-map)
**Commit:** dac9db8
**Author:** Clay Loveless

Committed direct borrowed insertion for span fields, removal of the whole-map
`Clone` path, and precedence regression coverage. This entry supersedes the
preceding working-tree commit reference without changing the finding's
disposition count.

---

## 2026-08-01 — Replace inert OTEL environment fields with constants

**Disposition:** fixed
**Addresses:** [otel-config-env-var-name-fields-unread](README.md#otel-config-env-var-name-fields-unread)
**Commit:** pending (working tree)
**Author:** Codex

Removed the three mutable `env_var_*` fields from `OtelConfig`; they advertised
custom environment-variable selection but were only metadata captured after
the environment had already been read. The standard OTLP endpoint and protocol
names are now associated constants used by the implementation, while the
application-specific deployment variable remains an internal derivation.

The public-constant regression first failed because the constants did not
exist, then passed after the API correction. A separate behavior test verifies
that `{APP}_ENV` is actually read. Both OTEL feature modes passed all 7 tests,
all 22 Cargo feature configurations passed, and `just check` passed 264 tests
(2 skipped), 37 doctests, Clippy, dependency policy, formatting, and API
documentation.

---

## 2026-08-01 — Land fixed OTEL environment names

**Disposition:** fixed
**Addresses:** [otel-config-env-var-name-fields-unread](README.md#otel-config-env-var-name-fields-unread)
**Commit:** efbaefa
**Author:** Clay Loveless

Committed removal of the inert mutable environment-name fields, associated
constants for the standard OTLP names, and behavior coverage for the
application-specific deployment environment. This entry supersedes the
preceding working-tree commit reference without changing the finding's
disposition count.

---

## 2026-08-01 — Exercise gRPC OTLP without an ambient async runtime

**Disposition:** fixed
**Addresses:** [otel-grpc-feature-has-no-test](README.md#otel-grpc-feature-has-no-test)
**Commit:** pending (working tree)
**Author:** Codex

Added inline `otel-grpc` integration coverage that selects the gRPC protocol
and constructs the OTLP layer from an ordinary synchronous test. The initial
regression exposed a runtime panic in Tonic's lazy channel construction, so the
remediation also builds and drives the exporter on a private current-thread
Tokio runtime retained by `OtelGuard`. Provider shutdown now completes before
the private runtime is stopped and joined.

The regression failed with `there is no reactor running` before the runtime
fix and passed afterward. The gRPC-only and all-features OTEL suites each
passed all 8 tests, all 22 Cargo feature configurations passed, and
`just check` passed 265 tests (2 skipped), 37 doctests, Clippy, dependency
policy, formatting, and API documentation.

## 2026-08-01 — Land private-runtime gRPC OTLP export

**Disposition:** fixed
**Addresses:** [otel-grpc-feature-has-no-test](README.md#otel-grpc-feature-has-no-test)
**Commit:** d870e0e
**Author:** Clay Loveless

Committed the inline gRPC protocol regression, private Tonic runtime worker,
and ordered provider/runtime shutdown. This entry supersedes the preceding
working-tree commit reference without changing the finding's disposition
count.

---

## 2026-08-01 — Probe the actual daily log target

**Disposition:** fixed
**Addresses:** [log-writability-probe-creates-unused-file](README.md#log-writability-probe-creates-unused-file)
**Commit:** pending (working tree)
**Author:** Codex

Routed log-directory writability checks through the existing daily-file
opener. Resolution now validates the same owner-only dated path used by the
appender instead of creating an undated zero-byte file that is never written.

The regression first observed only the stray undated file and passed after
the probe targeted the dated appender path. The logging suite passed all 12
tests, all 22 Cargo feature configurations passed, and `just check` passed
266 tests (2 skipped), 37 doctests, Clippy, dependency policy, formatting,
and API documentation.

---

## 2026-08-01 — Land daily-path log probing

**Disposition:** fixed
**Addresses:** [log-writability-probe-creates-unused-file](README.md#log-writability-probe-creates-unused-file)
**Commit:** 79a9b15
**Author:** Clay Loveless

Committed reuse of the daily appender path for writability checks and the
regression proving resolution leaves no unused undated file. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Define reachable dependency boundaries

**Disposition:** fixed
**Addresses:** [unreachable-dependency-types-in-public-api](README.md#unreachable-dependency-types-in-public-api)
**Commit:** pending (working tree)
**Author:** Codex

Made deliberate extension dependencies reachable from the feature modules
that expose them: `cli::clap`, `config::serde_json`,
`logging::tracing_subscriber`, `otel::tracing_subscriber`, and `mcp::rmcp`.
HTTP protocol and buffer types now come from direct `http` and `bytes`
dependencies re-exported by `librebar::http`, leaving Hyper private to the
transport implementation. The MCP stdio helper now returns an opaque RMCP
transport so Tokio's concrete stdin and stdout types are no longer public API.

Added consumer-style compile coverage for each reachable path, updated the
examples and README, and documented the local call-site migration and redundant
dependency cleanup steps. All six new consumer paths failed before the API
changes and passed afterward. The affected suites passed 142 tests (2 ignored),
all 22 Cargo feature configurations passed, and `just check` passed 267 tests
(2 skipped), 37 doctests, Clippy, dependency policy, formatting, and API
documentation. The migration note also passed the readability gate at grade
6.3.

---

## 2026-08-01 — Land explicit public dependency boundaries

**Disposition:** fixed
**Addresses:** [unreachable-dependency-types-in-public-api](README.md#unreachable-dependency-types-in-public-api)
**Commit:** c9b58af
**Author:** Clay Loveless

Committed the scoped extension-crate re-exports, direct HTTP protocol boundary,
opaque MCP stdio transport, consumer compile coverage, and local migration
note. This entry supersedes the preceding working-tree commit reference without
changing the finding's disposition count.

---

## 2026-08-01 — Preserve opaque nested error sources

**Disposition:** fixed
**Addresses:** [dependency-error-payloads-are-unwrappable](README.md#dependency-error-payloads-are-unwrappable), [error-variants-drop-the-source-chain](README.md#error-variants-drop-the-source-chain)
**Commit:** pending (working tree)
**Author:** Codex

Added the public `error::BoxError` boundary and replaced every dependency-owned
error payload in Librebar's public enums with that stable boxed source. Concrete
errors remain available for downcasting, and their own nested sources remain in
the standard `Error::source()` chain without making dependency versions part of
the enum layout. The four `std::io::Error` wrappers that previously stopped the
chain now carry `#[source]` as well.

Consumer regressions first failed because `BoxError` did not exist and the
variants still required dependency-specific payloads. All 12 passed after the
change, including traversal from `librebar::Error` through a dependency error to
its root cause. All 22 Cargo feature configurations passed without warnings,
and `just check` passed 279 tests (2 skipped), 37 doctests, Clippy, dependency
policy, formatting, and API documentation. The updated migration note passed
the readability gate at grade 7.3.

---

## 2026-08-01 — Land stable nested error sources

**Disposition:** fixed
**Addresses:** [dependency-error-payloads-are-unwrappable](README.md#dependency-error-payloads-are-unwrappable), [error-variants-drop-the-source-chain](README.md#error-variants-drop-the-source-chain)
**Commit:** f404978
**Author:** Clay Loveless

Committed the stable boxed error boundary, complete nested source traversal,
consumer regression coverage, and migration guidance. This entry supersedes
the preceding working-tree commit reference without changing either finding's
disposition count.

---

## 2026-08-01 — Stabilize growable public records

**Disposition:** fixed
**Addresses:** [growable-public-structs-lack-non-exhaustive](README.md#growable-public-structs-lack-non-exhaustive)
**Commit:** pending (working tree)
**Author:** Codex

Marked Librebar's growable config, diagnostics, crash, update, and CLI schema
records `#[non_exhaustive]` while preserving readable and mutable public
fields. Caller-created records now have explicit constructors and builders,
and `HttpClientConfig::http_cache_stale_retention` exists in every `http`
build so feature unification cannot change the struct's shape.

The constructor regressions first failed because `CheckResult`, `CrashInfo`,
and `UpdateInfo` had no supported construction API. The compile-fail contracts
also failed because downstream `HttpClientConfig` and `OutputField` literals
still compiled. All passed after the boundary change. All 22 Cargo feature
configurations passed, and `just check` passed 280 tests (2 skipped), 39
doctests, Clippy, dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land stable public record construction

**Disposition:** fixed
**Addresses:** [growable-public-structs-lack-non-exhaustive](README.md#growable-public-structs-lack-non-exhaustive)
**Commit:** acb87fc
**Author:** Clay Loveless

Committed the non-exhaustive public records, stable HTTP configuration shape,
caller construction APIs, regression contracts, and migration guidance. This
entry supersedes the preceding working-tree commit reference without changing
the finding's disposition count.

---

## 2026-08-01 — Make update checks pluggable and fallible

**Disposition:** fixed
**Addresses:** [update-checker-hardcodes-github-and-its-collaborators](README.md#update-checker-hardcodes-github-and-its-collaborators), [update-check-drops-errors-it-documents-as-logged](README.md#update-check-drops-errors-it-documents-as-logged)
**Commit:** pending (working tree)
**Author:** Codex

Added an async `ReleaseSource` boundary and moved GitHub-specific transport,
response parsing, API configuration, and optional bearer authentication into
`GitHubReleaseSource`. `UpdateChecker` now accepts a caller-owned source and an
injectable or disabled cache, while `UpdateChecker::github` preserves the
common setup path. Cached entries contain the complete source-provided version
and release URL; authentication is marked sensitive on the request and redacted
from backend debug output.

`check()` now returns `Result<Option<UpdateInfo>, UpdateError>`, reserving
`Ok(None)` for suppressed or up-to-date checks. Client construction, source,
transport, status, and decoding failures are returned with nested sources;
cache read, decode, and write failures are logged and fall back to the source.
Fourteen update regressions cover injected sources and caches, corrupt-cache
fallback, GitHub parsing, bearer authentication, debug redaction, and error
chains. All 22 Cargo feature configurations passed, and `just check` passed 289
tests (2 skipped), 39 doctests and compile-fail checks, Clippy, dependency
policy, formatting, and API documentation.

---

## 2026-08-01 — Land pluggable update checks

**Disposition:** fixed
**Addresses:** [update-checker-hardcodes-github-and-its-collaborators](README.md#update-checker-hardcodes-github-and-its-collaborators), [update-check-drops-errors-it-documents-as-logged](README.md#update-check-drops-errors-it-documents-as-logged)
**Commit:** 80be63e
**Author:** Clay Loveless

Committed the release-source boundary, authenticated GitHub backend,
injectable caching, explicit nested errors, regression coverage, and migration
guidance. This entry supersedes the preceding working-tree commit reference
without changing either finding's disposition count.

---

## 2026-08-01 — Scope environment sources and preserve config provenance

**Disposition:** fixed
**Addresses:** [environment-source-trait-over-constrains-implementors](README.md#environment-source-trait-over-constrains-implementors)
**Commit:** pending (working tree)
**Author:** Codex

Removed the `Debug` supertrait from `EnvironmentSource`, replaced the derived
`ConfigLoader` formatter with a source-opaque implementation, and passed the
normalized application prefix into fallible source queries. Remote or
restricted backends can now query only the relevant namespace and preserve
their concrete query error through `Error::ConfigEnvironmentSource`.

The same configuration path now records the winning origin for defaults,
preloaded values, files, environment variables, and programmatic overrides.
Final conversion failures report the Serde field path and origin through
`Error::ConfigValue` while keeping the concrete deserializer error as a nested
source; successful loads expose the map through `ConfigSources::origin`.
Replacement layers discard obsolete descendant origins. All 22 Cargo feature
configurations passed, and `just check` passed 295 tests (2 skipped), 38
doctests plus 2 compile-fail checks, Clippy, dependency policy, formatting, and
API documentation.

---

## 2026-08-01 — Land scoped config sources and value provenance

**Disposition:** fixed
**Addresses:** [environment-source-trait-over-constrains-implementors](README.md#environment-source-trait-over-constrains-implementors)
**Commit:** aa2e111
**Author:** Clay Loveless

Committed the scoped and fallible environment-source contract, source-opaque
loader formatting, merged-value provenance, path-aware nested errors,
regression coverage, and migration guidance. This entry supersedes the
preceding working-tree commit reference without changing the finding's
disposition count.

---

## 2026-08-01 — Hide doctor check storage from callers

**Disposition:** fixed
**Addresses:** [doctor-check-registration-forces-caller-boxing](README.md#doctor-check-registration-forces-caller-boxing)
**Commit:** pending (working tree)
**Author:** Codex

Changed `DoctorRunner::add` to accept a concrete `DoctorCheck` and box it
internally, keeping heterogeneous storage private to the runner. Removed the
unused `Send` supertrait because checks execute sequentially on the calling
thread, allowing checks to hold thread-local state such as `Rc`.

The regression first failed on both the `Send` bound and the required
`Box<dyn DoctorCheck>`, then passed with an unboxed `Rc`-backed check. Updated
the module documentation, doctor-bundle example, existing tests, and migration
guide. All 22 Cargo feature configurations passed, and `just check` passed 296
tests (2 skipped), 38 doctests plus 2 compile-fail checks, Clippy, dependency
policy, formatting, and API documentation.

---

## 2026-08-01 — Land unboxed doctor check registration

**Disposition:** fixed
**Addresses:** [doctor-check-registration-forces-caller-boxing](README.md#doctor-check-registration-forces-caller-boxing)
**Commit:** fa80023
**Author:** Clay Loveless

Committed concrete doctor check registration, support for thread-local check
state, updated examples, regression coverage, and migration guidance. This
entry supersedes the preceding working-tree commit reference without changing
the finding's disposition count.

---

## 2026-08-01 — Make CLI schema documents readable

**Disposition:** fixed
**Addresses:** [schema-wire-types-are-serialize-only](README.md#schema-wire-types-are-serialize-only)
**Commit:** pending (working tree)
**Author:** Codex

Added `Deserialize`, `PartialEq`, and `Eq` across the CLI schema document tree
and replaced borrowed wire strings with owned values. Empty collections and
false flags now deserialize to the values omitted during serialization, while
unknown fields remain forward-compatible. Boxed the schema parse outcome to
keep the generic result compact after the owned representation grew.

The regression first failed because `SchemaDocument` lacked deserialization
and equality, then exposed the missing defaults for omitted fields before
passing a fully populated JSON round trip. Added user documentation and a
migration note. All 22 Cargo feature configurations passed, and `just check`
passed 297 tests (2 skipped), 39 doctests plus 2 compile-fail checks, Clippy,
dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land readable CLI schema documents

**Disposition:** fixed
**Addresses:** [schema-wire-types-are-serialize-only](README.md#schema-wire-types-are-serialize-only)
**Commit:** 19a3982
**Author:** Clay Loveless

Committed deserializable and comparable CLI schema documents, owned wire
strings, round-trip defaults, compact schema parse outcomes, regression
coverage, and migration guidance. This entry supersedes the preceding
working-tree commit reference without changing the finding's disposition
count.

---

## 2026-08-01 — Distinguish lock contention from I/O failure

**Disposition:** fixed
**Addresses:** [lock-error-message-misreports-the-cause](README.md#lock-error-message-misreports-the-cause)
**Commit:** pending (working tree)
**Author:** Codex

Added `Error::LockContended { path }` for the exact `WouldBlock` case and now
preserve genuine `TryLockError::Error` values as sourced `Error::Lock`
failures. Updated the lockfile error contract and migration guidance so
callers can classify an already-running process without mistaking operating
system failures for contention.

The regression first failed because the contention variant did not exist.
Focused tests then covered both a second holder and preservation of a genuine
I/O error's kind and message. All 22 Cargo feature configurations passed, and
`just check` passed 298 tests (2 skipped), 39 doctests plus 2 compile-fail
checks, Clippy, dependency policy, formatting, and API documentation.

---

## 2026-08-01 — Land precise lock error classification

**Disposition:** fixed
**Addresses:** [lock-error-message-misreports-the-cause](README.md#lock-error-message-misreports-the-cause)
**Commit:** 8db1326
**Author:** Clay Loveless

Committed the dedicated lock-contention variant, preservation of genuine lock
I/O sources, focused branch coverage, and migration guidance. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Render each error chain layer once

**Disposition:** fixed
**Addresses:** [error-display-duplicates-its-source](README.md#error-display-duplicates-its-source)
**Commit:** pending (working tree)
**Author:** Codex

Removed nested source interpolation from source-bearing error displays. The
top-level HTTP and cache adapters now delegate their display, while
context-bearing variants retain concise domain labels and preserve their
underlying sources for traversal and downcasting. Config parse failures retain
the file, parser, and dependency layers as distinct chain entries.

The representative regressions first reproduced repeated config, HTTP, cache,
and diagnostic messages, then passed with each message rendered exactly once.
Updated the error contract and migration guidance for callers with display
snapshots or string comparisons. All 22 Cargo feature configurations passed,
and `just check` passed Clippy, dependency policy, the full test and doctest
suites, formatting, and API documentation.

---

## 2026-08-01 — Land nonduplicating error displays

**Disposition:** fixed
**Addresses:** [error-display-duplicates-its-source](README.md#error-display-duplicates-its-source)
**Commit:** d10da27
**Author:** Clay Loveless

Committed one-message-per-layer error displays, preservation of typed nested
causes, representative chain regressions, and migration guidance. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Qualify advisory lock guarantees

**Disposition:** fixed
**Addresses:** [lockfile-exclusion-guarantee-unqualified](README.md#lockfile-exclusion-guarantee-unqualified)
**Commit:** pending (working tree)
**Author:** Codex

Replaced the unconditional exclusion claim with the actual advisory-locking
contract. The module and acquisition docs now require a filesystem that
implements the platform's locking semantics, warn that network, FUSE, and
overlay filesystems may reject or defeat exclusion, and explain that a lock
file left on disk is not itself a stale held lock. The public feature tables
now describe local process coordination rather than unconditional prevention.

The overlapping lock-error classification was already fixed separately in
`8db1326`; this action closes the finding's remaining documentation defect.
All 22 Cargo feature configurations passed, and `just check` passed Clippy,
dependency policy, the full test and doctest suites, formatting, and API
documentation.

---

## 2026-08-01 — Land advisory lock guarantees

**Disposition:** fixed
**Addresses:** [lockfile-exclusion-guarantee-unqualified](README.md#lockfile-exclusion-guarantee-unqualified)
**Commit:** 601cc71
**Author:** Clay Loveless

Committed the filesystem-qualified advisory locking contract, custom-directory
caveats, stale-file clarification, and aligned public feature descriptions.
This entry supersedes the preceding working-tree commit reference without
changing the finding's disposition count.

---

## 2026-08-01 — Remove the shared Linux lock fallback

**Disposition:** fixed
**Addresses:** [lockfile-falls-back-to-shared-tmp-on-linux](README.md#lockfile-falls-back-to-shared-tmp-on-linux)
**Commit:** pending (working tree)
**Author:** Codex

Replaced Linux's `/tmp/{app}` fallback with the established per-user directory
resolver already used elsewhere in Librebar. Lock paths now prefer
`XDG_RUNTIME_DIR`, fall back to `XDG_STATE_HOME` or `~/.local/state`, and return
an explicit error when neither is available instead of entering a shared local
user namespace. The `lockfile` feature now enables the existing `directories`
dependency.

Made `default_lock_dir` return `Result<PathBuf>`, updated its only call sites,
and documented the intentional migration. The regression first failed on the
infallible API and missing secure selector, then passed per-user state fallback,
refusal of a shared fallback, and the complete lockfile suite. All 22 Cargo
feature configurations passed, and `just check` passed Clippy, dependency
policy, the full test and doctest suites, formatting, and API documentation.

---

## 2026-08-01 — Land per-user Linux lock paths

**Disposition:** fixed
**Addresses:** [lockfile-falls-back-to-shared-tmp-on-linux](README.md#lockfile-falls-back-to-shared-tmp-on-linux)
**Commit:** 55508ff
**Author:** Clay Loveless

Committed per-user Linux runtime/state lock resolution, explicit failure when
no secure directory exists, regression coverage, and the direct-caller
migration. This entry supersedes the preceding working-tree commit reference
without changing the finding's disposition count.

---

## 2026-08-01 — Move YAML parsing onto a stable semver line

**Disposition:** fixed
**Addresses:** [serde-saphyr-exact-pin-on-default-path](README.md#serde-saphyr-exact-pin-on-default-path)
**Commit:** pending (working tree)
**Author:** Codex

Replaced the exact-by-construction `serde-saphyr` 0.0.29 requirement with the
stable `1.0` line. The resolved 1.0.0 release keeps Librebar's `from_str` and
`to_string` integration intact while allowing ordinary compatible updates to
flow through Cargo. Its dependency refresh also moves `granit-parser` to 1.0.0,
removes `ahash`, and introduces a tolerated `base64` 0.23 duplicate alongside
the 0.22 line still required by OTEL, MCP, and Tonic.

The 50 config tests passed before and after the upgrade, and the focused
structured-diagnostic redaction regression passed on 1.0.0. `cargo audit`
reported no vulnerabilities and the existing allowed yanked
`proc-macro-error3` warning. All 22 Cargo feature configurations passed, and
`just check` passed Clippy, dependency policy, 304 tests (2 skipped), 39
doctests plus 2 compile-fail checks, formatting, and API documentation.

---

## 2026-08-01 — Land the stable YAML parser line

**Disposition:** fixed
**Addresses:** [serde-saphyr-exact-pin-on-default-path](README.md#serde-saphyr-exact-pin-on-default-path)
**Commit:** 7edd011
**Author:** Clay Loveless

Committed the `serde-saphyr` 1.0 requirement, refreshed parser dependency
graph, semantic config and diagnostic verification, and full feature-matrix
coverage. This entry supersedes the preceding working-tree commit reference
without changing the finding's disposition count.

---

## 2026-08-01 — Ratchet dependency and unsafe-feature drift

**Disposition:** fixed
**Addresses:** [bans-multiple-versions-warn-only](README.md#bans-multiple-versions-warn-only), [base64-simd-unsafe-optout-holds](README.md#base64-simd-unsafe-optout-holds)
**Commit:** pending (working tree)
**Author:** Codex

Updated every direct dependency reported by `just outdated`, including RMCP
3.1 and SHA-2 0.11, and refreshed the complete lockfile. RMCP defaults are now
disabled in favor of its explicit server and stdio features; Librebar's own
example uses RMCP 3's `CallToolResponse`, and the public re-export migration is
documented. SHA-2 0.11 no longer formats digest arrays as hexadecimal, so the
cache now writes the same lowercase fingerprint explicitly and locks its exact
wire representation with a regression test.

Changed duplicate-version policy from a warning to a denial and recorded the
eight exact versions that remain unavoidable, each with its upstream owner and
removal condition. Unknown registries are now denied. Added a cargo-deny feature
ban for `base64/simd-unsafe`; this caught RMCP and serde-saphyr defaults
re-enabling the unsafe implementation during the upgrade. Serde-saphyr now
parses only, while redacted YAML is emitted as pretty JSON, a valid YAML 1.2
subset, and the diagnostic regression verifies that output remains parseable.

`just outdated` reports all dependencies current. `cargo audit` found no
vulnerabilities across 308 packages, the all-features graph contains only
base64's safe `std` and `alloc` features, and all 22 Cargo feature
configurations passed. `just check` passed Clippy, dependency policy, 305 tests
(2 skipped), 39 doctests plus 2 compile-fail checks, formatting, and API
documentation.

---

## 2026-08-01 — Land the dependency policy ratchet

**Disposition:** fixed
**Addresses:** [bans-multiple-versions-warn-only](README.md#bans-multiple-versions-warn-only), [base64-simd-unsafe-optout-holds](README.md#base64-simd-unsafe-optout-holds)
**Commit:** 87d1d4e
**Author:** Clay Loveless

Committed the complete direct-dependency refresh, exact duplicate-version
baseline, unknown-registry denial, and enforced ban on base64's unsafe SIMD
feature. RMCP moved directly to 3.1 without a compatibility layer because no
Librebar consumer uses the MCP surface yet. This entry supersedes the preceding
working-tree commit reference without changing the findings' disposition count.

---

## 2026-08-01 — Make supply-chain exceptions self-expiring

**Disposition:** fixed
**Addresses:** [ring-is-the-sole-c-asm-island](README.md#ring-is-the-sole-c-asm-island), [license-allowlist-stale-entries](README.md#license-allowlist-stale-entries), [advisory-suppressions-removed-after-cause-cleared](README.md#advisory-suppressions-removed-after-cause-cleared)
**Commit:** pending (working tree)
**Author:** Codex

Documented the deliberate choice of `ring` over `aws-lc-rs` as the smaller
audited C and assembly surface, along with the two concrete revisit triggers:
a production-equivalent pure-Rust rustls provider or a FIPS requirement.

Removed the three licenses not used by the all-features dependency graph —
BSL-1.0, MIT-0, and CC0-1.0 — and changed cargo-deny to reject future stale
allowlist entries. Preserved the empty advisory-ignore history and recorded the
required shape of future exceptions: a reason naming the upstream removal
condition plus an expiration date that fails closed.

The focused cargo-deny licenses, bans, and sources checks passed. All 22 Cargo
feature configurations passed, and `just check` passed Clippy, dependency
policy, 305 tests (2 skipped), 39 doctests plus 2 compile-fail checks,
formatting, and API documentation.

---

## 2026-08-01 — Land self-expiring supply-chain exceptions

**Disposition:** fixed
**Addresses:** [ring-is-the-sole-c-asm-island](README.md#ring-is-the-sole-c-asm-island), [license-allowlist-stale-entries](README.md#license-allowlist-stale-entries), [advisory-suppressions-removed-after-cause-cleared](README.md#advisory-suppressions-removed-after-cause-cleared)
**Commit:** 29c6f57
**Author:** Clay Loveless

Committed the explicit `ring` boundary rationale, stale-license denial, and
future advisory-exception requirements. This entry supersedes the preceding
working-tree commit reference without changing the findings' disposition
count.

---

## 2026-08-01 — Stop config discovery at a root boundary

**Disposition:** fixed
**Addresses:** [git-boundary-marker-inert-when-search-root-is-repo-root](README.md#git-boundary-marker-inert-when-search-root-is-repo-root)
**Commit:** pending (working tree)
**Author:** Codex

Removed the exception that ignored a boundary marker in the project search
root. Discovery still checks that directory's config candidates first, then
stops before inspecting its parent. Added a regression that plants a config in
the parent of a search root containing `.git` and verifies that the parent file
does not participate in resolution.

The regression failed before the implementation change by resolving the
parent's `Warn` value instead of the default `Info`, then passed after the
change. All 51 config integration tests passed, all 22 Cargo feature
configurations passed, and `just check` passed Clippy, dependency policy, 306
tests (2 skipped), 39 doctests plus 2 compile-fail checks, formatting, and API
documentation.

---

## 2026-08-01 — Land repository-root config containment

**Disposition:** fixed
**Addresses:** [git-boundary-marker-inert-when-search-root-is-repo-root](README.md#git-boundary-marker-inert-when-search-root-is-repo-root)
**Commit:** 569606a
**Author:** Clay Loveless

Committed the repository-root boundary fix and its red-green regression. This
entry supersedes the preceding working-tree commit reference without changing
the finding's disposition count.

---

## 2026-08-01 — Collapse project-config filesystem probes

**Disposition:** fixed
**Addresses:** [config-discovery-stat-fanout](README.md#config-discovery-stat-fanout)
**Commit:** pending (working tree)
**Author:** Codex

Precomputed the eight distinct candidate names once per load, then replaced
twelve unconditional `is_file` calls per directory level with one directory
enumeration and an additional `.config` enumeration only when that entry is
present. Candidate metadata is now queried only for names that actually match;
if directory enumeration is unavailable, discovery falls back to the original
direct probes instead of losing access to a known file.

Added a characterization test that locks the existing extension-before-layout
precedence across TOML and YAML candidates. All 52 config integration tests
passed, all 22 Cargo feature configurations passed, and `just check` passed
Clippy, dependency policy, 307 tests (2 skipped), 39 doctests plus 2
compile-fail checks, formatting, and API documentation.

---

## 2026-08-01 — Land bounded config discovery probes

**Disposition:** fixed
**Addresses:** [config-discovery-stat-fanout](README.md#config-discovery-stat-fanout)
**Commit:** ce973cc
**Author:** Clay Loveless

Committed directory-enumeration-based project config discovery, direct-probe
fallback, and precedence characterization. This entry supersedes the preceding
working-tree commit reference without changing the finding's disposition
count.

---

## 2026-08-01 — Restrict plugin lookup to absolute PATH entries

**Disposition:** fixed
**Addresses:** [dispatch-resolves-binary-from-current-directory](README.md#dispatch-resolves-binary-from-current-directory)
**Commit:** pending (working tree)
**Author:** Codex

Changed dispatch resolution to build a sanitized PATH containing only absolute
directories before calling `which_in`, then reject any non-absolute result as
defense in depth. Empty and relative PATH components can no longer select a
binary from the working directory, while a legitimate binary in a later
absolute directory remains discoverable.

The child-process regression isolated both a leading empty component and a
relative component. Before the fix each resolved the planted cwd binary as the
bare path `myapp-deploy`; after the fix both resolved the trusted absolute
binary. All four dispatch integration tests passed, all 22 Cargo feature
configurations passed, and `just check` passed Clippy, dependency policy, 309
tests (2 skipped), 39 doctests plus 2 compile-fail checks, formatting, and API
documentation.

---

## 2026-08-01 — Land absolute-only plugin dispatch

**Disposition:** fixed
**Addresses:** [dispatch-resolves-binary-from-current-directory](README.md#dispatch-resolves-binary-from-current-directory)
**Commit:** d8a39f1
**Author:** Clay Loveless

Committed absolute-only PATH filtering, defense-in-depth result validation,
and isolated regressions for empty and relative components. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count.

---

## 2026-08-01 — Validate update metadata before caching or display

**Disposition:** fixed
**Addresses:** [cached-version-string-interpolated-into-release-url](README.md#cached-version-string-interpolated-into-release-url)
**Commit:** pending (working tree)
**Author:** Codex

The earlier pluggable-release-source remediation removed the finding's original
GitHub URL synthesis from cached version strings. Closed the remaining trust
boundary by requiring source and cached release metadata to contain a valid
semantic version and an absolute HTTPS URI before it can be cached, compared,
or displayed. Invalid cache metadata now falls back to the configured release
source, and GitHub API responses are held to the same contract without
hard-coding github.com, preserving custom and enterprise API support.

Update messages also escape terminal control characters as defense in depth.
The malformed-cache, malformed-source, and output-escaping regressions failed
before their respective implementation changes, then passed afterward. All 18
update integration tests passed, all 22 Cargo feature configurations passed,
and `just check` passed Clippy, dependency policy, 313 tests (2 skipped), 39
doctests plus 2 compile-fail checks, formatting, and API documentation.

---

## 2026-08-01 — Land validated update metadata

**Disposition:** fixed
**Addresses:** [cached-version-string-interpolated-into-release-url](README.md#cached-version-string-interpolated-into-release-url)
**Commit:** bdd5b23
**Author:** Clay Loveless

Committed strict semantic-version and HTTPS release-URL validation, invalid
cache fallback, and terminal-safe update rendering. This entry supersedes the
preceding working-tree commit reference without changing the finding's
disposition count.

---

## 2026-08-01 — Measure filesystem cache reads and writes

**Disposition:** fixed
**Addresses:** [bench-apparatus-measures-nothing](README.md#bench-apparatus-measures-nothing)
**Commit:** pending (working tree)
**Author:** Codex

Added a real Divan benchmark target for `Cache::set` and `Cache::get` at 1
KiB, 64 KiB, and 1 MiB. The target uses the existing `BenchConfig` to
configure Divan's sampling iterations and time ceiling, and `just bench`
provides the canonical invocation. The existing `bench` feature, optional
dependency, benchmark profile, and configuration type now support measured
Librebar behavior instead of compile-only scaffolding.

The target-absence check failed before the change because Cargo had no `cache`
benchmark, then `cargo bench --bench cache --features bench,cache -- --test`
ran all six cases once without warnings. All 22 Cargo feature configurations
passed, and `just check` passed Clippy, dependency policy, 313 tests (2
skipped), 39 doctests plus 2 compile-fail checks, formatting, and API
documentation.

---

## 2026-08-01 — Land filesystem cache benchmarks

**Disposition:** fixed
**Addresses:** [bench-apparatus-measures-nothing](README.md#bench-apparatus-measures-nothing)
**Commit:** 95a6cbe
**Author:** Clay Loveless

Committed the Divan cache read/write target, its three representative body
sizes, the `BenchConfig` runner integration, and the canonical `just bench`
recipe. This entry supersedes the preceding working-tree commit reference
without changing the finding's disposition count.

---

## 2026-08-01 — Forbid unsafe code in the library target

**Disposition:** fixed
**Addresses:** [unsafe-escape-hatch-rationale-does-not-match-use](README.md#unsafe-escape-hatch-rationale-does-not-match-use)
**Commit:** pending (working tree)
**Author:** Codex

Removed the package-wide `unsafe_code = "deny"` lint and its obsolete benchmark
rationale from `Cargo.toml`, then changed the library crate attribute to
`#![forbid(unsafe_code)]`. Unsafe code can no longer be re-enabled anywhere in
the shipped library. The integration-test crates remain independently scoped,
so their serialized Rust 2024 environment mutation can retain its narrow,
documented unsafe blocks.

All 22 Cargo feature configurations passed. `just check` also proved the
library hardening and integration-test scoping coexist: Clippy and dependency
policy passed, all 313 tests passed (2 skipped), all 39 doctests and 2
compile-fail checks passed, formatting was clean, and API documentation built.

---

## 2026-08-01 — Land library-target unsafe prohibition

**Disposition:** fixed
**Addresses:** [unsafe-escape-hatch-rationale-does-not-match-use](README.md#unsafe-escape-hatch-rationale-does-not-match-use)
**Commit:** 5695eb0
**Author:** Clay Loveless

Committed the library-level `forbid(unsafe_code)` boundary and removal of the
obsolete package-wide benchmark exception. This entry supersedes the preceding
working-tree commit reference without changing the finding's disposition
count.

---

## 2026-08-01 — Clarify and simplify local Cargo profiles

**Disposition:** fixed
**Addresses:** [cargo-profiles-do-not-reach-consumers](README.md#cargo-profiles-do-not-reach-consumers)
**Commit:** pending (working tree)
**Author:** Codex

Documented that Librebar's profile tables affect only builds where this crate
is the workspace root; downstream consumers retain control of their own Cargo
profiles. Removed the redundant dependency `opt-level = 1` override and the
two `codegen-units = 256` settings that restated Cargo's dev/test defaults.
Kept the benchmark profile because the preceding remediation added the real
cache benchmark target it now configures.

`cargo metadata` accepted the simplified manifest, all 22 Cargo feature
configurations passed, and `just check` passed Clippy, dependency policy, all
313 tests (2 skipped), all 39 doctests and 2 compile-fail checks, formatting,
and API documentation.

---

## 2026-08-01 — Land simplified local Cargo profiles

**Disposition:** fixed
**Addresses:** [cargo-profiles-do-not-reach-consumers](README.md#cargo-profiles-do-not-reach-consumers)
**Commit:** 3e4b96e
**Author:** Clay Loveless

Committed the local-only profile documentation and removal of redundant Cargo
settings while retaining the now-active benchmark profile. This entry
supersedes the preceding working-tree commit reference without changing the
finding's disposition count. The audit closes with all 63 findings fixed.
