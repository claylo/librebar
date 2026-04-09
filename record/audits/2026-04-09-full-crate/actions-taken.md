---
audit: 2026-04-09-full-crate
last_updated: 2026-04-09
status:
  fixed: 10
  mitigated: 0
  accepted: 1
  disputed: 0
  deferred: 2
  open: 0
---

# Actions Taken: rebar Full Crate Audit

Summary of remediation status for the [2026-04-09 full crate audit](index.md).

---

## 2026-04-09 — Disable opentelemetry-otlp default features

**Disposition:** fixed
**Addresses:** [otel-otlp-default-features](index.md#opentelemetry-otlp-default-features-pull-reqwest-unnecessarily)
**Commit:** 4f5c8a5
**Author:** Clay Loveless

Added `default-features = false` and explicit `trace` feature to the opentelemetry-otlp dependency in Cargo.toml. This prevents reqwest, tower, and tower-http from entering the dependency tree when the `otel` feature is enabled — eliminating roughly 50 transitive crates.

```toml Cargo.toml:53
opentelemetry-otlp = { version = "0.31", default-features = false, features = ["http-proto", "hyper-client", "trace"], optional = true }
```

---

## 2026-04-09 — Fix stale project name in deny.toml

**Disposition:** fixed
**Addresses:** [deny-toml-stale-comment](index.md#denytoml-references-wrong-project-name)
**Commit:** 4f5c8a5
**Author:** Clay Loveless

Changed the header comment in `.config/deny.toml` from "ah-ah-ah" to "rebar" — a leftover from the project template.

---

## 2026-04-09 — Add Response::json(), into_text(), text_ref() and serde_json to http feature

**Disposition:** fixed
**Addresses:** [http-feature-missing-serde-json](index.md#http-feature-omits-serde_json-despite-json-being-the-primary-response-format), [response-text-body-clone](index.md#responsetext-clones-body-with-no-borrowing-alternative)
**Commit:** pending (fix/audit-remediation branch)
**Author:** Clay Loveless

Added `dep:serde_json` to the `http` feature in Cargo.toml. Added three methods to `Response`: `json::<T>()` (deserializes directly from body bytes via `serde_json::from_slice`), `into_text(self)` (consuming, avoids clone), and `text_ref(&self)` (zero-copy borrow as `&str`). Simplified `update.rs` to use `resp.json()` instead of the `text().ok()? -> from_str()` two-step.

---

## 2026-04-09 — Log parse errors in update check, replace .ok()? with explicit match

**Disposition:** fixed
**Addresses:** [update-ok-chain-context-loss](index.md#update-check-silently-drops-parse-errors-without-logging)
**Commit:** pending (fix/audit-remediation branch)
**Author:** Clay Loveless

Replaced `resp.text().ok()?` / `serde_json::from_str(&body).ok()?` in `update.rs` with an explicit `match resp.json()` that logs the error at debug level before returning `None`. This is consistent with the HTTP error handling pattern already used on the line above.

---

## 2026-04-09 — Document silent error discards, add eprintln fallback in logging

**Disposition:** fixed
**Addresses:** [silent-error-discards](index.md#five-let--discards-lack-intent-documentation)
**Commit:** pending (fix/audit-remediation branch)
**Author:** Clay Loveless

Added intent comments above all five `let _ =` sites: `logging.rs` (log write), `shutdown.rs` (watch channel send), `cache.rs` (expired entry cleanup, clear iteration), `update.rs` (cache write). The `logging.rs` write discard was upgraded from silent to an `eprintln!` fallback so broken log sinks are visible.

---

## 2026-04-09 — Level string match replaces to_lowercase() in logging hot path

**Disposition:** fixed
**Addresses:** [logging-hot-path-allocations](index.md#per-event-string-allocations-in-json-log-layer)
**Commit:** pending (fix/audit-remediation branch)
**Author:** Clay Loveless

Replaced `.as_str().to_lowercase()` with a match returning `&'static str` for the five tracing levels. Eliminates per-event String allocation and Unicode scan. The static key allocations (`"timestamp".to_string()`, etc.) remain — eliminating those requires a direct-to-buffer JSON writer, which is a larger refactor not justified without profiling data.

---

## 2026-04-09 — Add Debug derives to public types

**Disposition:** fixed
**Addresses:** [missing-debug-derives](index.md#multiple-public-types-missing-debug-derive)
**Commit:** pending (fix/audit-remediation branch)
**Author:** Clay Loveless

Added `#[derive(Debug)]` to `HttpClientConfig`, `Cache`, `UpdateChecker`, and `DebugBundle`.

---

## 2026-04-09 — DebugBundle add_text/add_bytes return &mut Self, DoctorCheck gains Send

**Disposition:** fixed
**Addresses:** [infallible-result-return](index.md#debugbundleadd_textadd_bytes-return-result-but-cannot-fail), [doctor-check-not-send](index.md#doctorcheck-trait-lacks-send-bound)
**Commit:** pending (fix/audit-remediation branch)
**Author:** Clay Loveless

Changed `add_text`, `add_bytes`, and `add_doctor_results` return types from `Result<()>` to `&mut Self` for builder-style chaining. Added `Send` as a supertrait to `DoctorCheck` so `DoctorRunner` can be passed across thread boundaries.

---

## 2026-04-09 — Add depth limit to deep_merge

**Disposition:** fixed
**Addresses:** [deep-merge-no-depth-limit](index.md#recursive-deep_merge-has-no-depth-bound)
**Commit:** pending (fix/audit-remediation branch)
**Author:** Clay Loveless

`deep_merge` now delegates to an inner function with a depth counter. Recursion beyond 64 levels returns `Error::ConfigDeserialize`. The public signature changed from `fn deep_merge(&mut, Value)` to `fn deep_merge(&mut, Value) -> Result<()>` — acceptable for pre-1.0.

---

## 2026-04-09 — Extract BuilderInner to eliminate builder method duplication

**Disposition:** fixed
**Addresses:** [builder-method-duplication](index.md#builder-and-configuredbuilder-duplicate-130-lines-of-identical-methods)
**Commit:** pending (fix/audit-remediation branch)
**Author:** Clay Loveless

Extracted `BuilderInner` struct holding all shared fields, plus a `builder_methods!` macro generating the 7 builder methods on both `Builder` and `ConfiguredBuilder`. The config transition methods (`config`, `config_from_file`, `with_config`) now move `self.inner` in a single field rather than copying 8 cfg-gated fields individually. A `SubsystemInit` struct and `init_subsystems()` method deduplicate the ~100 lines of tracing/shutdown/crash setup from both `start()` methods. Net reduction: ~530 lines to ~390.

---

## 2026-04-09 — Accept span field clone as known characteristic

**Disposition:** accepted
**Addresses:** [span-fields-clone](index.md#span-field-map-cloned-per-event-in-logging-layer)
**Commit:** n/a
**Author:** Clay Loveless

The `fields.values.clone()` in the logging layer is structurally required by the tracing extensions API (which provides only shared references). The clone cost is bounded by span depth and field count, and the non-blocking writer already moves this off the application thread. An `Arc<Map>` refactor would reduce clone cost but adds complexity to `on_new_span` and `on_record`. Without profiling data showing this is a bottleneck, the current approach is the right tradeoff.

---

## 2026-04-09 — Defer error type refinement to pre-1.0

**Disposition:** deferred
**Addresses:** [error-type-erasure](index.md#error-variants-erase-concrete-source-types-behind-boxdyn-error)
**Commit:** n/a — tracked for pre-1.0 milestone
**Author:** Clay Loveless

The `Box<dyn Error>` pattern in 11/13 error variants is pragmatic for 0.1.0. The most actionable variants (Http, Cache) would benefit from concrete error enums, but the right variants to expose depend on downstream usage patterns that don't exist yet. Init-time errors (OtelInit, TracingInit, ShutdownInit) are reasonable as Box<dyn Error> since callers rarely inspect those. Deferring to pre-1.0 when real consumers inform the design.
