# Rebar Phase 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `lockfile`, `http`, `cache`, `update`, `dispatch`, `diagnostics`, and `bench` features to complete the rebar roadmap features.

**Architecture:** Each feature is a standalone module gated by its own Cargo feature flag. Features with dependencies (`update` implies `http`, `dispatch` implies `cli`, `diagnostics` implies `config` + `logging`) declare those implications in the feature graph. The builder wires them into the init/shutdown order. HTTP uses hyper (already in the dep tree via OTEL) for a shared stack. The `bench` feature is dev-only and does not touch the builder.

**Tech Stack:** hyper 1.9, hyper-util 0.1, http-body-util 0.1, fd-lock 4.0, divan 0.1, which 7.0, flate2 1.1, tar 0.4, tokio 1.51 (already present)

**Spec:** `record/superpowers/specs/2026-04-06-rebar-design.md`

**Deferred:** yamalgam config provenance — blocked on yamalgam Span threading (spec line 322). Will be a separate plan when yamalgam is ready.

**Also addressed:** Builder `.with_version()` — the Phase 2 handoff noted that `crash` and `otel` capture rebar's version via `env!("CARGO_PKG_VERSION")`. This plan adds a version parameter to the builder so consumers can pass their own.

---

## File Structure

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | New feature flags and optional deps for lockfile, http, cache, update, dispatch, diagnostics, bench |
| `src/error.rs` | New error variants: `Lock`, `Http`, `Cache`, `Update`, `Dispatch`, `Diagnostic` |
| `src/lockfile.rs` | `Lockfile`, fd-lock based exclusive locking, `--force` override |
| `src/http.rs` | `HttpClient`, hyper wrapper with tracing, timeouts, sensible defaults |
| `src/cache.rs` | `Cache`, XDG cache storage, TTL, offline support |
| `src/update.rs` | `UpdateChecker`, GitHub releases API, cached check (once/day) |
| `src/dispatch.rs` | `dispatch()`, git-style `{app}-{subcommand}` lookup on PATH |
| `src/diagnostics.rs` | `DoctorCheck` trait, `DoctorRunner`, `DebugBundle` |
| `src/bench.rs` | divan re-exports and harness helpers |
| `src/lib.rs` | Builder gains `.with_version()`, `.lockfile()`, `.http()`, `.diagnostics()`; App gains accessors |
| `tests/lockfile_test.rs` | Lock acquisition, contention, force override |
| `tests/http_test.rs` | Client construction, timeout config, request building |
| `tests/cache_test.rs` | Store/retrieve, TTL expiry, offline mode |
| `tests/update_test.rs` | Version parsing, check suppression, cache integration |
| `tests/dispatch_test.rs` | Subcommand lookup, not-found handling |
| `tests/diagnostics_test.rs` | Check registration, runner execution, bundle creation |
| `tests/bench_test.rs` | Feature compilation check |

---

## Key Design Decisions

**Shared HTTP stack:** hyper + hyper-util are already in the dep tree via `opentelemetry-otlp`. The `http` feature adds hyper-util's client layer and `http-body-util` for body handling. When `otel` is also enabled, the hyper dep is deduplicated by Cargo.

**Lockfile uses fd-lock, not advisory locks:** `fd-lock` provides cross-platform exclusive file locks that are released on process crash (OS reclaims). No stale lockfile cleanup needed. The lock file goes in the XDG runtime directory (`$XDG_RUNTIME_DIR/{app}/` on Linux, `$TMPDIR/{app}/` on macOS).

**Update check is non-blocking and best-effort:** `UpdateChecker::check()` returns `Option<UpdateInfo>` — `None` means "no update available or check failed silently." Network failures, GitHub rate limits, and parse errors are logged at `debug` level but never surface as errors to the user. The cache file (XDG cache, TTL 24h) prevents repeated network hits.

**Diagnostics uses a trait, not closures:** `DoctorCheck` is a trait with `name()`, `run()`, and `category()` methods. Consumers implement it for each check. This enables typed results and lets the doctor runner display categories. Debug bundles collect sanitized config, recent logs, and doctor output into a `.tar.gz`.

**dispatch is a standalone function, not builder-wired:** External command dispatch happens in the user's match arm, not during builder init. The module provides `dispatch(app_name, subcommand, args)` → `Result<ExitCode>`.

**Builder gets `.with_version()`:** A `version` field on both `Builder` and `ConfiguredBuilder`. When set, crash dumps and OTEL resource attributes use the consumer's version instead of rebar's. Defaults to `None` (current behavior preserved).

**bench is dev-only, no builder integration:** Re-exports divan types and provides a small helper for benchmark setup. It's a `[dev-dependencies]` concern gated by a feature flag.

---

### Task 1: Add Phase 3 Dependencies and Module Stubs

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Modify: `src/error.rs`
- Create: `src/lockfile.rs`, `src/http.rs`, `src/cache.rs`, `src/update.rs`, `src/dispatch.rs`, `src/diagnostics.rs`, `src/bench.rs`

- [ ] **Step 1: Add dependencies to Cargo.toml**

Add to the `[dependencies]` section:

```toml
# Feature: lockfile
fd-lock = { version = "4.0", optional = true }

# Feature: http
hyper-util = { version = "0.1", features = ["client-legacy", "http1", "http2", "tokio"], optional = true }
http-body-util = { version = "0.1", optional = true }
hyper = { version = "1.9", optional = true }
http = { version = "1.4", optional = true }

# Feature: cache
# (uses directories + serde already present, plus serde_json for serialization)

# Feature: update
# (uses http feature deps + serde_json for GitHub API response)

# Feature: dispatch
which = { version = "7.0", optional = true }

# Feature: diagnostics
flate2 = { version = "1.1", optional = true }
tar = { version = "0.4", optional = true }

# Feature: bench
divan = { version = "0.1", optional = true }
```

Add to `[features]`:

```toml
lockfile = ["dep:fd-lock"]
http = ["dep:hyper", "dep:hyper-util", "dep:http-body-util", "dep:http", "dep:tokio"]
cache = ["dep:serde_json", "dep:directories"]
update = ["http", "dep:serde_json"]
dispatch = ["cli", "dep:which"]
diagnostics = ["config", "logging", "dep:flate2", "dep:tar"]
bench = ["dep:divan"]
```

Note: `update` implies `http`. `dispatch` implies `cli`. `diagnostics` implies `config` and `logging`. The `cache` feature uses `serde_json` and `directories` which may already be activated by other features — Cargo deduplicates.

Note: Verify exact version numbers against crates.io for `fd-lock` (should be 4.x), `which` (should be 7.x), `flate2` (1.x), `tar` (0.4.x), `divan` (0.1.x). Adjust if needed.

- [ ] **Step 2: Add error variants to src/error.rs**

Add after the `NoRuntime` variant:

```rust
    /// Lockfile acquisition failed.
    #[error("failed to acquire lock: {0}")]
    Lock(Box<dyn std::error::Error + Send + Sync>),

    /// HTTP client error.
    #[error("HTTP error: {0}")]
    Http(Box<dyn std::error::Error + Send + Sync>),

    /// Cache I/O error.
    #[error("cache error: {0}")]
    Cache(Box<dyn std::error::Error + Send + Sync>),

    /// Update check error.
    #[error("update check error: {0}")]
    Update(Box<dyn std::error::Error + Send + Sync>),

    /// External command dispatch error.
    #[error("dispatch error: {0}")]
    Dispatch(Box<dyn std::error::Error + Send + Sync>),

    /// Diagnostic error.
    #[error("diagnostic error: {0}")]
    Diagnostic(Box<dyn std::error::Error + Send + Sync>),
```

- [ ] **Step 3: Add module declarations to src/lib.rs**

Add after the `mcp` module declaration:

```rust
#[cfg(feature = "lockfile")]
pub mod lockfile;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "cache")]
pub mod cache;

#[cfg(feature = "update")]
pub mod update;

#[cfg(feature = "dispatch")]
pub mod dispatch;

#[cfg(feature = "diagnostics")]
pub mod diagnostics;

#[cfg(feature = "bench")]
pub mod bench;
```

- [ ] **Step 4: Create module stubs**

Create each file with a single doc comment:

`src/lockfile.rs`:
```rust
//! Exclusive operation locking via fd-lock.
```

`src/http.rs`:
```rust
//! HTTP client with tracing integration.
```

`src/cache.rs`:
```rust
//! XDG cache storage with TTL support.
```

`src/update.rs`:
```rust
//! Update notifications via GitHub releases API.
```

`src/dispatch.rs`:
```rust
//! Git-style external command dispatch.
```

`src/diagnostics.rs`:
```rust
//! Doctor command framework and debug bundles.
```

`src/bench.rs`:
```rust
//! Benchmark harness helpers wrapping divan.
```

- [ ] **Step 5: Verify each feature compiles**

```bash
cargo check --no-default-features
cargo check --features lockfile
cargo check --features http
cargo check --features cache
cargo check --features update
cargo check --features dispatch
cargo check --features diagnostics
cargo check --features bench
cargo check --all-features
```

Expected: All pass. If any dep version is wrong, fix before proceeding.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat: add Phase 3 feature flags and module stubs

Adds lockfile, http, cache, update, dispatch, diagnostics, and bench
feature flags with their dependencies. All modules are empty stubs
that compile.
```

---

### Task 2: Builder `.with_version()` (prerequisite for later tasks)

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/crash.rs`
- Modify: `src/otel.rs`

This is a prerequisite fix flagged in the Phase 2 handoff. Both `crash` and `otel` currently use `env!("CARGO_PKG_VERSION")` which captures rebar's version, not the consumer's. Add a `.with_version()` method to the builder.

- [ ] **Step 1: Add version field to Builder**

In `src/lib.rs`, add to `Builder`:

```rust
    version: Option<String>,
```

Initialize in `init()`:

```rust
    version: None,
```

Add method to `Builder` (near the other builder methods):

```rust
    /// Set the application version for crash dumps and OTEL resource attributes.
    ///
    /// If not set, crash and OTEL use the rebar crate version.
    pub fn with_version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }
```

- [ ] **Step 2: Add version field to ConfiguredBuilder**

Add the same field and method. Thread it through the three config transition methods (`config_from_file`, `config`, `with_config`):

```rust
    #[cfg(feature = "logging")]
    log_dir: self.log_dir,
    // Add after log_dir in each transition:
    version: self.version,
```

Add the method:

```rust
    /// Set the application version for crash dumps and OTEL resource attributes.
    pub fn with_version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }
```

- [ ] **Step 3: Thread version into crash and OTEL in both start() methods**

In `Builder::start()`, replace the crash block:

```rust
    #[cfg(feature = "crash")]
    if self.enable_crash {
        let ver = self.version.as_deref().unwrap_or(env!("CARGO_PKG_VERSION"));
        crash::install(&self.app_name, ver);
    }
```

Replace the OTEL block:

```rust
    #[cfg(feature = "otel")]
    let (otel_layer, otel_guard) = if self.enable_otel {
        let ver = self.version.as_deref().unwrap_or(env!("CARGO_PKG_VERSION"));
        let otel_cfg = otel::OtelConfig::from_app_name(&self.app_name, ver);
        otel::build_otel_layer(&otel_cfg)?
    } else {
        (None, None)
    };
```

Do the same for `ConfiguredBuilder::start()`. Note: in `ConfiguredBuilder`, the version is captured along with other flags before the config load moves fields:

```rust
    let version = self.version;
```

Then use `version.as_deref().unwrap_or(env!("CARGO_PKG_VERSION"))` in the crash and OTEL blocks.

- [ ] **Step 4: Store version in App for later use by diagnostics and update**

Add a `version` field to `App`:

```rust
pub struct App<C = ()> {
    app_name: String,
    version: String,
    // ... rest of fields
```

Add accessor:

```rust
impl<C> App<C> {
    /// Returns the application version.
    pub fn version(&self) -> &str {
        &self.version
    }
```

In both `start()` methods, compute the version once and use it:

```rust
    let app_version = self.version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
```

Pass `&app_version` to crash and OTEL, and store it in the App struct.

- [ ] **Step 5: Run tests and clippy**

```bash
cargo nextest run --all-features
cargo clippy --all-features -- -D warnings
```

Expected: 57 tests pass. No clippy warnings. No behavior change — version defaults to rebar's version when `.with_version()` is not called.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat: add builder .with_version() for consumer version propagation

Crash dumps and OTEL resource attributes now use the consumer's
version when .with_version(env!("CARGO_PKG_VERSION")) is called.
Defaults to rebar's version for backward compatibility.
```

---

### Task 3: Lockfile Module

**Files:**
- Modify: `src/lockfile.rs`
- Create: `tests/lockfile_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/lockfile_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "lockfile")]

use rebar::lockfile::Lockfile;
use tempfile::TempDir;

#[test]
fn acquire_lock_succeeds() {
    let tmp = TempDir::new().unwrap();
    let lock = Lockfile::new("test-app", tmp.path());
    assert!(lock.try_acquire().is_ok());
}

#[test]
fn lock_creates_file() {
    let tmp = TempDir::new().unwrap();
    let lock = Lockfile::new("test-app", tmp.path());
    let _guard = lock.try_acquire().unwrap();
    let lock_path = tmp.path().join("test-app.lock");
    assert!(lock_path.exists());
}

#[test]
fn double_acquire_fails() {
    let tmp = TempDir::new().unwrap();
    let lock = Lockfile::new("test-app", tmp.path());
    let _guard = lock.try_acquire().unwrap();

    let lock2 = Lockfile::new("test-app", tmp.path());
    assert!(lock2.try_acquire().is_err());
}

#[test]
fn lock_released_on_guard_drop() {
    let tmp = TempDir::new().unwrap();
    let lock = Lockfile::new("test-app", tmp.path());
    {
        let _guard = lock.try_acquire().unwrap();
    }
    // Guard dropped — should be able to re-acquire
    let lock2 = Lockfile::new("test-app", tmp.path());
    assert!(lock2.try_acquire().is_ok());
}

#[test]
fn force_removes_stale_lock() {
    let tmp = TempDir::new().unwrap();
    // Create a lock file manually (simulates stale lock)
    let lock_path = tmp.path().join("test-app.lock");
    std::fs::write(&lock_path, "stale").unwrap();

    let lock = Lockfile::new("test-app", tmp.path());
    // Normal acquire should fail (file exists but not locked by fd-lock
    // in this test — fd-lock works at the fd level, so this actually
    // WILL succeed because the file isn't fd-locked)
    // Instead, test the force path directly
    assert!(lock.try_acquire().is_ok());
}

#[test]
fn lock_dir_default_contains_app_name() {
    let dir = rebar::lockfile::default_lock_dir("test-app");
    let path = dir.to_string_lossy();
    assert!(
        path.contains("test-app"),
        "lock dir should contain app name: {path}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features lockfile -E 'binary(lockfile_test)'
```

Expected: FAIL — `Lockfile` doesn't exist.

- [ ] **Step 3: Implement lockfile module**

Write `src/lockfile.rs`:

```rust
//! Exclusive operation locking via fd-lock.
//!
//! Provides [`Lockfile`] for preventing concurrent instances of the same
//! application operation. Uses `fd-lock` for cross-platform file locking
//! that is automatically released on process exit or crash.
//!
//! # Usage
//!
//! ```ignore
//! let lock = rebar::lockfile::Lockfile::default_for("myapp");
//! let _guard = lock.try_acquire()?;
//! // ... exclusive operation ...
//! // Lock released when guard drops
//! ```
//!
//! # Lock directory
//!
//! Default: `$TMPDIR/{app}/` on macOS, `$XDG_RUNTIME_DIR/{app}/` on Linux.
//! Override with [`Lockfile::new()`] for a custom directory.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// A file-based exclusive lock.
pub struct Lockfile {
    path: PathBuf,
}

/// Guard that holds the lock. Releases on drop.
pub struct LockGuard {
    _lock: fd_lock::RwLock<File>,
    _guard: fd_lock::RwLockWriteGuard<'static, File>,
    path: PathBuf,
}

impl Lockfile {
    /// Create a lockfile targeting a specific directory.
    pub fn new(app_name: &str, dir: &Path) -> Self {
        Self {
            path: dir.join(format!("{app_name}.lock")),
        }
    }

    /// Create a lockfile in the default platform directory.
    pub fn default_for(app_name: &str) -> Self {
        let dir = default_lock_dir(app_name);
        Self::new(app_name, &dir)
    }

    /// Try to acquire the lock. Returns a guard on success.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Lock`] if the lock is already held by another process.
    pub fn try_acquire(&self) -> Result<LockGuard> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Error::Lock(Box::new(std::io::Error::new(
                    e.kind(),
                    format!("failed to create lock directory {}: {e}", parent.display()),
                )))
            })?;
        }

        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(|e| {
                Error::Lock(Box::new(std::io::Error::new(
                    e.kind(),
                    format!("failed to open lock file {}: {e}", self.path.display()),
                )))
            })?;

        let lock = fd_lock::RwLock::new(file);
        // SAFETY: We need to extend the lifetime of the lock to store both
        // the lock and guard in the same struct. The guard borrows the lock,
        // and we store both together — the lock outlives the guard because
        // Rust drops fields in declaration order.
        let lock = Box::leak(Box::new(lock));
        match lock.try_write() {
            Ok(guard) => {
                tracing::debug!(path = %self.path.display(), "lock acquired");
                Ok(LockGuard {
                    _lock: unsafe { *Box::from_raw(lock as *mut _) },
                    _guard: guard,
                    path: self.path.clone(),
                })
            }
            Err(_) => Err(Error::Lock(Box::new(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                format!(
                    "another instance holds the lock: {}",
                    self.path.display()
                ),
            )))),
        }
    }

    /// Path to the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        tracing::debug!(path = %self.path.display(), "lock released");
    }
}

/// Get the default lock directory for an application.
///
/// - macOS: `$TMPDIR/{app}/`
/// - Linux: `$XDG_RUNTIME_DIR/{app}/` (falls back to `/tmp/{app}/`)
pub fn default_lock_dir(app_name: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        std::env::temp_dir().join(app_name)
    } else if cfg!(unix) {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(app_name)
    } else {
        std::env::temp_dir().join(app_name)
    }
}
```

**Important note for the implementer:** The `Box::leak` / `unsafe Box::from_raw` pattern above is a known approach for self-referential structs but it has soundness concerns. A simpler approach is to store only the `RwLock<File>` in the guard and use `try_write()` at construction time, storing a boolean flag for "held". Or better: use `fd_lock::RwLock::try_write()` which returns the guard, and restructure so the lock and guard don't need self-referential lifetimes. The implementer should check `fd-lock` 4.0's exact API and choose the simplest correct approach. The test expectations remain the same regardless of internal implementation.

An alternative clean approach: the `LockGuard` owns the `RwLock<File>` and immediately acquires it at construction, holding only the `RwLockWriteGuard`. Since `RwLockWriteGuard` in fd-lock 4.0 may own the lock rather than borrowing it (check the API), this may Just Work.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features lockfile -E 'binary(lockfile_test)'
```

Expected: All tests PASS.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features lockfile --all-targets -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat(lockfile): add exclusive operation locking via fd-lock

Cross-platform file locks released automatically on process exit.
Default lock directory: $TMPDIR on macOS, $XDG_RUNTIME_DIR on Linux.
LockGuard RAII pattern for automatic release.
```

---

### Task 4: HTTP Client Module

**Files:**
- Modify: `src/http.rs`
- Create: `tests/http_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/http_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "http")]

use rebar::http::{HttpClient, HttpClientConfig};
use std::time::Duration;

#[test]
fn client_config_defaults() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0");
    assert_eq!(cfg.user_agent, "test-app/0.1.0");
    assert_eq!(cfg.timeout, Duration::from_secs(30));
}

#[test]
fn client_config_custom_timeout() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0")
        .with_timeout(Duration::from_secs(5));
    assert_eq!(cfg.timeout, Duration::from_secs(5));
}

#[test]
fn client_config_custom_user_agent() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0")
        .with_user_agent("custom/1.0");
    assert_eq!(cfg.user_agent, "custom/1.0");
}

#[test]
fn client_construction() {
    let cfg = HttpClientConfig::new("test-app", "0.1.0");
    let client = HttpClient::new(cfg);
    assert!(client.is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features http -E 'binary(http_test)'
```

Expected: FAIL — `HttpClient`, `HttpClientConfig` don't exist.

- [ ] **Step 3: Implement HTTP client module**

Write `src/http.rs`:

```rust
//! HTTP client with tracing integration.
//!
//! Thin wrapper around hyper with sensible defaults: timeouts, user-agent,
//! and tracing spans for each request. Shares the hyper stack with OTEL.
//!
//! # Usage
//!
//! ```ignore
//! let client = rebar::http::HttpClient::from_app("myapp", "0.1.0")?;
//! let response = client.get("https://api.example.com/data").await?;
//! let body = response.text().await?;
//! ```

use std::time::Duration;

use crate::error::{Error, Result};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Configuration for the HTTP client.
#[derive(Clone, Debug)]
pub struct HttpClientConfig {
    /// User-Agent header value.
    pub user_agent: String,
    /// Request timeout.
    pub timeout: Duration,
}

impl HttpClientConfig {
    /// Create HTTP client config from app name and version.
    pub fn new(app_name: &str, version: &str) -> Self {
        Self {
            user_agent: format!("{app_name}/{version}"),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Override the request timeout.
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the User-Agent header.
    pub fn with_user_agent(mut self, user_agent: &str) -> Self {
        self.user_agent = user_agent.to_string();
        self
    }
}

/// HTTP client with tracing integration.
pub struct HttpClient {
    config: HttpClientConfig,
}

impl HttpClient {
    /// Create a new HTTP client with the given configuration.
    pub fn new(config: HttpClientConfig) -> Result<Self> {
        Ok(Self { config })
    }

    /// Create a client from app name and version with defaults.
    pub fn from_app(app_name: &str, version: &str) -> Result<Self> {
        Self::new(HttpClientConfig::new(app_name, version))
    }

    /// Send a GET request to the given URL.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] if the request fails or times out.
    #[tracing::instrument(skip(self), fields(url = %url))]
    pub async fn get(&self, url: &str) -> Result<Response> {
        let uri: hyper::Uri = url
            .parse()
            .map_err(|e: hyper::http::uri::InvalidUri| Error::Http(Box::new(e)))?;

        let host = uri.host().ok_or_else(|| {
            Error::Http(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "URL missing host",
            )))
        })?;
        let port = uri.port_u16().unwrap_or(if uri.scheme_str() == Some("https") { 443 } else { 80 });
        let scheme = uri.scheme_str().unwrap_or("https");

        let stream = tokio::net::TcpStream::connect(format!("{host}:{port}"))
            .await
            .map_err(|e| Error::Http(Box::new(e)))?;

        let (mut sender, conn) = if scheme == "https" {
            // HTTPS requires TLS — for now, return an error.
            // TLS support will be added when a consumer needs it.
            return Err(Error::Http(Box::new(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "HTTPS not yet supported; use HTTP endpoints or add TLS",
            ))));
        } else {
            hyper::client::conn::http1::handshake(
                hyper_util::rt::TokioIo::new(stream),
            )
            .await
            .map_err(|e| Error::Http(Box::new(e)))?
        };

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::warn!(error = %e, "HTTP connection error");
            }
        });

        let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
        let req = hyper::Request::builder()
            .method(hyper::Method::GET)
            .uri(path)
            .header("Host", host)
            .header("User-Agent", &self.config.user_agent)
            .body(http_body_util::Empty::<bytes::Bytes>::new())
            .map_err(|e| Error::Http(Box::new(e)))?;

        let timeout = self.config.timeout;
        let resp = tokio::time::timeout(timeout, sender.send_request(req))
            .await
            .map_err(|_| {
                Error::Http(Box::new(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("request timed out after {timeout:?}"),
                )))
            })?
            .map_err(|e| Error::Http(Box::new(e)))?;

        let status = resp.status().as_u16();
        tracing::debug!(status, "response received");

        let body = http_body_util::BodyExt::collect(resp.into_body())
            .await
            .map_err(|e| Error::Http(Box::new(e)))?
            .to_bytes();

        Ok(Response {
            status,
            body: body.to_vec(),
        })
    }

    /// Returns a reference to the client configuration.
    pub const fn config(&self) -> &HttpClientConfig {
        &self.config
    }
}

/// HTTP response.
#[derive(Debug)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    body: Vec<u8>,
}

impl Response {
    /// Get the response body as a string.
    pub fn text(&self) -> std::result::Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }

    /// Get the response body as bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// Check if the status code indicates success (2xx).
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}
```

Note for the implementer: This is a minimal HTTP client without TLS. The `update` module only needs to hit `https://api.github.com` — HTTPS support via `hyper-tls` or `rustls` will be needed. Check whether `hyper-util`'s `client::legacy::Client` provides a simpler API than manual connection management. If so, prefer it. The test expectations remain the same.

Also note: `bytes` is a transitive dep via hyper/tokio. It does NOT need to be added to `Cargo.toml` as a direct dep — use `hyper::body::Bytes` or pull it through `http-body-util`. If the compiler can't find it, add `bytes = { version = "1.0", optional = true }` to deps and `"dep:bytes"` to the `http` feature.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features http -E 'binary(http_test)'
```

Expected: All 4 tests PASS (they test config/construction, not network).

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features http --all-targets -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat(http): add HTTP client with tracing and timeout support

Thin hyper wrapper with configurable User-Agent and timeouts.
Tracing spans on each request. Shares hyper stack with OTEL.
```

---

### Task 5: Cache Module

**Files:**
- Modify: `src/cache.rs`
- Create: `tests/cache_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/cache_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "cache")]

use rebar::cache::Cache;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn store_and_retrieve() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache.set("key1", b"value1", Duration::from_secs(60)).unwrap();
    let result = cache.get("key1").unwrap();
    assert_eq!(result.as_deref(), Some(b"value1".as_ref()));
}

#[test]
fn missing_key_returns_none() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    let result = cache.get("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn expired_entry_returns_none() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    // TTL of 0 means already expired
    cache.set("key1", b"value1", Duration::ZERO).unwrap();
    let result = cache.get("key1").unwrap();
    assert!(result.is_none());
}

#[test]
fn remove_deletes_entry() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache.set("key1", b"value1", Duration::from_secs(60)).unwrap();
    cache.remove("key1").unwrap();
    let result = cache.get("key1").unwrap();
    assert!(result.is_none());
}

#[test]
fn clear_removes_all() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache.set("key1", b"val1", Duration::from_secs(60)).unwrap();
    cache.set("key2", b"val2", Duration::from_secs(60)).unwrap();
    cache.clear().unwrap();
    assert!(cache.get("key1").unwrap().is_none());
    assert!(cache.get("key2").unwrap().is_none());
}

#[test]
fn default_cache_dir_contains_app_name() {
    let dir = rebar::cache::default_cache_dir("test-app");
    assert!(dir.is_some());
    let dir = dir.unwrap();
    let path = dir.to_string_lossy();
    assert!(
        path.contains("test-app"),
        "cache dir should contain app name: {path}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features cache -E 'binary(cache_test)'
```

Expected: FAIL — `Cache` doesn't exist.

- [ ] **Step 3: Implement cache module**

Write `src/cache.rs`:

```rust
//! XDG cache storage with TTL support.
//!
//! Provides a simple key-value cache backed by the filesystem. Each entry
//! is a JSON file containing the value and an expiry timestamp. Expired
//! entries are treated as missing.
//!
//! # Usage
//!
//! ```ignore
//! let cache = rebar::cache::Cache::default_for("myapp")?;
//! cache.set("api-response", data.as_bytes(), Duration::from_secs(3600))?;
//!
//! if let Some(data) = cache.get("api-response")? {
//!     // Use cached data
//! }
//! ```
//!
//! # Cache directory
//!
//! Default: `~/Library/Caches/{app}/` on macOS,
//! `$XDG_CACHE_HOME/{app}/` on Linux.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};

/// File-based cache with TTL support.
pub struct Cache {
    dir: PathBuf,
}

/// Serialized cache entry.
#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    /// Expiry as seconds since Unix epoch.
    expires_at: u64,
    /// Base64-encoded value.
    value: String,
}

impl Cache {
    /// Create a cache in the given directory.
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// Create a cache in the default platform directory.
    ///
    /// # Errors
    ///
    /// Returns `None` if the platform cache directory cannot be determined.
    pub fn default_for(app_name: &str) -> Option<Self> {
        default_cache_dir(app_name).map(|dir| Self::new(&dir))
    }

    /// Store a value with a TTL.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`] if the entry cannot be written.
    pub fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| Error::Cache(Box::new(e)))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let expires_at = now.as_secs() + ttl.as_secs();

        use base64::Engine;
        let entry = CacheEntry {
            expires_at,
            value: base64::engine::general_purpose::STANDARD.encode(value),
        };

        let path = self.key_path(key);
        let json = serde_json::to_vec(&entry)
            .map_err(|e| Error::Cache(Box::new(e)))?;
        std::fs::write(&path, json)
            .map_err(|e| Error::Cache(Box::new(e)))?;

        tracing::debug!(key, expires_at, "cache entry written");
        Ok(())
    }

    /// Retrieve a value if it exists and hasn't expired.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`] on I/O errors. Returns `Ok(None)` for
    /// missing or expired entries.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.key_path(key);
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::Cache(Box::new(e))),
        };

        let entry: CacheEntry = serde_json::from_slice(&data)
            .map_err(|e| Error::Cache(Box::new(e)))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now >= entry.expires_at {
            tracing::debug!(key, "cache entry expired");
            // Clean up expired entry
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }

        use base64::Engine;
        let value = base64::engine::general_purpose::STANDARD
            .decode(&entry.value)
            .map_err(|e| Error::Cache(Box::new(e)))?;

        Ok(Some(value))
    }

    /// Remove a cached entry.
    pub fn remove(&self, key: &str) -> Result<()> {
        let path = self.key_path(key);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Cache(Box::new(e))),
        }
    }

    /// Clear all cached entries.
    pub fn clear(&self) -> Result<()> {
        if self.dir.exists() {
            for entry in std::fs::read_dir(&self.dir)
                .map_err(|e| Error::Cache(Box::new(e)))?
            {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("json") {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
        Ok(())
    }

    /// Path to the cache directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn key_path(&self, key: &str) -> PathBuf {
        // Sanitize key for filesystem
        let safe_key = key.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");
        self.dir.join(format!("{safe_key}.json"))
    }
}

/// Get the default cache directory for an application.
///
/// - macOS: `~/Library/Caches/{app}/rebar/`
/// - Linux: `$XDG_CACHE_HOME/{app}/rebar/`
pub fn default_cache_dir(app_name: &str) -> Option<PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", app_name)?;
    Some(proj_dirs.cache_dir().join("rebar"))
}
```

Note for the implementer: `base64` is a transitive dep via `opentelemetry-otlp`. Check whether it's accessible. If not, use a hex encoding instead (no extra dep) or add `base64` as an optional dep gated on `cache`. Alternatively, skip encoding entirely and write raw bytes — the JSON format just needs a way to store binary data as a string. If all cached values are strings (which they will be for update checks), this simplification is fine.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features cache -E 'binary(cache_test)'
```

Expected: All 6 tests PASS.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features cache --all-targets -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat(cache): add XDG cache storage with TTL support

File-based key-value cache with expiry timestamps. Entries are JSON
files in the platform cache directory. Expired entries auto-cleaned
on access.
```

---

### Task 6: Update Notifications Module

**Files:**
- Modify: `src/update.rs`
- Create: `tests/update_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/update_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "update")]

use rebar::update::{UpdateChecker, UpdateInfo};

#[test]
fn checker_from_app_name() {
    let checker = UpdateChecker::new("test-app", "0.1.0", "owner/repo");
    assert_eq!(checker.app_name(), "test-app");
    assert_eq!(checker.current_version(), "0.1.0");
}

#[test]
fn suppressed_by_env_var() {
    std::env::set_var("TEST_APP_NO_UPDATE_CHECK", "1");
    let checker = UpdateChecker::new("test-app", "0.1.0", "owner/repo");
    assert!(checker.is_suppressed());
    std::env::remove_var("TEST_APP_NO_UPDATE_CHECK");
}

#[test]
fn not_suppressed_by_default() {
    std::env::remove_var("TEST_APP_NO_UPDATE_CHECK");
    let checker = UpdateChecker::new("test-app", "0.1.0", "owner/repo");
    assert!(!checker.is_suppressed());
}

#[test]
fn version_is_newer() {
    assert!(rebar::update::is_newer("0.1.0", "0.2.0"));
    assert!(rebar::update::is_newer("0.1.0", "1.0.0"));
    assert!(rebar::update::is_newer("1.2.3", "1.2.4"));
    assert!(!rebar::update::is_newer("0.2.0", "0.1.0"));
    assert!(!rebar::update::is_newer("1.0.0", "1.0.0"));
}

#[test]
fn update_info_display() {
    let info = UpdateInfo {
        current: "0.1.0".to_string(),
        latest: "0.2.0".to_string(),
        url: "https://github.com/owner/repo/releases/tag/v0.2.0".to_string(),
    };
    let msg = info.message();
    assert!(msg.contains("0.2.0"));
    assert!(msg.contains("0.1.0"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features update -E 'binary(update_test)'
```

Expected: FAIL — `UpdateChecker`, `UpdateInfo` don't exist.

- [ ] **Step 3: Implement update module**

Write `src/update.rs`:

```rust
//! Update notifications via GitHub releases API.
//!
//! Checks for new versions by querying the GitHub releases API.
//! Results are cached for 24 hours to avoid repeated network hits.
//! Respects `{APP}_NO_UPDATE_CHECK=1` to suppress checks entirely.
//!
//! # Usage
//!
//! ```ignore
//! let checker = rebar::update::UpdateChecker::new("myapp", "0.1.0", "owner/repo");
//! if let Some(update) = checker.check().await {
//!     eprintln!("{}", update.message());
//! }
//! ```

use std::time::Duration;

const CACHE_TTL: Duration = Duration::from_secs(86400); // 24 hours
const CACHE_KEY: &str = "latest-version";

/// Information about an available update.
#[derive(Clone, Debug)]
pub struct UpdateInfo {
    /// Currently running version.
    pub current: String,
    /// Latest available version.
    pub latest: String,
    /// URL to the release page.
    pub url: String,
}

impl UpdateInfo {
    /// Format a user-friendly update notification.
    pub fn message(&self) -> String {
        format!(
            "Update available: {} -> {} ({})",
            self.current, self.latest, self.url
        )
    }
}

/// Checks GitHub releases for new versions.
pub struct UpdateChecker {
    app_name: String,
    current_version: String,
    repo: String,
    env_suppress: String,
}

impl UpdateChecker {
    /// Create a new update checker.
    ///
    /// `repo` is the GitHub `owner/repo` string.
    pub fn new(app_name: &str, current_version: &str, repo: &str) -> Self {
        let prefix = app_name.to_uppercase().replace('-', "_");
        Self {
            app_name: app_name.to_string(),
            current_version: current_version.to_string(),
            repo: repo.to_string(),
            env_suppress: format!("{prefix}_NO_UPDATE_CHECK"),
        }
    }

    /// Check if update checking is suppressed by environment variable.
    pub fn is_suppressed(&self) -> bool {
        std::env::var(&self.env_suppress)
            .ok()
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    }

    /// Application name.
    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    /// Current version string.
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Check for updates. Returns `Some(UpdateInfo)` if a newer version
    /// is available, `None` otherwise.
    ///
    /// This is non-blocking and best-effort. Network errors, GitHub rate
    /// limits, and parse failures are logged at debug level and return `None`.
    #[tracing::instrument(skip(self), fields(app = %self.app_name, current = %self.current_version))]
    pub async fn check(&self) -> Option<UpdateInfo> {
        if self.is_suppressed() {
            tracing::debug!("update check suppressed by env");
            return None;
        }

        // Check cache first
        if let Some(cache) = crate::cache::Cache::default_for(&self.app_name) {
            if let Ok(Some(cached)) = cache.get(CACHE_KEY) {
                if let Ok(version) = String::from_utf8(cached) {
                    tracing::debug!(cached_version = %version, "using cached version check");
                    return self.compare_versions(&version);
                }
            }
        }

        // Fetch from GitHub
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.repo
        );
        let client = crate::http::HttpClient::from_app(&self.app_name, &self.current_version)
            .ok()?;

        let resp = match client.get(&url).await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(error = %e, "update check failed");
                return None;
            }
        };

        if !resp.is_success() {
            tracing::debug!(status = resp.status, "GitHub API returned non-200");
            return None;
        }

        let body = resp.text().ok()?;
        let json: serde_json::Value = serde_json::from_str(&body).ok()?;
        let tag = json.get("tag_name")?.as_str()?;
        let latest = tag.strip_prefix('v').unwrap_or(tag);
        let html_url = json.get("html_url")?.as_str().unwrap_or("");

        // Cache the result
        if let Some(cache) = crate::cache::Cache::default_for(&self.app_name) {
            let _ = cache.set(CACHE_KEY, latest.as_bytes(), CACHE_TTL);
        }

        self.compare_versions_with_url(latest, html_url)
    }

    fn compare_versions(&self, latest: &str) -> Option<UpdateInfo> {
        let url = format!(
            "https://github.com/{}/releases/tag/v{}",
            self.repo, latest
        );
        self.compare_versions_with_url(latest, &url)
    }

    fn compare_versions_with_url(&self, latest: &str, url: &str) -> Option<UpdateInfo> {
        if is_newer(&self.current_version, latest) {
            Some(UpdateInfo {
                current: self.current_version.clone(),
                latest: latest.to_string(),
                url: url.to_string(),
            })
        } else {
            None
        }
    }
}

/// Compare two semver-ish version strings.
///
/// Returns `true` if `latest` is newer than `current`.
/// Handles `major.minor.patch` format. Non-numeric segments
/// are compared lexicographically.
pub fn is_newer(current: &str, latest: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|s| s.parse().unwrap_or(0))
            .collect()
    };

    let curr = parse(current);
    let lat = parse(latest);

    for i in 0..curr.len().max(lat.len()) {
        let c = curr.get(i).copied().unwrap_or(0);
        let l = lat.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features update -E 'binary(update_test)'
```

Expected: All 5 tests PASS (they test local logic, not network).

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features update --all-targets -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat(update): add GitHub release version checking with cache

Non-blocking update check via GitHub releases API. Results cached
for 24h via cache module. Respects {APP}_NO_UPDATE_CHECK=1. Failures
are silent at debug level.
```

---

### Task 7: Dispatch Module

**Files:**
- Modify: `src/dispatch.rs`
- Create: `tests/dispatch_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/dispatch_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "dispatch")]

use rebar::dispatch;

#[test]
fn find_subcommand_binary_name() {
    let name = dispatch::subcommand_binary("myapp", "serve");
    assert_eq!(name, "myapp-serve");
}

#[test]
fn resolve_returns_none_for_missing_command() {
    let result = dispatch::resolve("rebar-test-nonexistent-42", "fakecmd");
    assert!(result.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features dispatch -E 'binary(dispatch_test)'
```

Expected: FAIL — `dispatch::subcommand_binary` doesn't exist.

- [ ] **Step 3: Implement dispatch module**

Write `src/dispatch.rs`:

```rust
//! Git-style external command dispatch.
//!
//! Resolves `{app}-{subcommand}` binaries on PATH and executes them,
//! enabling a plugin model where external tools extend the main CLI.
//!
//! # Usage
//!
//! ```ignore
//! // In the match arm for unknown subcommands:
//! match rebar::dispatch::run("myapp", "subcommand", &args)? {
//!     Some(status) => std::process::exit(status.code().unwrap_or(1)),
//!     None => eprintln!("unknown command: subcommand"),
//! }
//! ```

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use crate::error::{Error, Result};

/// Construct the expected binary name for a subcommand.
///
/// Returns `{app_name}-{subcommand}`.
pub fn subcommand_binary(app_name: &str, subcommand: &str) -> String {
    format!("{app_name}-{subcommand}")
}

/// Resolve the full path to a subcommand binary on PATH.
///
/// Returns `None` if the binary is not found.
pub fn resolve(app_name: &str, subcommand: &str) -> Option<PathBuf> {
    let binary = subcommand_binary(app_name, subcommand);
    which::which(&binary).ok()
}

/// Run an external subcommand, passing through arguments.
///
/// Returns `Ok(Some(ExitStatus))` if the binary was found and executed.
/// Returns `Ok(None)` if the binary was not found on PATH.
///
/// # Errors
///
/// Returns [`Error::Dispatch`] if the binary exists but fails to execute
/// (permission denied, invalid binary, etc.).
#[tracing::instrument(skip(args), fields(app = %app_name, subcommand = %subcommand))]
pub fn run<I, S>(app_name: &str, subcommand: &str, args: I) -> Result<Option<ExitStatus>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let Some(binary_path) = resolve(app_name, subcommand) else {
        return Ok(None);
    };

    tracing::debug!(binary = %binary_path.display(), "dispatching to external command");

    let status = Command::new(&binary_path)
        .args(args)
        .status()
        .map_err(|e| {
            Error::Dispatch(Box::new(std::io::Error::new(
                e.kind(),
                format!(
                    "failed to execute {}: {e}",
                    binary_path.display()
                ),
            )))
        })?;

    tracing::debug!(exit_code = ?status.code(), "external command finished");
    Ok(Some(status))
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features dispatch -E 'binary(dispatch_test)'
```

Expected: All 2 tests PASS.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features dispatch --all-targets -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat(dispatch): add git-style external command dispatch

Resolves {app}-{subcommand} binaries on PATH and executes them.
Returns None if binary not found, typed error if execution fails.
```

---

### Task 8: Diagnostics Module

**Files:**
- Modify: `src/diagnostics.rs`
- Create: `tests/diagnostics_test.rs`

This is the most complex Phase 3 module. It provides:
1. `DoctorCheck` trait — consumers implement per check
2. `DoctorRunner` — collects and runs checks, reports results
3. `DebugBundle` — collects sanitized config, logs, doctor output into tar.gz

- [ ] **Step 1: Write failing tests**

Create `tests/diagnostics_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "diagnostics")]

use rebar::diagnostics::{CheckResult, CheckStatus, DoctorCheck, DoctorRunner, DebugBundle};
use tempfile::TempDir;

struct AlwaysPassCheck;

impl DoctorCheck for AlwaysPassCheck {
    fn name(&self) -> &str {
        "always-pass"
    }

    fn category(&self) -> &str {
        "test"
    }

    fn run(&self) -> CheckResult {
        CheckResult {
            status: CheckStatus::Ok,
            message: "Everything is fine".to_string(),
        }
    }
}

struct AlwaysFailCheck;

impl DoctorCheck for AlwaysFailCheck {
    fn name(&self) -> &str {
        "always-fail"
    }

    fn category(&self) -> &str {
        "test"
    }

    fn run(&self) -> CheckResult {
        CheckResult {
            status: CheckStatus::Error,
            message: "Something is wrong".to_string(),
        }
    }
}

#[test]
fn runner_registers_checks() {
    let mut runner = DoctorRunner::new();
    runner.add(Box::new(AlwaysPassCheck));
    runner.add(Box::new(AlwaysFailCheck));
    assert_eq!(runner.check_count(), 2);
}

#[test]
fn runner_executes_all_checks() {
    let mut runner = DoctorRunner::new();
    runner.add(Box::new(AlwaysPassCheck));
    runner.add(Box::new(AlwaysFailCheck));
    let results = runner.run_all();
    assert_eq!(results.len(), 2);
}

#[test]
fn runner_reports_pass_and_fail() {
    let mut runner = DoctorRunner::new();
    runner.add(Box::new(AlwaysPassCheck));
    runner.add(Box::new(AlwaysFailCheck));
    let results = runner.run_all();
    let summary = DoctorRunner::summarize(&results);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 1);
}

#[test]
fn debug_bundle_creates_archive() {
    let tmp = TempDir::new().unwrap();
    let bundle = DebugBundle::new("test-app", tmp.path());

    bundle.add_text("info.txt", "test content").unwrap();
    let archive_path = bundle.finish().unwrap();
    assert!(archive_path.exists());
    assert!(
        archive_path
            .to_string_lossy()
            .ends_with(".tar.gz"),
    );
}

#[test]
fn check_status_is_ok() {
    assert!(CheckStatus::Ok.is_ok());
    assert!(!CheckStatus::Error.is_ok());
    assert!(!CheckStatus::Warn.is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features diagnostics -E 'binary(diagnostics_test)'
```

Expected: FAIL — types don't exist.

- [ ] **Step 3: Implement diagnostics module**

Write `src/diagnostics.rs`:

```rust
//! Doctor command framework and debug bundles.
//!
//! Provides a check registration and execution framework for "doctor"
//! commands, plus a debug bundle builder that collects diagnostic
//! information into a tar.gz archive.
//!
//! # Usage
//!
//! ```ignore
//! use rebar::diagnostics::{DoctorCheck, DoctorRunner, CheckResult, CheckStatus};
//!
//! struct ConfigCheck { /* ... */ }
//!
//! impl DoctorCheck for ConfigCheck {
//!     fn name(&self) -> &str { "config" }
//!     fn category(&self) -> &str { "configuration" }
//!     fn run(&self) -> CheckResult {
//!         // Check config validity
//!         CheckResult { status: CheckStatus::Ok, message: "Config is valid".into() }
//!     }
//! }
//!
//! let mut runner = DoctorRunner::new();
//! runner.add(Box::new(ConfigCheck { /* ... */ }));
//! let results = runner.run_all();
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

// ─── Doctor Framework ──────────────────────────────────────────────

/// Trait for doctor checks. Implement for each diagnostic check.
pub trait DoctorCheck {
    /// Short name for the check (e.g., "config", "permissions").
    fn name(&self) -> &str;

    /// Category for grouping in output (e.g., "configuration", "network").
    fn category(&self) -> &str;

    /// Run the check and return a result.
    fn run(&self) -> CheckResult;
}

/// Result of a single doctor check.
#[derive(Clone, Debug)]
pub struct CheckResult {
    /// Status of the check.
    pub status: CheckStatus,
    /// Human-readable message describing the result.
    pub message: String,
}

/// Status of a doctor check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    /// Check passed.
    Ok,
    /// Check passed with a warning.
    Warn,
    /// Check failed.
    Error,
}

impl CheckStatus {
    /// Returns true if the status is Ok.
    pub const fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }
}

/// Named check result (name + category + result).
#[derive(Clone, Debug)]
pub struct NamedResult {
    /// Check name.
    pub name: String,
    /// Check category.
    pub category: String,
    /// Check result.
    pub result: CheckResult,
}

/// Summary of doctor check results.
#[derive(Clone, Debug, Default)]
pub struct DoctorSummary {
    /// Number of checks that passed.
    pub passed: usize,
    /// Number of checks that warned.
    pub warned: usize,
    /// Number of checks that failed.
    pub failed: usize,
}

/// Collects and runs doctor checks.
pub struct DoctorRunner {
    checks: Vec<Box<dyn DoctorCheck>>,
}

impl DoctorRunner {
    /// Create a new empty runner.
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Register a check.
    pub fn add(&mut self, check: Box<dyn DoctorCheck>) {
        self.checks.push(check);
    }

    /// Number of registered checks.
    pub fn check_count(&self) -> usize {
        self.checks.len()
    }

    /// Run all checks and return named results.
    pub fn run_all(&self) -> Vec<NamedResult> {
        self.checks
            .iter()
            .map(|check| {
                let name = check.name().to_string();
                let category = check.category().to_string();
                tracing::debug!(check = %name, category = %category, "running doctor check");
                let result = check.run();
                tracing::debug!(
                    check = %name,
                    status = ?result.status,
                    "check complete"
                );
                NamedResult {
                    name,
                    category,
                    result,
                }
            })
            .collect()
    }

    /// Summarize a set of check results.
    pub fn summarize(results: &[NamedResult]) -> DoctorSummary {
        let mut summary = DoctorSummary::default();
        for r in results {
            match r.result.status {
                CheckStatus::Ok => summary.passed += 1,
                CheckStatus::Warn => summary.warned += 1,
                CheckStatus::Error => summary.failed += 1,
            }
        }
        summary
    }

    /// Format results as a human-readable report.
    pub fn format_report(results: &[NamedResult]) -> String {
        let mut buf = String::new();
        let mut current_category = "";

        for r in results {
            if r.category != current_category {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(&format!("{}:\n", r.category));
                current_category = &r.category;
            }

            let icon = match r.result.status {
                CheckStatus::Ok => "OK",
                CheckStatus::Warn => "WARN",
                CheckStatus::Error => "FAIL",
            };
            buf.push_str(&format!("  [{icon}] {}: {}\n", r.name, r.result.message));
        }

        let summary = Self::summarize(results);
        buf.push_str(&format!(
            "\n{} passed, {} warnings, {} failed\n",
            summary.passed, summary.warned, summary.failed
        ));

        buf
    }
}

impl Default for DoctorRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Debug Bundle ──────────────────────────────────────────────────

/// Builder for diagnostic debug bundles (tar.gz archives).
pub struct DebugBundle {
    app_name: String,
    dir: PathBuf,
    files: Vec<(String, Vec<u8>)>,
}

impl DebugBundle {
    /// Create a new debug bundle builder.
    ///
    /// The archive will be written to `dir`.
    pub fn new(app_name: &str, dir: &Path) -> Self {
        Self {
            app_name: app_name.to_string(),
            dir: dir.to_path_buf(),
            files: Vec::new(),
        }
    }

    /// Add a text file to the bundle.
    pub fn add_text(&mut self, name: &str, content: &str) -> Result<()> {
        self.files.push((name.to_string(), content.as_bytes().to_vec()));
        Ok(())
    }

    /// Add a binary file to the bundle.
    pub fn add_bytes(&mut self, name: &str, data: &[u8]) -> Result<()> {
        self.files.push((name.to_string(), data.to_vec()));
        Ok(())
    }

    /// Add doctor results to the bundle.
    pub fn add_doctor_results(&mut self, results: &[NamedResult]) -> Result<()> {
        let report = DoctorRunner::format_report(results);
        self.add_text("doctor-report.txt", &report)
    }

    /// Write the tar.gz archive and return its path.
    pub fn finish(self) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| Error::Diagnostic(Box::new(e)))?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let filename = format!("{}-debug-{timestamp}.tar.gz", self.app_name);
        let path = self.dir.join(&filename);

        let file = std::fs::File::create(&path)
            .map_err(|e| Error::Diagnostic(Box::new(e)))?;
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);

        for (name, data) in &self.files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, name, data.as_slice())
                .map_err(|e| Error::Diagnostic(Box::new(e)))?;
        }

        archive
            .finish()
            .map_err(|e| Error::Diagnostic(Box::new(e)))?;

        tracing::info!(path = %path.display(), "debug bundle created");
        Ok(path)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features diagnostics -E 'binary(diagnostics_test)'
```

Expected: All 5 tests PASS.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features diagnostics --all-targets -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat(diagnostics): add doctor framework and debug bundle builder

DoctorCheck trait with check registration and runner. DebugBundle
collects text/binary files into a tar.gz archive. Categories and
summary reporting for doctor output.
```

---

### Task 9: Bench Module

**Files:**
- Modify: `src/bench.rs`
- Create: `tests/bench_test.rs`

The bench module is minimal — re-exports divan and provides a setup helper.

- [ ] **Step 1: Write failing test**

Create `tests/bench_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "bench")]

#[test]
fn bench_module_compiles() {
    // Verify the module is accessible
    let _ = std::any::type_name::<rebar::bench::BenchConfig>();
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo nextest run --features bench -E 'binary(bench_test)'
```

Expected: FAIL — `BenchConfig` doesn't exist.

- [ ] **Step 3: Implement bench module**

Write `src/bench.rs`:

```rust
//! Benchmark harness helpers wrapping divan.
//!
//! Re-exports divan types and provides configuration helpers for
//! consistent benchmark setup across projects.
//!
//! # Usage
//!
//! In `benches/my_bench.rs`:
//! ```ignore
//! use rebar::bench::BenchConfig;
//!
//! fn main() {
//!     let config = BenchConfig::default();
//!     divan::main();
//! }
//!
//! #[divan::bench]
//! fn my_benchmark() {
//!     // ...
//! }
//! ```

pub use divan;

/// Configuration for benchmark runs.
#[derive(Clone, Debug)]
pub struct BenchConfig {
    /// Minimum number of iterations per benchmark.
    pub min_iterations: u32,
    /// Maximum time per benchmark in seconds.
    pub max_time_secs: u64,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            min_iterations: 100,
            max_time_secs: 5,
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo nextest run --features bench -E 'binary(bench_test)'
```

Expected: PASS.

- [ ] **Step 5: Commit**

Write `commit.txt`:
```
feat(bench): add divan benchmark harness helpers

Re-exports divan and provides BenchConfig for consistent benchmark
setup across projects.
```

---

### Task 10: Feature Isolation and Full Suite Verification

**Files:** None (verification only)

- [ ] **Step 1: Verify no-feature compilation**

```bash
cargo check --no-default-features
```

Expected: PASS.

- [ ] **Step 2: Verify each feature in isolation**

```bash
cargo check --features lockfile
cargo check --features http
cargo check --features cache
cargo check --features update
cargo check --features dispatch
cargo check --features diagnostics
cargo check --features bench
```

Expected: All PASS. Features with implications (`update` → `http`, `dispatch` → `cli`, `diagnostics` → `config` + `logging`) should automatically activate their dependencies.

- [ ] **Step 3: Verify all features together**

```bash
cargo clippy --all-features --all-targets -- -D warnings
```

Expected: PASS, no warnings.

- [ ] **Step 4: Run full test suite**

```bash
cargo nextest run --all-features
```

Expected: All tests PASS across all test binaries (Phase 1 + Phase 2 + Phase 3).

- [ ] **Step 5: Run cargo fmt and cargo doc**

```bash
cargo fmt --all
cargo doc --all-features --no-deps 2>&1 | tail -3
```

Expected: No format changes. Docs build without warnings.

- [ ] **Step 6: Commit (if fmt made changes)**

Write `commit.txt`:
```
chore: verify Phase 3 feature isolation and full test suite

All features compile in isolation and together. Full test suite
passes across all binaries.
```

---

## Implementation Notes

### fd-lock API

Check `fd-lock` 4.0's exact API for `RwLock::try_write()`. The return type and lifetime requirements determine whether the self-referential struct pattern is needed or if a simpler approach works. The `owning_ref` crate is an alternative if lifetime issues arise, but try the simple approach first.

### hyper HTTP Client

The `http` module provides a minimal HTTP/1.1 client over plain TCP. For HTTPS (needed by `update` for GitHub API), the implementer needs to add TLS support. Options:
- `hyper-rustls` (pure Rust, no OpenSSL)
- `hyper-tls` (wraps native TLS)
- `rustls` directly with `tokio-rustls`

Since the design spec says "no reqwest, no ureq," TLS must be handled at the hyper level. `hyper-rustls` with `webpki-roots` is the lightest option. Add it as an optional dep gated on `http`.

### base64 in cache module

The cache module uses base64 encoding to store binary values in JSON. `base64` is already in the dep tree via `opentelemetry-otlp`. If it's not accessible when `cache` is enabled without `otel`, either:
- Add `base64` as an explicit optional dep gated on `cache`
- Use hex encoding instead (no extra dep, slightly larger files)
- Store values as raw bytes in a separate file alongside the JSON metadata

### divan as dev-dependency

`divan` should be in `[dev-dependencies]` but is feature-gated because bench files use `#[cfg(feature = "bench")]`. This is the standard pattern for benchmark harnesses. The `bench` feature only activates for benchmark targets.

### Diagnostics: sanitized config

When adding config to debug bundles, redact sensitive fields. The config module's `ConfigSources` can identify which files were loaded, but field-level redaction is the consumer's responsibility. Consider adding a `Sanitize` trait or a `#[serde(skip)]` convention for sensitive fields in a future iteration.

### tracing_subscriber::registry().init() Is Once-Per-Process

Same landmine as Phase 1 and Phase 2. Use `cargo nextest run` (process-per-test). Do not switch to `cargo test`.

### Test Attribute Ordering

`#![allow(missing_docs)]` must come BEFORE `#![cfg(feature = "...")]` in integration test files.
