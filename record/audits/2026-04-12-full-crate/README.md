---
audit_date: 2026-04-12
project: rebar
commit: 89431caaefbdffcdaf2f0a3077c891ad27e4625b
scope: Fresh full-library audit — src/, tests/, Cargo.toml, README.md, and feature-gated API surface
auditor: Codex (GPT-5)
findings:
  critical: 0
  significant: 6
  moderate: 4
  advisory: 1
  note: 0
---

# Audit: rebar

rebar's supported verification path is clean: `cargo nextest run --workspace --all-features`
passes 88/88, `clippy` is clean at `-D warnings`, `cargo deny check --config
.config/deny.toml` passes, and the crate-wide `#![deny(unsafe_code)]` posture
holds. The real problems are at the documentation and API seam, where several
advertised capabilities are either not wired at all or require hidden setup.
**The Completeness Surface** is the sharpest: shutdown docs do not work as
written, logging docs promise config-driven behavior that the builder never
implements, CLI scripting flags are parsed but inert, and OTEL docs overstate
transport support. **The Builder Contract Surface**, **The Error Handling
Surface**, and **The Performance Surface** are all fundamentally recoverable,
but each contains one or two places where the current public contract is
narrower, noisier, or more expensive than it looks. The single most important
takeaway is that rebar's core is solid, but the docs-and-contracts boundary is
currently overpromising.

See [report.html](report.html) for the fully rendered evidence view.

## The Completeness Surface

*The README and module docs currently promise more than the shipped feature
surface delivers.*

### Shutdown usage docs fail as written and hide the Tokio runtime requirement

**significant** · `src/shutdown.rs:9-16` · effort: small

The published shutdown example is not usable as shown: `shutdown_token()`
returns an `Option`, so the snippet does not compile without an unwrap, and the
startup path also requires an active Tokio runtime that the docs never mention.
This is a direct entry-point mismatch, not a speculative API polish issue.

**Remediation:** rewrite the example around an async `main`, unwrap the token
explicitly, and surface the runtime requirement in the shutdown builder docs.

### Builder docs promise `log_dir` from config, but logging never sees loaded config

**significant** · `README.md:182-186` · effort: medium

The README says logging reads `log_dir` from the loaded config and even lists it
in the resolution order, but `ConfiguredBuilder::start()` never threads the
loaded config value into `init_subsystems()`. Only `.with_log_dir(...)` reaches
logging. Users who follow the documented config pattern get no effect.

**Remediation:** either remove the config-driven `log_dir` claim from the docs,
or add an explicit bridge from loaded config into logging setup.

### `CommonArgs` advertises scripting flags that rebar never implements

**significant** · `README.md:90-97` · effort: medium

`--json` and `--version-only` are documented as behaviors every rebar-based app
receives, but in code they are only stored booleans. `CommonArgs` only ships
helpers for color and chdir. Downstream apps must implement the promised
scripting behavior themselves, which defeats the stated goal of a consistent
shared CLI surface.

**Remediation:** either document them as raw flags only, or add first-class
helpers/wiring so the behavior actually lives in the library.

### OTEL docs advertise `http/json`, but the feature set only ships protobuf-over-HTTP

**moderate** · `src/otel.rs:17-20` · effort: small

The docs list `http/json` as a supported OTLP protocol, but the dependency
configuration only enables `http-proto`, and the builder routes `http/json`
through the same `with_http()` path as `http/protobuf`. Users selecting
`http/json` silently get a different transport than the one documented.

**Remediation:** either remove `http/json` from the docs and reject unsupported
values, or add the actual JSON transport support.

*Verdict: this is the highest-value surface to fix first. The crate is more
trustworthy than the docs make it look because the implementation is often
stricter than the prose, but users only see the prose first.*

## The Builder Contract Surface

*The main builder API is close to ergonomic, but one escape hatch inherits
constraints from a different state machine branch.*

### Preloaded config path still requires deserialization-only bounds

**significant** · `src/lib.rs:555-626` · effort: medium

`with_config()` is presented as the escape hatch for already-materialized
configuration, yet the only public `start()` for `ConfiguredBuilder<C>` still
requires `DeserializeOwned + Default + Serialize`. Those bounds are needed for
discovery/file loading, not for the `Preloaded(C)` branch. In practice, the
escape hatch exists and then narrows itself at the final call site.

**Remediation:** split the preloaded path into its own builder state, or add a
separate `start()` path that does not inherit deserialization/default bounds.

*Verdict: one design bug, but it lands directly on library users. Fixing it
would materially improve rebar's claim to be a thin foundation crate rather
than a framework-shaped API.*

## The Error Handling Surface

*Typed errors are used throughout the crate, but a few public paths still erase
or distort failure information in caller-visible ways.*

### Caller-supplied cache TTL can overflow expiry arithmetic

**significant** · `src/cache.rs:69-80` · effort: small

`Cache::set()` uses unchecked `u64` addition for expiry arithmetic.
Pathologically large TTLs can panic in debug builds or wrap in release builds,
which makes cache semantics depend on build mode and caller input size.

**Remediation:** use checked or saturating addition and return a typed error for
unrepresentable expiries.

### Cache clearing reports success after discarding per-entry read and delete failures

**moderate** · `src/cache.rs:145-159` · effort: small

`Cache::clear()` flattens away directory-entry iterator errors and discards
`remove_file()` failures, then still returns `Ok(())`. That makes "best effort"
indistinguishable from "fully cleared" for callers.

**Remediation:** either return the first failure or introduce an explicitly
best-effort API that reports partial cleanup honestly.

### Update checks collapse transport and parse failures into the same None as no-update

**moderate** · `src/update.rs:89-138` · effort: medium

The update checker returns `Option<UpdateInfo>`, so client construction
failures, request failures, malformed JSON, and a genuine "no update available"
result all collapse into the same `None`. Debug logs are not a stable API for
library callers.

**Remediation:** add a typed error-preserving path such as
`Result<Option<UpdateInfo>, UpdateError>`, even if the current convenience API
stays as a thin wrapper.

*Verdict: this is a localized surface, not a systemic reliability problem. The
important work is to stop public APIs from quietly lying about what happened.*

## The Performance Surface

*The implementation is straightforward, but two featured surfaces pay steady
heap and copy costs that will show up under real load.*

### JSON logging clones span field maps and re-allocates field keys on every event

**significant** · `src/logging.rs:437-515` · effort: medium

`JsonLogLayer::on_event` clones accumulated span maps into each emitted event,
then re-owns field names and string values while also formatting debug fields
into fresh heap strings before serializing to JSON. In a logging-heavy service,
that moves logging overhead from incidental to structural.

**Remediation:** keep span fields in a merge-friendly representation and
serialize directly into the output buffer instead of rebuilding owned JSON state
for every event.

### HTTP response handling duplicates body buffers on the request path

**moderate** · `src/http.rs:124-174` · effort: small

After Hyper has already consolidated the body into `Bytes`, the code copies that
buffer into a `Vec<u8>` for `Response`, and `Response::text()` clones it again.
That is an unconditional O(n) copy tax on a surface the crate explicitly sells
for HTTP and update-check use.

**Remediation:** store the body in `Bytes` (or an equivalently cheap buffer)
and keep borrowed inspection paths borrow-based.

*Verdict: these are worth fixing because they sit on steady-state surfaces, not
cold setup code. They are not emergencies, but they are real tax on the
abstractions rebar exposes.*

## The Supply-Chain Surface

*The supported dependency gate is mostly clean, but one optional benchmark
feature still inherits an upstream unmaintained-crate warning.*

### The benchmark-only gungraun feature inherits an unmaintained bincode dependency

**advisory** · `Cargo.toml:82-83` · effort: small

`cargo audit` currently reports `RUSTSEC-2025-0141`: `bincode 1.3.3` is
unmaintained and reaches rebar only through `gungraun 0.17.2`, which is behind
the optional `bench-gungraun` feature. This is not on the default runtime
surface, but it is still repository-owned maintenance debt on a public feature.

**Remediation:** check whether a newer `gungraun` removes the edge; otherwise
document the feature as benchmark-only accepted risk or move the guidance
out-of-tree.

*Verdict: not an emergency and not a reason to distrust the default crate
surface, but it should not remain an undocumented warning either.*

## Remediation Ledger

| Finding | Concern | Location | Effort | Chains |
|---------|---------|----------|--------|--------|
| `shutdown-entry-point-is-not-usable-as-documented` | significant | `src/shutdown.rs:9-16` | small | — |
| `logging-config-log-dir-is-never-read` | significant | `README.md:182-186` | medium | related: `common-args-json-and-version-flags-are-no-ops` |
| `common-args-json-and-version-flags-are-no-ops` | significant | `README.md:90-97` | medium | related: `logging-config-log-dir-is-never-read` |
| `otel-http-json-protocol-is-documented-but-not-supported` | moderate | `src/otel.rs:17-20` | small | — |
| `preloaded-config-builder-requires-unused-serde-bounds` | significant | `src/lib.rs:555-626` | medium | — |
| `cache-set-ttl-overflow` | significant | `src/cache.rs:69-80` | small | — |
| `cache-clear-drops-delete-errors` | moderate | `src/cache.rs:145-159` | small | — |
| `update-check-hides-check-failures` | moderate | `src/update.rs:89-138` | medium | — |
| `logging-layer-clones-span-fields-per-event` | significant | `src/logging.rs:437-515` | medium | — |
| `http-response-body-is-copied-once-and-often-cloned-again` | moderate | `src/http.rs:124-174` | small | — |
| `bench-gungraun-pulls-unmaintained-bincode` | advisory | `Cargo.toml:82-83` | small | — |

<sub>
Generated 2026-04-12 at commit 89431ca. Intermediate artifacts: recon.yaml,
findings.yaml, report.html.
</sub>
