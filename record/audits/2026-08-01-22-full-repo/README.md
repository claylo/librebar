---
audit_date: 2026-08-01
project: librebar
commit: 3e4b96e7f4b81cfcc5ecd054b1b0340643afea8a
scope: "Full tracked repository: source, tests, examples, dependencies, configuration, CI/tooling, and documented public contracts"
auditor: gpt-5-generated
findings:
  critical: 0
  significant: 2
  moderate: 5
  advisory: 6
  note: 0
---

# Audit: Librebar

Librebar has a disciplined Rust baseline: Clippy, RustSec, cargo-deny, and
unused-dependency checks are clean, and no first-party unsafe-code finding
survived review. **The Documented Logging Contract Surface** contains the two
material defects: configuration-backed logging settings are not applied and an
"exact" log path is rewritten with a date suffix. **The Concurrency and Error
Surfaces** expose bounded lifecycle, cache-loss, and diagnostic-quality gaps;
the type, cookie, and dependency surfaces are contained improvements rather
than architectural defects.

---

## The Documented Logging Contract Surface

*The initialization order exists, but two documented logging contracts diverge
from the implementation in ways operators can observe immediately.*

### builder-ignores-configured-log-settings

The builder promises config-driven logging but ignores both `log_level` and
`log_dir`.

**significant** · `README.md:583-589` · effort: medium · <img src="assets/sparkline-builder-ignores-configured-log-settings.svg" height="14" alt="commit activity" />

`ConfiguredBuilder::start` loads the typed config, then initializes subsystems
without passing or inspecting it. Logging therefore uses only explicit builder
state and the literal `info` baseline. The runnable examples reinforce the
README contract by describing config-backed log settings that no builder path
reads.

```markdown README.md:583-589
## Builder API

The builder wires everything in the correct initialization order:

1. Load config (if requested)
2. Initialize logging (reads log settings from config if available)
3. Return `App<C>` with everything wired up
```

Related to [log-path-override-is-not-exact](#log-path-override-is-not-exact).

**Remediation:** Map the loaded top-level `log_level` and `log_dir` into
logging initialization while preserving CLI, environment, and explicit builder
precedence. Add integration coverage for both fields and their precedence.

<div>&hairsp;</div>

### log-path-override-is-not-exact

`LOG_PATH` is documented as an exact path but always receives a date suffix.

**significant** · `README.md:543-549` · effort: small · <img src="assets/sparkline-log-path-override-is-not-exact.svg" height="14" alt="commit activity" />

`build_log_writer` resolves `{APP}_LOG_PATH` into a target and then sends every
target through the daily appender. `/tmp/myapp.jsonl` consequently becomes
`/tmp/myapp.jsonl.2026-08-01`; a shipper or supervisor watching the documented
path sees no file.

```markdown README.md:543-549
### Log directory resolution

The logging system finds a writable log directory using this priority:

1. `{APP}_LOG_PATH` env var (exact file path)
2. `{APP}_LOG_DIR` env var (directory, appends `{app}.jsonl`)
3. `log_dir` from config
```

Related to
[builder-ignores-configured-log-settings](#builder-ignores-configured-log-settings).

**Remediation:** Preserve an explicit target mode. Open `{APP}_LOG_PATH`
exactly with a private non-rotating append writer; retain daily rotation for
directory-based and platform-default targets.

*Verdict: Both findings contradict explicit README promises. Repair the
config-to-logging handoff and exact-path mode before expanding the builder.*

---

## The Signal Lifecycle Surface

*Permanent process-wide signal handling is intentional, but repeated
registration creates independent loops whose escalation state can disagree.*

### detached-signal-task-outlives-app-lifecycle

Duplicate signal registrations retain stale forced-exit state.

**moderate** · `src/shutdown.rs:142-175` · effort: medium · <img src="assets/sparkline-detached-signal-task-outlives-app-lifecycle.svg" height="14" alt="commit activity" />

Every `register_signals` call spawns an independent loop with its own cloned
handle and escalation state. Tokio notifies every registered stream. If an old
registration has entered shutdown, a later App's first signal can also reach
the stale loop and force process exit before the later handle gets its graceful
shutdown opportunity.

```rust src/shutdown.rs:142-175
        let handle = self.clone();

        tracing::debug!("registering shutdown signal handlers");
        runtime.spawn(async move {
            loop {
                #[cfg(unix)]
                let signal = tokio::select! {
                    received = sigint.recv() => received.map(|()| ShutdownSignal::Interrupt),
                    received = sigterm.recv() => received.map(|()| ShutdownSignal::Terminate),
                };

                #[cfg(not(unix))]
                let signal = ctrl_c_signal(tokio::signal::ctrl_c().await);

                let Some(signal) = signal else {
                    tracing::error!("shutdown signal stream closed; signal task exiting");
                    return;
                };

                match action_for_signal(&handle, signal) {
                    SignalAction::Shutdown => {
                        tracing::info!(signal = signal.name(), "shutdown signal received");
                    }
                    SignalAction::Exit(exit_code) => {
                        tracing::warn!(
                            signal = signal.name(),
                            exit_code,
                            "repeated shutdown signal received; forcing exit"
                        );
                        std::process::exit(exit_code);
                    }
                }
            }
        });
```

**Remediation:** Preserve permanent process-wide handling, but use one
process-global dispatcher and escalation state. Reject duplicate registration
or atomically update the active `ShutdownHandle`.

*Verdict: The first shutdown path is sound. Duplicate registration needs one
process-global owner rather than another detached loop.*

---

## The Cache Concurrency Surface

*Atomic replacement protects individual writes, but pruning can act on a
pathname after its inspected inode has been replaced.*

### cache-prune-can-delete-concurrent-fresh-write

Cache pruning can unlink a fresh entry installed by a concurrent writer.

**moderate** · `src/cache.rs:188-204` · effort: medium · <img src="assets/sparkline-cache-prune-can-delete-concurrent-fresh-write.svg" height="14" alt="commit activity" />

A pruner can inspect an expired entry, then a concurrent `Cache::set` can
atomically rename a fresh entry onto that path before `remove_file`. The pruner
unlinks the replacement. Clone-local prune suppression does not serialize
writes, explicit pruning, independent cache instances, or other processes.

```rust src/cache.rs:188-204
            let path = entry.path();
            if !is_v2_cache_path(&path) {
                continue;
            }
            let expires_at = match read_expiry_at(&path) {
                Ok(expires_at) => expires_at,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to inspect cache entry while pruning");
                    continue;
                }
            };
            if now < expires_at {
                continue;
            }

            match std::fs::remove_file(&path) {
                Ok(()) => removed += 1,
```

**Remediation:** Serialize expiry inspection/removal with final replacement
using a per-cache advisory lock shared by `set`, explicit `prune`, and automatic
pruning. Avoid recursive acquisition when automatic pruning precedes a write.

*Verdict: The race causes cache loss rather than corruption or credential
exposure, but it is an actionable cross-task/process TOCTOU defect.*

---

## The Error Boundary Surface

*Librebar's crate-level errors are generally typed, but several public
subsystem boundaries collapse distinct failures into `Option`, `String`, or
unconditional success.*

### dispatch-resolution-errors-collapse-to-not-found

Dispatch resolution failures are reported as a missing plugin.

**moderate** · `src/dispatch.rs:38-53` · effort: small · <img src="assets/sparkline-dispatch-resolution-errors-collapse-to-not-found.svg" height="14" alt="commit activity" />

`None` means both "binary not found" and failure to join `PATH`, obtain the
current directory, or resolve a candidate. `run` converts all of them into
`Ok(None)`, so an operational failure becomes an unknown-command report.

```rust src/dispatch.rs:38-53
pub fn resolve(app_name: &str, subcommand: &str) -> Option<PathBuf> {
    let binary = subcommand_binary(app_name, subcommand);
    let path = std::env::var_os("PATH")?;
    let absolute_paths: Vec<_> = std::env::split_paths(&path)
        .filter(|entry| entry.is_absolute())
        .collect();
    if absolute_paths.is_empty() {
        return None;
    }

    let path = std::env::join_paths(absolute_paths).ok()?;
    let cwd = std::env::current_dir().ok()?;
    which::which_in(&binary, Some(path), cwd)
        .ok()
        .filter(|resolved| resolved.is_absolute())
}
```

**Remediation:** Add typed `try_resolve`, distinguish `NotFound` from
environment/filesystem/resolver failures, and make `run` use it. Keep
`resolve` as a compatibility wrapper.

<div>&hairsp;</div>

### crash-dump-errors-erased

Crash dump creation erases every failure cause.

**moderate** · `src/crash.rs:162-194` · effort: small · <img src="assets/sparkline-crash-dump-errors-erased.svg" height="14" alt="commit activity" />

Directory creation, exclusive open, serialization, writes, pruning, and
cleanup all return the same `None`. Ordinary callers cannot distinguish
permissions, a filename collision, disk exhaustion, or retention failure; a
failed cleanup can leave a partial file behind.

```rust src/crash.rs:162-194
pub fn write_crash_dump_to(info: &CrashInfo, dir: &Path) -> Option<PathBuf> {
    if std::fs::create_dir_all(dir).is_err() {
        return None;
    }

    // Use timestamp chars that are safe in filenames
    let ts = info.timestamp.replace([':', '.'], "-");
    let filename = format!("{}-{}.crash", info.app_name, ts);
    let path = dir.join(&filename);

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }

    let mut file = options.open(&path).ok()?;
    if serde_json::to_writer(&mut file, info).is_err() {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return None;
    }
    drop(file);

    if prune_crash_dumps(dir).is_err() {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(path)
}
```

**Remediation:** Add `CrashDumpError` plus
`try_write_crash_dump_to`. The panic hook may intentionally degrade the result;
ordinary callers should retain operation, path, and source context.

<div>&hairsp;</div>

### cache-clear-reports-partial-success

Cache clear returns success after skipped entries and failed removals.

**advisory** · `src/cache.rs:234-248` · effort: trivial · <img src="assets/sparkline-cache-clear-reports-partial-success.svg" height="14" alt="commit activity" />

Best-effort behavior is intentional, but the method name and return type do not
tell callers that directory-entry and removal failures can leave data behind
while `clear` returns `Ok(())`.

```rust src/cache.rs:234-248
    pub fn clear(&self) -> Result<()> {
        if self.dir.exists() {
            for entry in std::fs::read_dir(&self.dir)
                .map_err(CacheError::from)?
                .flatten()
            {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("cache") {
                    // Best-effort: skip files that can't be removed (permissions, etc.)
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        Ok(())
    }
```

**Remediation:** Document `clear` as best-effort. Add a separately named
strict/reporting API returning removed and failed paths; preserve the current
method's compatibility.

<div>&hairsp;</div>

### logging-resolution-uses-string-errors

Public log-target resolution strips typed I/O sources.

**advisory** · `src/logging.rs:188-217` · effort: medium · <img src="assets/sparkline-logging-resolution-uses-string-errors.svg" height="14" alt="commit activity" />

The resolver discards candidate-specific I/O errors and returns a generic
String. A caller cannot inspect the operation, path, `ErrorKind`, or source
chain, so permissions and filesystem failures are indistinguishable from a
platform without a usable log directory.

```rust src/logging.rs:188-217
pub fn resolve_log_target_with(
    service: &str,
    path_override: Option<PathBuf>,
    dir_override: Option<PathBuf>,
    config_dir: Option<PathBuf>,
) -> std::result::Result<LogTarget, String> {
    if let Some(path) = path_override {
        return log_target_from_path(path);
    }

    if let Some(dir) = dir_override {
        return log_target_from_dir(dir, service);
    }

    if let Some(dir) = config_dir {
        return log_target_from_dir(dir, service);
    }

    let candidates = default_log_candidates(platform_log_dir(service));

    let file_name = format!("{service}{LOG_FILE_SUFFIX}");

    for dir in candidates {
        if ensure_writable(&dir, &file_name).is_ok() {
            return Ok(LogTarget { dir, file_name });
        }
    }

    Err("No writable log directory found".to_string())
}
```

**Remediation:** Add typed `try_resolve_log_target_with` preserving candidate
failures. Keep `resolve_log_target_with` as the documented String-returning
compatibility wrapper.

*Verdict: No panic or process-killing error path was confirmed. The recurring
weakness is diagnostic fidelity at filesystem and resolver boundaries.*

---

## The Public Type Contract Surface

*The API is broadly idiomatic, but two public types permit semantically invalid
or needlessly awkward caller states.*

### release-info-allows-invalid-metadata

`ReleaseInfo` represents validated versions and release URLs as interchangeable
strings.

**advisory** · `src/update.rs:35-49` · effort: medium · <img src="assets/sparkline-release-info-allows-invalid-metadata.svg" height="14" alt="commit activity" />

Custom sources can transpose or malform two public strings and still satisfy
the trait contract. Boundary validation prevents unsafe use and degrades the
value to a debug-logged missed update, but the type permits the backend defect
at construction time.

```rust src/update.rs:35-49
pub struct ReleaseInfo {
    /// Latest available version.
    pub version: String,
    /// URL where the release can be viewed or installed.
    pub url: String,
}

impl ReleaseInfo {
    /// Create release information.
    pub fn new(version: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            url: url.into(),
        }
    }
```

**Remediation:** Add validated `ReleaseVersion` and `ReleaseUrl` newtypes plus
`ReleaseInfo::try_new`. Keep the current constructor and public fields through a
compatibility window; privatization belongs in the next major release.

<div>&hairsp;</div>

### http-client-is-not-cloneable

The pooled HTTP client cannot be cloned for concurrent reuse.

**advisory** · `src/http.rs:506-514` · effort: small · <img src="assets/sparkline-http-client-is-not-cloneable.svg" height="14" alt="commit activity" />

The client contains a cloneable boxed service, configuration, and shareable
cookie jar, but exposes no `Clone`. Callers must add `Arc` ceremony or construct
another client, fragmenting connection pools and TLS session state.

```rust src/http.rs:506-514
/// Uses rustls for TLS with Mozilla's CA root certificates.
/// HTTP/2 with HTTP/1.1 fallback. Connection pooling handled
/// automatically.
pub struct HttpClient {
    inner: HttpService,
    config: HttpClientConfig,
    #[cfg(feature = "http-cookies")]
    cookie_jar: Option<CookieJar>,
}
```

**Remediation:** Implement `Clone` for `HttpClientConfig` and `HttpClient`,
preserving shared cookie-jar semantics. Cover both `http` and `http-cookies`
feature sets.

*Verdict: These are compatibility-sensitive polish issues. Land them through
additive constructors and derives, not in-place API churn.*

---

## The Cookie Hot-Path Surface

*Cookie limits are bounded correctly, but enforcement pays whole-jar allocation
and sorting costs after responses that cannot mutate the jar.*

### cookie-limit-enforcement-scans-full-jar-on-every-response

Cookie limit enforcement clones and sorts the entire jar after every HTTP
response.

**moderate** · `src/http/cookies.rs:212-255` · effort: small · <img src="assets/sparkline-cookie-limit-enforcement-scans-full-jar-on-every-response.svg" height="14" alt="commit activity" />

Every response with a parseable URL calls `store_response`, even without a
`Set-Cookie` header. Enforcement inventories the jar three times, clones key
strings, groups domains, and sorts domain and global vectors while holding the
write lock on the async response path.

```rust src/http/cookies.rs:212-255
    fn enforce_limits(&self, store: &mut cookie_store::CookieStore) {
        let oversized = stored_cookie_keys(store)
            .into_iter()
            .filter(|key| key.size > self.limits.max_cookie_bytes)
            .collect::<Vec<_>>();
        for key in oversized {
            if store.remove(&key.domain, &key.path, &key.name).is_some() {
                tracing::warn!(
                    cookie_name = %key.name,
                    cookie_domain = %key.domain,
                    size = key.size,
                    limit = self.limits.max_cookie_bytes,
                    "dropping stored cookie that exceeds configured size limit"
                );
            }
        }

        let mut by_domain = BTreeMap::<String, Vec<StoredCookieKey>>::new();
        for key in stored_cookie_keys(store) {
            by_domain.entry(key.domain.clone()).or_default().push(key);
        }
        for cookies in by_domain.values_mut() {
            cookies.sort_unstable();
            let excess = cookies
                .len()
                .saturating_sub(self.limits.max_cookies_per_domain);
            evict_cookies(
                store,
                cookies.iter().take(excess),
                "per-domain cookie count",
                self.limits.max_cookies_per_domain,
            );
        }

        let mut cookies = stored_cookie_keys(store);
        cookies.sort_unstable();
        let excess = cookies.len().saturating_sub(self.limits.max_cookies_total);
        evict_cookies(
            store,
            cookies.iter().take(excess),
            "total cookie count",
            self.limits.max_cookies_total,
        );
    }
```

**Remediation:** Return before locking when no cookie can be stored. After a
mutation, build one inventory and reuse it for size, domain, and total checks;
skip sorting when counts are in bounds.

*Verdict: The default ceiling prevents unbounded growth, but not avoidable
latency and lock contention. The fast path and inventory reuse are local.*

---

## The Dependency Fitness Surface

*The dependency graph is advisory-clean and intentionally gated, with two small
default-feature leaks adding code current Librebar paths cannot reach.*

### otel-enables-unused-default-integrations

OpenTelemetry enables unused logging, metrics, and async-runtime integrations.

**advisory** · `Cargo.toml:59-63` · effort: small · <img src="assets/sparkline-otel-enables-unused-default-integrations.svg" height="14" alt="commit activity" />

The `otel` feature activates log/internal-log modules, metrics/log integration,
and `opentelemetry_sdk/rt-tokio`. Librebar builds a trace provider and owns the
blocking exporters' Tokio runtimes directly, leaving those integrations unused.

```toml Cargo.toml:59-63
opentelemetry = { version = "0.32", features = ["trace"], optional = true }
opentelemetry-http = { version = "0.32", default-features = false, features = ["hyper"], optional = true }
opentelemetry_sdk = { version = "0.32", features = ["trace", "rt-tokio"], optional = true }
opentelemetry-otlp = { version = "0.32", default-features = false, features = ["http-proto", "hyper-client", "trace"], optional = true }
tracing-opentelemetry = { version = "0.33", optional = true }
```

**Remediation:** Disable defaults on the three OpenTelemetry crates, retain
trace features, and remove SDK `rt-tokio`. Verify `otel`, `otel-http-json`, and
`otel-grpc` separately.

<div>&hairsp;</div>

### diagnostics-tar-enables-unused-xattr

Diagnostics enables tar's unused extended-attribute support.

**advisory** · `Cargo.toml:106-108` · effort: trivial · <img src="assets/sparkline-diagnostics-tar-enables-unused-xattr.svg" height="14" alt="commit activity" />

`tar` enables `xattr` by default, adding a Unix-native dependency path used for
archive extraction. Librebar's diagnostics capability only creates archives
with `tar::Builder`; it cannot reach extended-attribute restoration.

```toml Cargo.toml:106-108
flate2 = { version = "1.1", optional = true }
leakguard = { version = "0.8", default-features = false, features = ["std"], optional = true }
tar = { version = "0.4", optional = true }
```

**Remediation:** Set `default-features = false` on `tar` and verify the
diagnostics feature. No replacement dependency or source change is required.

*Verdict: RustSec, cargo-deny, and machete are clean. These are manifest-only
trimming opportunities, not supply-chain incidents.*

---

## Remediation Ledger

| Finding | Concern | Location | Effort | Chains |
|---------|---------|----------|--------|--------|
| | | **Documented Logging Contract Surface** | | |
| [builder-ignores-configured-log-settings](#builder-ignores-configured-log-settings) | significant | `README.md:583-589` | medium | related: log-path-override-is-not-exact |
| [log-path-override-is-not-exact](#log-path-override-is-not-exact) | significant | `README.md:543-549` | small | related: builder-ignores-configured-log-settings |
| | | **Signal Lifecycle Surface** | | |
| [detached-signal-task-outlives-app-lifecycle](#detached-signal-task-outlives-app-lifecycle) | moderate | `src/shutdown.rs:142-175` | medium | — |
| | | **Cache Concurrency Surface** | | |
| [cache-prune-can-delete-concurrent-fresh-write](#cache-prune-can-delete-concurrent-fresh-write) | moderate | `src/cache.rs:188-204` | medium | — |
| | | **Error Boundary Surface** | | |
| [dispatch-resolution-errors-collapse-to-not-found](#dispatch-resolution-errors-collapse-to-not-found) | moderate | `src/dispatch.rs:38-53` | small | — |
| [crash-dump-errors-erased](#crash-dump-errors-erased) | moderate | `src/crash.rs:162-194` | small | related: cache-clear, logging-resolution |
| [cache-clear-reports-partial-success](#cache-clear-reports-partial-success) | advisory | `src/cache.rs:234-248` | trivial | related: crash-dump-errors-erased |
| [logging-resolution-uses-string-errors](#logging-resolution-uses-string-errors) | advisory | `src/logging.rs:188-217` | medium | related: crash-dump-errors-erased |
| | | **Public Type Contract Surface** | | |
| [release-info-allows-invalid-metadata](#release-info-allows-invalid-metadata) | advisory | `src/update.rs:35-49` | medium | — |
| [http-client-is-not-cloneable](#http-client-is-not-cloneable) | advisory | `src/http.rs:506-514` | small | — |
| | | **Cookie Hot-Path Surface** | | |
| [cookie-limit-enforcement-scans-full-jar-on-every-response](#cookie-limit-enforcement-scans-full-jar-on-every-response) | moderate | `src/http/cookies.rs:212-255` | small | — |
| | | **Dependency Fitness Surface** | | |
| [otel-enables-unused-default-integrations](#otel-enables-unused-default-integrations) | advisory | `Cargo.toml:59-63` | small | — |
| [diagnostics-tar-enables-unused-xattr](#diagnostics-tar-enables-unused-xattr) | advisory | `Cargo.toml:106-108` | trivial | — |

<sub>
Generated 2026-08-01 at commit 3e4b96e.
Intermediate artifacts: recon.yaml, findings.yaml.
</sub>
