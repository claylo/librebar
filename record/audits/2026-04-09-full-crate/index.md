---
audit_date: 2026-04-09
project: rebar
commit: f7a028c82848c1f80398059de89e5f2e43c29162
scope: Full crate audit — src/, Cargo.toml, build configuration, dependency tree
auditor: claude-opus-4-6 (crustoleum rubric, 6 parallel agents, 84 criteria)
findings:
  critical: 0
  significant: 1
  moderate: 4
  advisory: 6
  note: 2
---

# Audit: rebar

`rebar` is a Rust application foundation crate providing feature-gated
modules for CLI, config, logging, shutdown, HTTP, caching, and more.
The crate demonstrates strong safety discipline — `#![deny(unsafe_code)]`,
zero unsafe blocks, clean clippy, and disciplined error propagation with
no `unwrap()` or `panic!` anywhere. **The Supply Chain Surface** has
`opentelemetry-otlp` default features pulling reqwest and ~50 transitive
crates unnecessarily, and the `http` feature omits `serde_json` despite
JSON being the primary response format consumers need. **The Error Handling
Surface** erases concrete error types behind `Box<dyn Error>` and has
five undocumented silent discards. **The Performance Surface** shows
per-event String allocations in the logging layer's hot path. **The API
Design Surface** carries ~130 lines of duplicated builder methods and
missing `Debug` derives on several public types. **The Memory Safety
Surface** and **The Concurrency Surface** are both clean. Fix the otel
dependency and annotate the silent discards, and this is a solid
foundation crate.

---

## The Supply Chain Surface

*Dependency choices are deliberate and well-gated, except
opentelemetry-otlp whose default features smuggle a redundant HTTP
stack into the tree.*

<div>&hairsp;</div>

### opentelemetry-otlp default features pull reqwest unnecessarily {#otel-otlp-default-features}

**significant** · `Cargo.toml:53` · effort: trivial · <img src="assets/sparkline-otel-otlp-default-features.svg" height="14" alt="commit activity" />

The `opentelemetry-otlp` dependency specifies `features = ["http-proto",
"hyper-client"]` but does not set `default-features = false`. The crate's
defaults include `reqwest-blocking-client`, `logs`, `metrics`, and
`internal-logs` — pulling reqwest, tower, tower-http, and roughly 50
additional transitive crates. This creates a redundant HTTP stack
alongside rebar's own hyper-based client, and reqwest is a dependency
the author explicitly avoids.

```toml Cargo.toml:53
opentelemetry-otlp = { version = "0.31", features = ["http-proto", "hyper-client"], optional = true }
```

> A crate that went to the trouble of wrapping hyper directly, then
> accidentally ships reqwest in the trunk.

**Remediation:** Add `default-features = false` and include `trace`
explicitly:

```toml
opentelemetry-otlp = { version = "0.31", default-features = false, features = ["http-proto", "hyper-client", "trace"], optional = true }
```

<div>&hairsp;</div>

### HTTP feature omits serde_json despite JSON being the primary response format {#http-feature-missing-serde-json}

**moderate** · `Cargo.toml:100`, `src/http.rs:166-185`, `src/update.rs:121-122` · effort: small · <img src="assets/sparkline-http-feature-missing-serde-json.svg" height="14" alt="commit activity" />

The `http` feature provides an HTTP client whose `Response` offers
`.text()` and `.bytes()`, but no JSON deserialization — and `serde_json`
is not in the feature's dependency list. The crate's own `update` module
proves the gap: it enables `http` then immediately needs `serde_json` to
parse the GitHub API response, requiring `update` to add `dep:serde_json`
explicitly. Any consumer using the `http` feature for API calls — the
overwhelmingly common case — will hit the same two-step.

```toml Cargo.toml:100
http = ["dep:hyper", "dep:hyper-util", "dep:http-body-util", "dep:hyper-rustls", "dep:rustls", "dep:tokio"]
```

```rust src/update.rs:121-122
        let body = resp.text().ok()?;
        let json: serde_json::Value = serde_json::from_str(&body).ok()?;
```

> The HTTP client can fetch JSON, it just can't read it. Every caller
> independently rediscovers this.

Related: [response-text-body-clone](#response-text-body-clone),
[update-ok-chain-context-loss](#update-ok-chain-context-loss).

**Remediation:** Add `dep:serde_json` to the `http` feature and add a
`json()` method to `Response`:

```rust
pub fn json<T: serde::de::DeserializeOwned>(&self) -> crate::Result<T> {
    serde_json::from_slice(&self.body)
        .map_err(|e| crate::Error::Http(Box::new(e)))
}
```

<div>&hairsp;</div>

### deny.toml references wrong project name {#deny-toml-stale-comment}

**note** · `.config/deny.toml:1` · effort: trivial · <img src="assets/sparkline-deny-toml-stale-comment.svg" height="14" alt="commit activity" />

The comment header reads `# Security policy configuration for ah-ah-ah`
instead of rebar. Likely copied from another project during scaffolding.

```toml .config/deny.toml:1
# Security policy configuration for ah-ah-ah
```

**Remediation:** Update the comment to reference rebar.

*Verdict: The feature gating strategy is well-designed — 29 direct
dependencies are carefully scoped behind 12 feature flags, and the
serde-saphyr-over-serde_yaml and hyper-direct-over-reqwest choices show
deliberate dependency thinking. The otel default-features fix is a
one-line change. The http/serde_json gap is a small but real API
completeness issue that the crate's own update module already works
around.*

<!-- whitespace is important -->
<div>&nbsp;</div>

## The Error Handling Surface

*Error propagation is consistent and panic-free, but the error type
architecture erases concrete causes behind `Box<dyn Error>`, and
several silent discards lack documentation of intent.*

<div>&hairsp;</div>

### Error variants erase concrete source types behind Box<dyn Error> {#error-type-erasure}

**moderate** · `src/error.rs:7-72` · effort: medium · <img src="assets/sparkline-error-type-erasure.svg" height="14" alt="commit activity" />

Eleven of thirteen `Error` variants wrap
`Box<dyn Error + Send + Sync>`, fully erasing the concrete error type.
A caller who catches `Error::Http` cannot programmatically distinguish a
timeout from a TLS failure from a DNS error without string-matching the
`Display` output. For a foundation library, this limits downstream
error handling — consumers cannot implement retry-on-timeout or
report-on-TLS-mismatch without fragile text parsing.

```rust src/error.rs:7-16
#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to parse config file {path}: {source}")]
    ConfigParse {
        path: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("failed to deserialize config: {0}")]
    ConfigDeserialize(Box<dyn std::error::Error + Send + Sync>),
```

Related: [update-ok-chain-context-loss](#update-ok-chain-context-loss).

**Remediation:** For the most actionable variants (`Http`, `Cache`),
introduce per-module error enums with concrete variants. Wrap them in the
top-level `Error` via `#[error(transparent)]` / `#[from]`. Init-time
errors (`OtelInit`, `TracingInit`) are reasonable as `Box<dyn Error>`.

<div>&hairsp;</div>

### Five let _ = discards lack intent documentation {#silent-error-discards}

**moderate** · `src/logging.rs:448`, `src/shutdown.rs:41`, `src/cache.rs:114,152`, `src/update.rs:129` · effort: trivial · <img src="assets/sparkline-silent-error-discards.svg" height="14" alt="commit activity" />

Five sites use `let _ =` to discard Results without commenting on why
the error is acceptable. The most concerning is `logging.rs:448` where
a write failure in the JSON log layer silently drops log lines with no
fallback. In a tracing Layer callback, returning an error is not
possible, but an `eprintln!` fallback would surface the problem.

```rust src/logging.rs:448
            let _ = writer.write_all(&buf);
```

> A logging framework that silently loses logs is the kind of irony that
> only surfaces at 3 AM when you need the logs most.

**Remediation:** Add a one-line comment above each `let _ =` explaining
the discard. For `logging.rs:448`, consider an `eprintln!` fallback.

<div>&hairsp;</div>

### Recursive deep_merge has no depth bound {#deep-merge-no-depth-limit}

**advisory** · `src/config.rs:111-120` · effort: small · <img src="assets/sparkline-deep-merge-no-depth-limit.svg" height="14" alt="commit activity" />

`deep_merge` is a public function that recurses to the depth of the
overlay value's nesting. Config files are typically shallow (<5 levels),
but a pathologically nested YAML/JSON file could exhaust the stack. The
attack surface is local-only — the user controls which files are loaded.

```rust src/config.rs:111-120
pub fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                deep_merge(base_map.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, overlay) => *base = overlay,
    }
}
```

**Remediation:** Add a depth counter parameter or document the
stack-depth assumption. A max of 64 levels is generous for any config
format.

<div>&hairsp;</div>

### Update check silently drops parse errors without logging {#update-ok-chain-context-loss}

**advisory** · `src/update.rs:121-125` · effort: trivial · <img src="assets/sparkline-update-ok-chain-context-loss.svg" height="14" alt="commit activity" />

Lines 121-122 use `.ok()?` to convert typed errors (UTF-8 decode, JSON
parse) into `None`, discarding the error. The HTTP request failure on
line 108 correctly logs at debug level before returning `None`, but
these parse failures do not. If the GitHub API changes its response
format, update checks will silently return "no update" with no
diagnostic trail.

```rust src/update.rs:121-125
        let body = resp.text().ok()?;
        let json: serde_json::Value = serde_json::from_str(&body).ok()?;
        let tag = json.get("tag_name")?.as_str()?;
        let latest = tag.strip_prefix('v').unwrap_or(tag);
        let html_url = json.get("html_url")?.as_str().unwrap_or("");
```

Related: [error-type-erasure](#error-type-erasure).

**Remediation:** Add `tracing::debug!` before the `.ok()?` on lines
121-122, consistent with the pattern already used on line 111.

*Verdict: The crate has zero `unwrap()`, zero `panic!`, and disciplined
`?` propagation throughout — a strong baseline. The concerns are
structural, not correctness bugs: type erasure limits programmatic error
handling, undocumented discards create a maintenance trap, and the
recursive merge lacks bounds. None are urgent for 0.1.0 but all should
be addressed before 1.0.*

<!-- whitespace is important -->
<div>&nbsp;</div>

## The Performance Surface

*The logging hot path allocates several Strings per event that could be
avoided, and `Response::text()` clones the body unnecessarily in the
common case.*

<div>&hairsp;</div>

### Per-event String allocations in JSON log layer {#logging-hot-path-allocations}

**moderate** · `src/logging.rs:419-450` · effort: small · <img src="assets/sparkline-logging-hot-path-allocations.svg" height="14" alt="commit activity" />

Every tracing event triggers: (1) three `.to_string()` calls for static
keys ("timestamp", "level", "target"), (2) a `.to_lowercase()` allocation
for the level string despite only 5 possible values, (3) a `.to_string()`
for the target module path. The level `.to_lowercase()` is the most
avoidable — a match returning `&'static str` eliminates both the
allocation and the Unicode scan.

```rust src/logging.rs:422-431
        let timestamp = format_timestamp();
        map.insert("timestamp".to_string(), Value::String(timestamp));
        map.insert(
            "level".to_string(),
            Value::String(event.metadata().level().as_str().to_lowercase()),
        );
        map.insert(
            "target".to_string(),
            Value::String(event.metadata().target().to_string()),
        );
```

The non-blocking writer means these allocations happen off the
application thread, which significantly reduces their direct impact on
caller latency.

**Remediation:** Replace the level formatting with a match:

```rust
let level = match *event.metadata().level() {
    Level::TRACE => "trace",
    Level::DEBUG => "debug",
    Level::INFO => "info",
    Level::WARN => "warn",
    Level::ERROR => "error",
};
```

<div>&hairsp;</div>

### Response::text() clones body with no borrowing alternative {#response-text-body-clone}

**advisory** · `src/http.rs:172-174` · effort: trivial · <img src="assets/sparkline-response-text-body-clone.svg" height="14" alt="commit activity" />

`text()` takes `&self` and must clone the body `Vec<u8>` to pass
ownership to `String::from_utf8`. The common call site
(`update.rs:121`) uses `resp.text().ok()?` then discards the
`Response` — a consuming variant would avoid the copy entirely.

```rust src/http.rs:172-174
    pub fn text(&self) -> std::result::Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }
```

**Remediation:** Add `fn into_text(self)` that consumes without cloning,
and optionally `fn text_ref(&self) -> Result<&str, Utf8Error>` for
zero-copy reads.

<div>&hairsp;</div>

### Span field map cloned per event in logging layer {#span-fields-clone}

**advisory** · `src/logging.rs:433-438` · effort: medium · <img src="assets/sparkline-span-fields-clone.svg" height="14" alt="commit activity" />

Each event clones the `Map<String, Value>` from every span in scope. The
tracing extensions API provides only shared references, so the clone is
structurally necessary. For shallow span hierarchies with few fields, the
cost is small. For deep instrumented stacks, it scales linearly.

```rust src/logging.rs:433-438
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if let Some(fields) = span.extensions().get::<SpanFields>() {
                    map.extend(fields.values.clone());
                }
            }
        }
```

**Remediation:** If profiling shows this is a bottleneck, store span
fields as `Arc<Map<String, Value>>` and clone the Arc (~15ns) instead
of the map contents.

*Verdict: For a foundation library, performance is appropriate. The
logging allocations happen in the non-blocking writer thread, the
Response clone is for small API payloads, and the span fields clone is
structurally inherent. The level-string `.to_lowercase()` is the
easiest win — one match statement eliminates an allocation and Unicode
scan per log event.*

<!-- whitespace is important -->
<div>&nbsp;</div>

## The API Design Surface

*The type-state builder pattern is well-conceived but carries ~130 lines
of duplicated methods, and several public types are missing standard
trait implementations.*

<div>&hairsp;</div>

### Builder and ConfiguredBuilder duplicate ~130 lines of identical methods {#builder-method-duplication}

**advisory** · `src/lib.rs:300-462`, `src/lib.rs:586-648` · effort: medium · <img src="assets/sparkline-builder-method-duplication.svg" height="14" alt="commit activity" />

Eight methods (`with_cli`, `logging`, `with_log_dir`, `otel`, `shutdown`,
`crash_handler`, `with_version`, `cli_flags` x2) are duplicated verbatim
between `Builder` and `ConfiguredBuilder<C>`. Additionally, the `start()`
methods share ~50 lines of identical tracing-subscriber composition
logic. Changes to init behavior must be made in two places.

```rust src/lib.rs:302-306
    #[cfg(feature = "cli")]
    pub fn with_cli(mut self, common: cli::CommonArgs) -> Self {
        self.cli = Some(common);
        self
    }
```

**Remediation:** Extract common fields into a `BuilderInner` struct and
delegate the shared methods, or use `macro_rules!` to generate them. The
shared `start()` tail can be extracted into a private
`init_subsystems()` function.

<div>&hairsp;</div>

### Multiple public types missing Debug derive {#missing-debug-derives}

**advisory** · `src/http.rs:31`, `src/cache.rs:35`, `src/update.rs:44`, `src/diagnostics.rs:190` · effort: trivial · <img src="assets/sparkline-missing-debug-derives.svg" height="14" alt="commit activity" />

`HttpClientConfig`, `Cache`, `UpdateChecker`, and `DebugBundle` are
public types with all-`Debug` fields but no `Debug` derive. The Rust
API Guidelines recommend `Debug` on all public types. Without it,
consumers cannot use `{:?}` for diagnostics.

```rust src/http.rs:31-36
pub struct HttpClientConfig {
    pub user_agent: String,
    pub timeout: Duration,
}
```

**Remediation:** Add `#[derive(Debug)]` to the four types. For
`App<C>`, `Builder`, and `ConfiguredBuilder<C>`, add `Debug` with
appropriate bounds.

<div>&hairsp;</div>

### DebugBundle::add_text/add_bytes return Result but cannot fail {#infallible-result-return}

**note** · `src/diagnostics.rs:209-218` · effort: trivial · <img src="assets/sparkline-infallible-result-return.svg" height="14" alt="commit activity" />

Both methods push to a `Vec` and return `Ok(())` unconditionally. The
`Result` return type forces callers to handle a `?` that can never
trigger.

```rust src/diagnostics.rs:209-213
    pub fn add_text(&mut self, name: &str, content: &str) -> Result<()> {
        self.files
            .push((name.to_string(), content.as_bytes().to_vec()));
        Ok(())
    }
```

**Remediation:** Change return type to `&mut Self` for builder-style
chaining, or simply `()`.

<div>&hairsp;</div>

### DoctorCheck trait lacks Send bound {#doctor-check-not-send}

**advisory** · `src/diagnostics.rs:35-43` · effort: trivial · <img src="assets/sparkline-doctor-check-not-send.svg" height="14" alt="commit activity" />

`DoctorCheck` has no `Send` supertrait, which means `DoctorRunner` is
`!Send` if any registered check is `!Send`. This prevents the runner
from being used across thread boundaries or in async contexts.

```rust src/diagnostics.rs:35-38
pub trait DoctorCheck {
    fn name(&self) -> &str;
    fn category(&self) -> &str;
    fn run(&self) -> CheckResult;
```

**Remediation:** Add `Send` as a supertrait before the 1.0 boundary:
`pub trait DoctorCheck: Send`.

*Verdict: The API is ergonomic and idiomatic — references where
appropriate, owned types at storage boundaries, correct type-state
transitions. The builder duplication and missing Debug derives are
maintenance risks, not correctness issues. All findings are addressable
without breaking changes.*

<!-- whitespace is important -->
<div>&nbsp;</div>

## The Memory Safety Surface

*The crate denies unsafe code at the root level, uses no raw pointers,
no `MaybeUninit`, no manual memory management, and all Drop
implementations are correct.*

No findings. `#![deny(unsafe_code)]` at `lib.rs:124` provides a
compile-time guarantee against the introduction of unsafe blocks. All
Box usage is for trait objects (required), all Vec instances are bounded
and short-lived, and the three Drop implementations (`OtelGuard`,
`LockGuard`, `LoggingGuard`) correctly manage their resources without
panicking.

*Verdict: Clean.*

---

## The Concurrency Surface

*Async code is minimal, well-bounded, and uses appropriate primitives
with no shared mutable state across module boundaries.*

No findings. No `std::sync::Mutex`, no `Arc`, no `unsafe impl
Send/Sync`, no lock-across-await patterns. The shutdown watch channel
is sound and idempotent. The HTTP timeout correctly covers the full
request lifecycle (connect + body). The only minor note is synchronous
filesystem I/O in `cache.rs` called from the async `update::check()`,
which is acceptable for a startup-time operation.

*Verdict: Clean.*

<!-- whitespace is important -->
<div>&nbsp;</div>

## Remediation Ledger

| Finding | Concern | Location | Effort | Chains |
|---------|---------|----------|--------|--------|
| | | **Supply Chain** | | |
| [otel-otlp-default-features](#otel-otlp-default-features) | significant | `Cargo.toml:53` | trivial | -- |
| [http-feature-missing-serde-json](#http-feature-missing-serde-json) | moderate | `Cargo.toml:100` | small | related: response-text-body-clone |
| [deny-toml-stale-comment](#deny-toml-stale-comment) | note | `.config/deny.toml:1` | trivial | -- |
| | | **Error Handling** | | |
| [error-type-erasure](#error-type-erasure) | moderate | `src/error.rs:7-72` | medium | related: update-ok-chain-context-loss |
| [silent-error-discards](#silent-error-discards) | moderate | `src/logging.rs:448` +4 | trivial | -- |
| [deep-merge-no-depth-limit](#deep-merge-no-depth-limit) | advisory | `src/config.rs:111-120` | small | -- |
| [update-ok-chain-context-loss](#update-ok-chain-context-loss) | advisory | `src/update.rs:121-125` | trivial | related: error-type-erasure |
| | | **Performance** | | |
| [logging-hot-path-allocations](#logging-hot-path-allocations) | moderate | `src/logging.rs:419-450` | small | related: span-fields-clone |
| [response-text-body-clone](#response-text-body-clone) | advisory | `src/http.rs:172-174` | trivial | -- |
| [span-fields-clone](#span-fields-clone) | advisory | `src/logging.rs:433-438` | medium | related: logging-hot-path-allocations |
| | | **API Design** | | |
| [builder-method-duplication](#builder-method-duplication) | advisory | `src/lib.rs:300-462,586-648` | medium | -- |
| [missing-debug-derives](#missing-debug-derives) | advisory | 4 files | trivial | -- |
| [infallible-result-return](#infallible-result-return) | note | `src/diagnostics.rs:209-218` | trivial | -- |
| [doctor-check-not-send](#doctor-check-not-send) | advisory | `src/diagnostics.rs:35-43` | trivial | -- |

---

<sub>
Generated 2026-04-09 at commit f7a028c.
Intermediate artifacts: recon.yaml, findings.yaml.
</sub>
