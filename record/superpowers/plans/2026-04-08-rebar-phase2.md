# Rebar Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `shutdown`, `crash`, `otel`, and `mcp` features to reach full preset parity with the claylo-rs template.

**Architecture:** Each feature is a standalone module gated by its own Cargo feature flag. The builder composes them in the correct init order. Logging is refactored to expose a composable layer (instead of calling `registry().init()` internally), enabling logging + OTEL layers on one tracing-subscriber registry. Shutdown uses `tokio::sync::watch` for multi-consumer signaling. Crash installs a panic hook writing structured dumps. OTEL extracts the TracerProvider setup from the claylo-rs observability.rs template, switching from reqwest to hyper-client per the design spec.

**Tech Stack:** opentelemetry 0.31, opentelemetry_sdk 0.31, opentelemetry-otlp 0.31 (http-proto + hyper-client), tracing-opentelemetry 0.32, tokio 1.51, rmcp 1.3

**Spec:** `record/superpowers/specs/2026-04-06-rebar-design.md`

**Source material:** OTEL/logging composition extracted from `~/source/claylo/claylo-rs/claylo-rs/template/` observability.rs.jinja. Shutdown pattern from `~/source/reference/cli-batteries/src/shutdown.rs` (adapted to non-global design). Crash and MCP are new.

---

## File Structure

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | New feature flags and optional deps for shutdown, crash, otel, otel-grpc, mcp |
| `src/error.rs` | New error variants: `OtelInit`, `ShutdownInit` |
| `src/shutdown.rs` | `ShutdownHandle`, `ShutdownToken`, signal handler registration |
| `src/crash.rs` | Panic hook, `CrashInfo`, structured crash dump writer |
| `src/otel.rs` | `OtelConfig`, `OtelGuard`, TracerProvider setup, composable tracing layer |
| `src/mcp.rs` | rmcp re-exports, `serve_stdio()` helper |
| `src/logging.rs` | Refactor: add `pub(crate) build_json_layer()`, keep `init()` as escape hatch |
| `src/lib.rs` | Builder gains `.shutdown()`, `.crash_handler()`, `.otel()`; App gains `shutdown_token()`, new guard fields |
| `tests/shutdown_test.rs` | ShutdownHandle, token cancellation, state transitions |
| `tests/crash_test.rs` | Panic hook install, crash dump format, hook chaining |
| `tests/otel_test.rs` | OtelConfig, resource attributes, layer building |
| `tests/mcp_test.rs` | rmcp re-exports, serve helper compilation |

---

## Key Design Decisions

**Drop order in App:** OTEL guard must drop before logging guard so OTEL can flush spans while logging is still active. Rust drops struct fields in declaration order, so `_otel_guard` must be declared before `_logging_guard`.

**Layer composition:** When both `logging` and `otel` features are active, both layers are composed onto one `tracing_subscriber::Registry` using `Option<Layer>` (which implements `Layer<S>` — `None` is a no-op). Feature-gated `#[cfg]` blocks handle the different type signatures.

**Shutdown is not a global singleton:** Unlike cli-batteries which uses `Lazy<(Sender, Receiver)>`, rebar's `ShutdownHandle` lives in `App`. This avoids global state and allows multiple rebar instances in tests.

**OTEL without endpoint:** When no OTLP endpoint is configured (no env var, no config), the OTEL layer is `None`. The TracerProvider is not created. This means `.otel()` on the builder is always safe — it only exports if an endpoint is available.

**Crash hook chains:** `std::panic::take_hook()` saves the previous hook. Our crash handler runs first (writes dump), then calls the previous hook. This preserves default panic behavior (stderr output, abort).

---

### Task 1: Add Phase 2 Dependencies and Module Stubs

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Modify: `src/error.rs`
- Create: `src/shutdown.rs`, `src/crash.rs`, `src/otel.rs`, `src/mcp.rs`

- [ ] **Step 1: Add dependencies to Cargo.toml**

Add to the `[dependencies]` section:

```toml
# Feature: shutdown
tokio = { version = "1.51", features = ["rt", "signal", "sync"], optional = true }

# Feature: otel
opentelemetry = { version = "0.31", features = ["trace"], optional = true }
opentelemetry_sdk = { version = "0.31", features = ["trace", "rt-tokio"], optional = true }
opentelemetry-otlp = { version = "0.31", features = ["http-proto", "hyper-client"], optional = true }
tracing-opentelemetry = { version = "0.32", optional = true }

# Feature: mcp
rmcp = { version = "1.3", features = ["server", "transport-io"], optional = true }
```

Add to `[dev-dependencies]`:

```toml
tokio = { version = "1.51", features = ["rt", "macros", "time"] }
```

Add to `[features]`:

```toml
shutdown = ["dep:tokio"]
crash = []
otel = ["logging", "dep:opentelemetry", "dep:opentelemetry_sdk", "dep:opentelemetry-otlp", "dep:tracing-opentelemetry", "dep:tokio"]
otel-grpc = ["otel", "opentelemetry-otlp/grpc-tonic"]
mcp = ["dep:rmcp", "dep:tokio"]
```

Note: `otel` implies `logging` (the design spec's feature dependency graph). The `otel-grpc` feature activates the `grpc-tonic` feature on `opentelemetry-otlp`. The `crash` feature has no additional deps — it uses only `std` types. The `tokio` dep appears in both `shutdown` and `otel`/`mcp` — Cargo deduplicates.

Note: Verify that `http-proto` and `hyper-client` are the exact feature names in `opentelemetry-otlp` 0.31. Check the crate's Cargo.toml on crates.io. If the feature names differ, adjust accordingly.

- [ ] **Step 2: Add error variants to src/error.rs**

Add after the `LogDirNotWritable` variant:

```rust
    /// OpenTelemetry initialization failed.
    #[error("failed to initialize OpenTelemetry: {0}")]
    OtelInit(Box<dyn std::error::Error + Send + Sync>),

    /// Shutdown signal handler registration failed.
    #[error("failed to register shutdown handler: {0}")]
    ShutdownInit(Box<dyn std::error::Error + Send + Sync>),
```

- [ ] **Step 3: Add module declarations to src/lib.rs**

Add after the `logging` module declaration:

```rust
#[cfg(feature = "otel")]
pub mod otel;

#[cfg(feature = "shutdown")]
pub mod shutdown;

#[cfg(feature = "crash")]
pub mod crash;

#[cfg(feature = "mcp")]
pub mod mcp;
```

- [ ] **Step 4: Create module stubs**

Create `src/shutdown.rs`:
```rust
//! Graceful shutdown with signal handling.
```

Create `src/crash.rs`:
```rust
//! Panic hook with structured crash dumps.
```

Create `src/otel.rs`:
```rust
//! OpenTelemetry tracing with OTLP export.
```

Create `src/mcp.rs`:
```rust
//! MCP server helpers wrapping rmcp.
```

- [ ] **Step 5: Verify each feature compiles**

```bash
cargo check --no-default-features
cargo check --features shutdown
cargo check --features crash
cargo check --features otel
cargo check --features mcp
cargo check --features cli,config,logging,shutdown,crash,otel,mcp
```

Expected: All pass. If `opentelemetry-otlp` feature names are wrong, fix them before proceeding.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat: add Phase 2 feature flags and module stubs

Adds shutdown, crash, otel, otel-grpc, and mcp feature flags with
their dependencies. All modules are empty stubs that compile.
```

---

### Task 2: Shutdown Module

**Files:**
- Modify: `src/shutdown.rs`
- Modify: `src/lib.rs` (builder + App)
- Create: `tests/shutdown_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/shutdown_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "shutdown")]

use rebar::shutdown::{ShutdownHandle, ShutdownToken};

#[test]
fn handle_starts_not_shutting_down() {
    let handle = ShutdownHandle::new();
    assert!(!handle.is_shutting_down());
}

#[test]
fn shutdown_sets_flag() {
    let handle = ShutdownHandle::new();
    handle.shutdown();
    assert!(handle.is_shutting_down());
}

#[test]
fn token_is_cloneable() {
    let handle = ShutdownHandle::new();
    let token1 = handle.token();
    let token2 = token1.clone();
    handle.shutdown();
    assert!(token2.is_shutting_down());
}

#[tokio::test]
async fn token_cancelled_resolves_after_shutdown() {
    let handle = ShutdownHandle::new();
    let mut token = handle.token();

    // Spawn a task that shuts down after a short delay
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        handle_clone.shutdown();
    });

    // This should resolve when shutdown is called
    token.cancelled().await;
    assert!(handle.is_shutting_down());
}

#[tokio::test]
async fn token_cancelled_resolves_immediately_if_already_shutdown() {
    let handle = ShutdownHandle::new();
    handle.shutdown();

    let mut token = handle.token();
    // Should return immediately, not hang
    tokio::time::timeout(
        std::time::Duration::from_millis(100),
        token.cancelled(),
    )
    .await
    .expect("cancelled() should resolve immediately when already shut down");
}

#[test]
fn multiple_shutdown_calls_are_safe() {
    let handle = ShutdownHandle::new();
    handle.shutdown();
    handle.shutdown(); // should not panic
    assert!(handle.is_shutting_down());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features shutdown -E 'binary(shutdown_test)'
```

Expected: FAIL — `ShutdownHandle` and `ShutdownToken` don't exist yet.

- [ ] **Step 3: Implement shutdown module**

Write `src/shutdown.rs`:

```rust
//! Graceful shutdown with signal handling.
//!
//! Provides [`ShutdownHandle`] for triggering shutdown and [`ShutdownToken`]
//! for waiting on the shutdown signal. Uses `tokio::sync::watch` so multiple
//! consumers can await shutdown without ownership issues.
//!
//! # Usage
//!
//! ```ignore
//! let app = rebar::init("myapp").shutdown().start()?;
//! let mut token = app.shutdown_token();
//!
//! tokio::select! {
//!     _ = do_work() => {},
//!     _ = token.cancelled() => { /* cleanup */ },
//! }
//! ```

use tokio::sync::watch;

/// Handle for triggering and observing shutdown.
///
/// Stored in [`App`](crate::App). Clone is cheap (Arc internally via watch).
#[derive(Clone, Debug)]
pub struct ShutdownHandle {
    sender: watch::Sender<bool>,
    receiver: watch::Receiver<bool>,
}

impl ShutdownHandle {
    /// Create a new shutdown handle (not yet shutting down).
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender, receiver }
    }

    /// Trigger shutdown. All tokens will be notified.
    ///
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn shutdown(&self) {
        let _ = self.sender.send(true);
    }

    /// Check if shutdown has been triggered.
    pub fn is_shutting_down(&self) -> bool {
        *self.receiver.borrow()
    }

    /// Create a token for waiting on shutdown.
    pub fn token(&self) -> ShutdownToken {
        ShutdownToken {
            receiver: self.receiver.clone(),
        }
    }

    /// Register OS signal handlers (SIGTERM, SIGINT) that trigger shutdown.
    ///
    /// Spawns a tokio task that listens for signals. The task exits when
    /// a signal is received or when the handle is dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if signal handler registration fails.
    pub fn register_signals(&self) -> crate::Result<()> {
        let handle = self.clone();

        tokio::spawn(async move {
            let ctrl_c = tokio::signal::ctrl_c();
            #[cfg(unix)]
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("failed to register SIGTERM handler");

            #[cfg(unix)]
            tokio::select! {
                _ = ctrl_c => {},
                _ = sigterm.recv() => {},
            }

            #[cfg(not(unix))]
            ctrl_c.await.ok();

            tracing::info!("shutdown signal received");
            handle.shutdown();
        });

        Ok(())
    }
}

impl Default for ShutdownHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Token for waiting on shutdown. Cloneable and cheap.
///
/// Use in `tokio::select!` to cancel work on shutdown:
///
/// ```ignore
/// let mut token = app.shutdown_token();
/// tokio::select! {
///     result = do_work() => handle_result(result),
///     _ = token.cancelled() => tracing::info!("shutting down"),
/// }
/// ```
#[derive(Clone, Debug)]
pub struct ShutdownToken {
    receiver: watch::Receiver<bool>,
}

impl ShutdownToken {
    /// Wait until shutdown is triggered.
    ///
    /// Resolves immediately if shutdown has already been triggered.
    pub async fn cancelled(&mut self) {
        // If already shutting down, return immediately
        if *self.receiver.borrow_and_update() {
            return;
        }
        // Wait for the value to change
        self.receiver.changed().await.ok();
    }

    /// Check if shutdown has been triggered (non-async).
    pub fn is_shutting_down(&self) -> bool {
        *self.receiver.borrow()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features shutdown -E 'binary(shutdown_test)'
```

Expected: All 6 tests PASS.

- [ ] **Step 5: Wire shutdown into Builder and App**

In `src/lib.rs`, add to the `App` struct (before the `_logging_guard` field to get correct drop order):

```rust
    #[cfg(feature = "shutdown")]
    shutdown_handle: Option<shutdown::ShutdownHandle>,
```

Add an accessor to `App`:

```rust
#[cfg(feature = "shutdown")]
impl<C> App<C> {
    /// Get a shutdown token for waiting on graceful shutdown.
    ///
    /// Returns `None` if `.shutdown()` was not called on the builder.
    pub fn shutdown_token(&self) -> Option<shutdown::ShutdownToken> {
        self.shutdown_handle.as_ref().map(|h| h.token())
    }

    /// Trigger shutdown programmatically.
    pub fn shutdown(&self) {
        if let Some(ref handle) = self.shutdown_handle {
            handle.shutdown();
        }
    }
}
```

Add to `Builder`:

```rust
    #[cfg(feature = "shutdown")]
    enable_shutdown: bool,
```

Initialize it in `init()`:

```rust
    #[cfg(feature = "shutdown")]
    enable_shutdown: false,
```

Add method to `Builder`:

```rust
    /// Register signal handlers for graceful shutdown.
    #[cfg(feature = "shutdown")]
    pub const fn shutdown(mut self) -> Self {
        self.enable_shutdown = true;
        self
    }
```

Add the same field, init, and method to `ConfiguredBuilder<C>`.

In both `Builder::start()` and `ConfiguredBuilder::start()`, add after logging init but before `Ok(App { ... })`:

```rust
    #[cfg(feature = "shutdown")]
    let shutdown_handle = if self.enable_shutdown {
        let handle = shutdown::ShutdownHandle::new();
        handle.register_signals()?;
        Some(handle)
    } else {
        None
    };
```

Note: For `Builder`, use `self.enable_shutdown`. For `ConfiguredBuilder`, use the appropriate field (may need to capture before move like the logging pattern).

Add to the `App` construction in both `start()` methods:

```rust
    #[cfg(feature = "shutdown")]
    shutdown_handle,
```

- [ ] **Step 6: Run clippy**

```bash
cargo clippy --features cli,config,logging,shutdown --all-targets --message-format=short -- -D warnings
```

Expected: No warnings.

- [ ] **Step 7: Commit**

Write `commit.txt`:
```
feat(shutdown): add graceful shutdown with signal handling

ShutdownHandle uses tokio::sync::watch for multi-consumer signaling.
ShutdownToken for async cancellation in tokio::select!. Signal handler
registration for SIGTERM/SIGINT. Wired into builder and App.
```

---

### Task 3: Crash Module

**Files:**
- Modify: `src/crash.rs`
- Create: `tests/crash_test.rs`
- Modify: `src/lib.rs` (builder)

- [ ] **Step 1: Write failing tests**

Create `tests/crash_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "crash")]

use rebar::crash;
use std::fs;
use tempfile::TempDir;

#[test]
fn crash_info_format_contains_required_fields() {
    let info = crash::CrashInfo {
        message: "test panic".to_string(),
        location: Some("src/main.rs:42".to_string()),
        app_name: "test-app".to_string(),
        version: "0.1.0".to_string(),
        timestamp: "2026-04-08T12:00:00.000Z".to_string(),
        os: "macos".to_string(),
        backtrace: "   0: test::frame".to_string(),
    };

    let formatted = info.format();
    assert!(formatted.contains("test panic"));
    assert!(formatted.contains("test-app"));
    assert!(formatted.contains("0.1.0"));
    assert!(formatted.contains("src/main.rs:42"));
    assert!(formatted.contains("macos"));
}

#[test]
fn write_crash_dump_creates_file() {
    let tmp = TempDir::new().unwrap();
    let info = crash::CrashInfo {
        message: "test panic".to_string(),
        location: Some("src/main.rs:42".to_string()),
        app_name: "test-app".to_string(),
        version: "0.1.0".to_string(),
        timestamp: "2026-04-08T12:00:00.000Z".to_string(),
        os: std::env::consts::OS.to_string(),
        backtrace: String::new(),
    };

    let path = crash::write_crash_dump_to(&info, tmp.path());
    assert!(path.is_some(), "should write crash file");

    let path = path.unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("test panic"));
    assert!(content.contains("test-app"));
}

#[test]
fn crash_dir_contains_app_name() {
    let dir = crash::crash_dump_dir("test-app");
    let path = dir.to_string_lossy();
    assert!(
        path.contains("test-app"),
        "crash dir should contain app name: {path}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features crash -E 'binary(crash_test)'
```

Expected: FAIL — `crash::CrashInfo`, `write_crash_dump_to`, `crash_dump_dir` don't exist.

- [ ] **Step 3: Implement crash module**

Write `src/crash.rs`:

```rust
//! Panic hook with structured crash dumps.
//!
//! Installs a custom panic hook that captures panic information,
//! writes a structured crash dump to disk, and prints a user-friendly
//! message to stderr. The previous panic hook is chained so default
//! behavior (stderr output) is preserved.
//!
//! # Usage
//!
//! ```ignore
//! let app = rebar::init("myapp")
//!     .crash_handler()
//!     .start()?;
//! ```
//!
//! Or standalone:
//!
//! ```ignore
//! rebar::crash::install("myapp", env!("CARGO_PKG_VERSION"));
//! ```

use std::path::{Path, PathBuf};

/// Structured crash information written to dump files.
#[derive(Clone, Debug)]
pub struct CrashInfo {
    /// The panic message.
    pub message: String,
    /// Source location of the panic (file:line).
    pub location: Option<String>,
    /// Application name.
    pub app_name: String,
    /// Application version.
    pub version: String,
    /// Timestamp (RFC 3339 UTC).
    pub timestamp: String,
    /// Operating system.
    pub os: String,
    /// Backtrace (may be empty if not captured).
    pub backtrace: String,
}

impl CrashInfo {
    /// Format crash info as a human-readable report.
    pub fn format(&self) -> String {
        let mut buf = String::with_capacity(512);
        buf.push_str("=== CRASH REPORT ===\n\n");
        buf.push_str(&format!("Application: {} {}\n", self.app_name, self.version));
        buf.push_str(&format!("Timestamp:   {}\n", self.timestamp));
        buf.push_str(&format!("OS:          {}\n", self.os));
        if let Some(ref loc) = self.location {
            buf.push_str(&format!("Location:    {loc}\n"));
        }
        buf.push_str(&format!("\nPanic: {}\n", self.message));
        if !self.backtrace.is_empty() {
            buf.push_str(&format!("\nBacktrace:\n{}\n", self.backtrace));
        }
        buf
    }
}

/// Install the crash handler panic hook.
///
/// Chains with the previous hook — our handler runs first (writes dump),
/// then the previous hook runs (default stderr output).
pub fn install(app_name: &str, version: &str) {
    let app_name = app_name.to_string();
    let version = version.to_string();
    let prev = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };

        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}", loc.file(), loc.line()));

        let backtrace = std::backtrace::Backtrace::force_capture().to_string();

        let info = CrashInfo {
            message,
            location,
            app_name: app_name.clone(),
            version: version.clone(),
            timestamp: format_timestamp(),
            os: std::env::consts::OS.to_string(),
            backtrace,
        };

        // Write crash dump (best-effort, don't panic in the panic hook)
        let dump_dir = crash_dump_dir(&info.app_name);
        if let Some(path) = write_crash_dump_to(&info, &dump_dir) {
            eprintln!(
                "\n{app} crashed. Report saved to: {path}\n",
                app = info.app_name,
                path = path.display()
            );
        } else {
            eprintln!("\n{} crashed: {}\n", info.app_name, info.message);
        }

        // Chain to previous hook
        prev(panic_info);
    }));
}

/// Write a crash dump to the given directory. Returns the file path on success.
pub fn write_crash_dump_to(info: &CrashInfo, dir: &Path) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;

    // Use timestamp in filename for uniqueness
    let safe_ts = info.timestamp.replace(':', "-");
    let filename = format!("crash-{safe_ts}.txt");
    let path = dir.join(filename);

    std::fs::write(&path, info.format()).ok()?;
    Some(path)
}

/// Get the crash dump directory for an application.
///
/// - macOS: `~/Library/Caches/{app}/crashes/`
/// - Linux: `$XDG_CACHE_HOME/{app}/crashes/` (defaults to `~/.cache/{app}/crashes/`)
/// - Fallback: `$TMPDIR/{app}/crashes/`
pub fn crash_dump_dir(app_name: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library/Caches")
                .join(app_name)
                .join("crashes");
        }
    } else if cfg!(unix) {
        let cache_base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache"))
            });
        if let Some(base) = cache_base {
            return base.join(app_name).join("crashes");
        }
    }

    // Fallback
    std::env::temp_dir().join(app_name).join("crashes")
}

/// Format timestamp (same algorithm as logging module, duplicated to keep
/// crash standalone with no cross-module dependency).
fn format_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = now.as_secs();
    let nanos = now.subsec_nanos();
    let days_since_epoch = secs / 86400;
    let secs_of_day = secs % 86400;
    let hours = secs_of_day / 3600;
    let minutes = (secs_of_day % 3600) / 60;
    let seconds = secs_of_day % 60;

    let (year, month, day) = days_to_ymd(days_since_epoch as i64);

    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z",
        millis = nanos / 1_000_000
    )
}

const fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}
```

Note: `format_timestamp` and `days_to_ymd` are duplicated from `logging.rs` intentionally. The crash module must be standalone (no `logging` feature dependency). If this duplication bothers you, extract to a private `src/time.rs` utility and have both modules use it — but that's a refactor, not a Phase 2 requirement.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features crash -E 'binary(crash_test)'
```

Expected: All 3 tests PASS.

- [ ] **Step 5: Wire crash into Builder**

In `src/lib.rs`, add to `Builder`:

```rust
    #[cfg(feature = "crash")]
    enable_crash: bool,
```

Initialize in `init()`:

```rust
    #[cfg(feature = "crash")]
    enable_crash: false,
```

Add method:

```rust
    /// Install a crash handler (panic hook with structured dump).
    #[cfg(feature = "crash")]
    pub const fn crash_handler(mut self) -> Self {
        self.enable_crash = true;
        self
    }
```

Add the same to `ConfiguredBuilder<C>`.

In both `start()` methods, add as the LAST step before `Ok(App { ... })`:

```rust
    #[cfg(feature = "crash")]
    if self.enable_crash {
        crash::install(&self.app_name, env!("CARGO_PKG_VERSION"));
    }
```

Note: For `ConfiguredBuilder`, the app_name may already be moved. Capture it before the move, or use a reference. Follow the existing pattern for how `app_name` is handled.

Note: `env!("CARGO_PKG_VERSION")` captures the *rebar* crate version, not the consumer's. The implementer should decide whether to pass the version as a builder parameter or use rebar's version. The template uses `env!("CARGO_PKG_VERSION")` from the consumer's crate, so the builder should probably accept a version string. For now, using rebar's version is acceptable — the version can be made configurable in a follow-up.

- [ ] **Step 6: Run clippy**

```bash
cargo clippy --features cli,config,logging,shutdown,crash --all-targets --message-format=short -- -D warnings
```

Expected: No warnings.

- [ ] **Step 7: Commit**

Write `commit.txt`:
```
feat(crash): add panic hook with structured crash dumps

Installs a custom panic hook that captures panic message, backtrace,
location, and OS info. Writes structured crash dump to XDG cache
directory. Chains with previous hook to preserve default behavior.
```

---

### Task 4: Refactor Logging for Composable Layers

**Files:**
- Modify: `src/logging.rs`
- Modify: `src/lib.rs` (builder start methods)

This task does NOT add OTEL — it only refactors logging so the builder can compose multiple layers on one registry. The key change: split `init()` into `build_json_layer()` (returns the layer + guard) and `init()` (escape hatch that composes and inits).

- [ ] **Step 1: Add build_json_layer to logging.rs**

Add this function to `src/logging.rs` (after the existing `init()` function):

```rust
/// Build the JSON log layer and its writer guard without initializing
/// the global subscriber.
///
/// Used internally by the builder to compose multiple layers (logging + OTEL)
/// on one registry. For standalone use, prefer [`init()`].
pub(crate) fn build_json_layer(
    cfg: &LoggingConfig,
) -> Result<(
    JsonLogLayer<tracing_appender::non_blocking::NonBlocking>,
    tracing_appender::non_blocking::WorkerGuard,
)> {
    let (log_writer, log_guard) = match build_log_writer(
        &cfg.service,
        &cfg.env_log_path,
        &cfg.env_log_dir,
        cfg.log_dir.as_deref(),
    ) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("Warning: {err}. Falling back to stderr logging.");
            tracing_appender::non_blocking(std::io::stderr())
        }
    };

    let log_layer = JsonLogLayer::new(log_writer);
    Ok((log_layer, log_guard))
}
```

Make `JsonLogLayer` visible to the builder:

```rust
// Change from:
struct JsonLogLayer<W> {
// To:
pub(crate) struct JsonLogLayer<W> {
```

- [ ] **Step 2: Refactor init() to use build_json_layer**

Replace the existing `init()` body:

```rust
pub fn init(cfg: &LoggingConfig, env_filter: EnvFilter) -> Result<LoggingGuard> {
    let (log_layer, log_guard) = build_json_layer(cfg)?;

    tracing_subscriber::registry()
        .with(env_filter)
        .with(log_layer)
        .init();

    tracing::debug!("logging initialized");

    Ok(LoggingGuard {
        _log_guard: log_guard,
    })
}
```

- [ ] **Step 3: Update builder start() to use build_json_layer**

In `src/lib.rs`, update `Builder::start()`. Replace the logging block:

```rust
// Replace:
#[cfg(feature = "logging")]
let logging_guard = if self.enable_logging {
    let (quiet, verbose) = self.cli_flags();
    let log_cfg = logging::LoggingConfig::from_app_name(&self.app_name);
    let filter = logging::env_filter(quiet, verbose, "info");
    Some(logging::init(&log_cfg, filter)?)
} else {
    None
};

// With:
#[cfg(feature = "logging")]
let logging_guard = if self.enable_logging {
    let (quiet, verbose) = self.cli_flags();
    let log_cfg = logging::LoggingConfig::from_app_name(&self.app_name);
    let filter = logging::env_filter(quiet, verbose, "info");
    let (log_layer, log_guard) = logging::build_json_layer(&log_cfg)?;

    tracing_subscriber::registry()
        .with(filter)
        .with(log_layer)
        .init();

    Some(logging::LoggingGuard::from_guard(log_guard))
} else {
    None
};
```

Do the same for `ConfiguredBuilder::start()`.

Also add a `pub(crate)` constructor to `LoggingGuard`:

```rust
impl LoggingGuard {
    /// Create a guard from a raw worker guard. Used by the builder.
    pub(crate) fn from_guard(guard: tracing_appender::non_blocking::WorkerGuard) -> Self {
        Self { _log_guard: guard }
    }
}
```

- [ ] **Step 4: Run ALL existing tests to verify no regression**

```bash
cargo nextest run --features cli,config,logging
```

Expected: All 42 existing tests PASS. The refactor is internal — no public API changed.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features cli,config,logging --all-targets --message-format=short -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
refactor(logging): split build_json_layer from init for composable layers

Extracts layer construction from subscriber initialization so the
builder can compose logging + OTEL layers on one registry. The
standalone init() escape hatch is unchanged.
```

---

### Task 5: OTEL Module

**Files:**
- Modify: `src/otel.rs`
- Modify: `src/lib.rs` (builder + App)
- Create: `tests/otel_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/otel_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "otel")]

use rebar::otel::OtelConfig;

#[test]
fn otel_config_from_app_name() {
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0");
    assert_eq!(cfg.service, "test-app");
    assert_eq!(cfg.version, "0.1.0");
    assert_eq!(cfg.env, "dev"); // default
    assert!(cfg.endpoint.is_none()); // no env var set
}

#[test]
fn otel_config_env_var_names() {
    let cfg = OtelConfig::from_app_name("my-tool", "1.0.0");
    assert_eq!(cfg.env_var_endpoint, "OTEL_EXPORTER_OTLP_ENDPOINT");
    assert_eq!(cfg.env_var_protocol, "OTEL_EXPORTER_OTLP_PROTOCOL");
    assert_eq!(cfg.env_var_env, "MY_TOOL_ENV");
}

#[test]
fn otel_config_with_endpoint() {
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0")
        .with_endpoint(Some("http://localhost:4318".to_string()));
    assert_eq!(cfg.endpoint.as_deref(), Some("http://localhost:4318"));
}

#[test]
fn otel_guard_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<rebar::otel::OtelGuard>();
}

#[test]
fn build_layer_returns_none_without_endpoint() {
    // No OTLP endpoint configured — layer should be None
    let cfg = OtelConfig::from_app_name("test-app", "0.1.0");
    let result = rebar::otel::build_otel_layer(&cfg);
    assert!(result.is_ok());
    let (layer, guard) = result.unwrap();
    assert!(layer.is_none(), "no endpoint means no layer");
    assert!(guard.is_none(), "no endpoint means no guard");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features otel -E 'binary(otel_test)'
```

Expected: FAIL — `OtelConfig`, `OtelGuard`, `build_otel_layer` don't exist.

- [ ] **Step 3: Implement OTEL module**

Write `src/otel.rs`:

```rust
//! OpenTelemetry tracing with OTLP export.
//!
//! Provides [`OtelConfig`] for OTLP endpoint configuration and
//! [`build_otel_layer()`] for creating a composable tracing layer.
//! The builder uses this to compose OTEL alongside JSONL logging
//! on one tracing-subscriber registry.
//!
//! # Endpoint resolution
//!
//! Priority: explicit config > `OTEL_EXPORTER_OTLP_ENDPOINT` env var.
//! If neither is set, OTEL is disabled (no export, no overhead).
//!
//! # Protocol selection
//!
//! `OTEL_EXPORTER_OTLP_PROTOCOL` env var selects the transport:
//! - `http/protobuf` (default) — HTTP with protobuf encoding
//! - `http/json` — HTTP with JSON encoding
//! - `grpc` — gRPC via tonic (requires `otel-grpc` feature)
//!
//! # Escape hatch
//!
//! ```ignore
//! let (layer, guard) = rebar::otel::build_otel_layer(&config)?;
//! ```

use opentelemetry::KeyValue;
use opentelemetry_sdk::trace as sdktrace;
use opentelemetry_sdk::Resource;

use crate::error::{Error, Result};

const ENV_OTEL_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const ENV_OTEL_PROTOCOL: &str = "OTEL_EXPORTER_OTLP_PROTOCOL";

/// Configuration for OTEL tracing export.
#[derive(Clone, Debug)]
pub struct OtelConfig {
    /// Service name for resource attributes.
    pub service: String,
    /// Application version.
    pub version: String,
    /// Deployment environment (defaults to "dev").
    pub env: String,
    /// OTLP endpoint (from config or env var).
    pub endpoint: Option<String>,
    /// Env var name for endpoint override.
    pub env_var_endpoint: String,
    /// Env var name for protocol selection.
    pub env_var_protocol: String,
    /// Env var name for environment.
    pub env_var_env: String,
}

impl OtelConfig {
    /// Create OTEL config from an application name and version.
    ///
    /// Reads the OTLP endpoint from the `OTEL_EXPORTER_OTLP_ENDPOINT` env var.
    /// The deployment environment is read from `{APP}_ENV` (defaults to "dev").
    pub fn from_app_name(app_name: &str, version: &str) -> Self {
        let prefix = app_name.to_uppercase().replace('-', "_");
        let env_var_env = format!("{prefix}_ENV");

        let endpoint = std::env::var(ENV_OTEL_ENDPOINT)
            .ok()
            .filter(|v| !v.trim().is_empty());

        let env = std::env::var(&env_var_env).unwrap_or_else(|_| "dev".to_string());

        Self {
            service: app_name.to_string(),
            version: version.to_string(),
            env,
            endpoint,
            env_var_endpoint: ENV_OTEL_ENDPOINT.to_string(),
            env_var_protocol: ENV_OTEL_PROTOCOL.to_string(),
            env_var_env,
        }
    }

    /// Override the OTLP endpoint (from config).
    pub fn with_endpoint(mut self, endpoint: Option<String>) -> Self {
        if self.endpoint.is_none() {
            self.endpoint = endpoint;
        }
        self
    }
}

/// Guard that holds the TracerProvider and flushes on drop.
pub struct OtelGuard {
    provider: sdktrace::SdkTracerProvider,
}

// Safety: SdkTracerProvider is Send + Sync
unsafe impl Send for OtelGuard {}
unsafe impl Sync for OtelGuard {}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(e) = self.provider.shutdown() {
            eprintln!("Error shutting down tracer provider: {e}");
        }
    }
}

/// Build the OTEL tracing layer and guard.
///
/// Returns `(None, None)` if no OTLP endpoint is configured.
/// Returns `(Some(layer), Some(guard))` when an endpoint is available.
///
/// The returned layer implements `tracing_subscriber::Layer<S>` and can
/// be composed with other layers via `registry.with(otel_layer)`.
///
/// # Errors
///
/// Returns an error if the OTLP exporter or TracerProvider fails to initialize.
pub fn build_otel_layer<S>(
    cfg: &OtelConfig,
) -> Result<(
    Option<tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>>,
    Option<OtelGuard>,
)>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let Some(ref endpoint) = cfg.endpoint else {
        return Ok((None, None));
    };

    if endpoint.trim().is_empty() {
        return Ok((None, None));
    }

    let resource = Resource::builder()
        .with_attributes([
            KeyValue::new("service.name", cfg.service.clone()),
            KeyValue::new("deployment.environment", cfg.env.clone()),
            KeyValue::new("service.version", cfg.version.clone()),
        ])
        .build();

    let protocol = std::env::var(ENV_OTEL_PROTOCOL)
        .unwrap_or_default()
        .to_lowercase();

    let exporter = match protocol.as_str() {
        "http/json" | "http/protobuf" => {
            opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_endpoint(endpoint.clone())
                .build()
                .map_err(|e| Error::OtelInit(Box::new(e)))?
        }
        #[cfg(feature = "otel-grpc")]
        "grpc" => {
            opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.clone())
                .build()
                .map_err(|e| Error::OtelInit(Box::new(e)))?
        }
        // Default to HTTP protobuf
        _ => {
            opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_endpoint(endpoint.clone())
                .build()
                .map_err(|e| Error::OtelInit(Box::new(e)))?
        }
    };

    let provider = sdktrace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer(cfg.service.clone());
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let guard = OtelGuard { provider };

    tracing::debug!(endpoint = %endpoint, "OTEL tracing initialized");

    Ok((Some(layer), Some(guard)))
}
```

Note: The `build_otel_layer` function is generic over `S` (the subscriber type). This allows it to work with any composed registry type. The `with_http()` and `with_tonic()` methods on `SpanExporter::builder()` are from `opentelemetry-otlp`. The exact API may differ slightly — check the 0.31 docs. The template at `~/source/claylo/claylo-rs/` uses the same pattern.

Note: The `unsafe impl Send/Sync for OtelGuard` may not be needed if `SdkTracerProvider` already implements these traits. Check and remove if unnecessary.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features otel -E 'binary(otel_test)'
```

Expected: All 5 tests PASS.

- [ ] **Step 5: Wire OTEL into Builder and App**

In `src/lib.rs`, add to the `App` struct. **Important:** `_otel_guard` must be declared BEFORE `_logging_guard` so it drops first (OTEL flushes spans while logging is still active):

```rust
    #[cfg(feature = "otel")]
    _otel_guard: Option<otel::OtelGuard>,
```

Add to `Builder`:

```rust
    #[cfg(feature = "otel")]
    enable_otel: bool,
```

Initialize in `init()`:

```rust
    #[cfg(feature = "otel")]
    enable_otel: false,
```

Add method:

```rust
    /// Enable OTLP tracing export.
    ///
    /// Export only happens if an OTLP endpoint is configured via
    /// `OTEL_EXPORTER_OTLP_ENDPOINT` env var or config. Calling `.otel()`
    /// without an endpoint is safe — it's a no-op.
    #[cfg(feature = "otel")]
    pub const fn otel(mut self) -> Self {
        self.enable_otel = true;
        self
    }
```

Add the same field, init, and method to `ConfiguredBuilder<C>`.

- [ ] **Step 6: Update builder start() for composable layer init**

This is the critical integration step. Both `Builder::start()` and `ConfiguredBuilder::start()` need to compose logging + OTEL layers on one registry.

Replace the logging initialization block in `Builder::start()`:

```rust
    // ─── Compose tracing layers ────────────────────────────────
    #[cfg(feature = "logging")]
    let (log_layer, log_guard) = if self.enable_logging {
        let (quiet, verbose) = self.cli_flags();
        let log_cfg = logging::LoggingConfig::from_app_name(&self.app_name);
        let filter = logging::env_filter(quiet, verbose, "info");
        let (layer, guard) = logging::build_json_layer(&log_cfg)?;
        (Some((layer, filter)), Some(logging::LoggingGuard::from_guard(guard)))
    } else {
        (None, None)
    };

    #[cfg(feature = "otel")]
    let (otel_layer, otel_guard) = if self.enable_otel {
        let otel_cfg = otel::OtelConfig::from_app_name(&self.app_name, env!("CARGO_PKG_VERSION"));
        otel::build_otel_layer(&otel_cfg)?
    } else {
        (None, None)
    };

    // Init the tracing subscriber with all active layers
    #[cfg(all(feature = "logging", not(feature = "otel")))]
    if let Some((layer, filter)) = log_layer {
        tracing_subscriber::registry()
            .with(filter)
            .with(layer)
            .init();
    }

    #[cfg(all(feature = "logging", feature = "otel"))]
    {
        let (layer, filter) = log_layer.unzip();
        tracing_subscriber::registry()
            .with(filter)
            .with(layer)
            .with(otel_layer)
            .init();
    }
```

Note: `otel` implies `logging` in the feature graph, so the `#[cfg(all(feature = "otel", not(feature = "logging")))]` case can't happen. The two cfg blocks cover all cases.

Note: The `filter` and `layer` being `Option<_>` works because `Option<L: Layer<S>>` implements `Layer<S>`.

Add to the App construction:

```rust
    #[cfg(feature = "otel")]
    _otel_guard: otel_guard,
```

Do the same for `ConfiguredBuilder::start()`. The OTEL config may also read the endpoint from the user's config type. For now, use env var only. Config integration can be added when a consumer needs it.

- [ ] **Step 7: Run ALL tests**

```bash
cargo nextest run --features cli,config,logging,shutdown,crash,otel
```

Expected: All tests PASS across all test binaries.

- [ ] **Step 8: Run clippy**

```bash
cargo clippy --features cli,config,logging,shutdown,crash,otel --all-targets --message-format=short -- -D warnings
```

Expected: No warnings.

- [ ] **Step 9: Commit**

Write `commit.txt`:
```
feat(otel): add OTLP tracing export with composable layer stack

TracerProvider with resource attributes (service.name, deployment.environment,
service.version). HTTP protobuf export via hyper-client (default), gRPC via
otel-grpc feature. Composable with logging layer on one tracing registry.
OtelGuard flushes provider on drop.
```

---

### Task 6: MCP Module

**Files:**
- Modify: `src/mcp.rs`
- Create: `tests/mcp_test.rs`

The MCP module is a thin wrapper around rmcp. It re-exports key types and provides a helper for the common "serve on stdio" pattern. No deep builder integration — MCP servers are started in command handlers, not during init.

- [ ] **Step 1: Write failing tests**

Create `tests/mcp_test.rs`:

```rust
#![allow(missing_docs)]
#![cfg(feature = "mcp")]

#[test]
fn transport_stdio_type_is_accessible() {
    // Verify re-exports compile
    let _: fn() -> _ = rebar::mcp::transport_stdio;
}

#[test]
fn service_ext_trait_is_accessible() {
    // The ServiceExt trait should be re-exported
    use rebar::mcp::ServiceExt as _;
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run --features mcp -E 'binary(mcp_test)'
```

Expected: FAIL — re-exports don't exist.

- [ ] **Step 3: Implement MCP module**

Write `src/mcp.rs`:

```rust
//! MCP server helpers wrapping rmcp.
//!
//! Re-exports key rmcp types and provides a convenience function for
//! the common pattern of serving an MCP server on stdio.
//!
//! # Usage
//!
//! ```ignore
//! use rebar::mcp::ServiceExt;
//!
//! let server = MyServer::new();
//! let service = server.serve(rebar::mcp::transport_stdio()).await?;
//! service.waiting().await?;
//! ```

// Re-export key types consumers need
pub use rmcp::ServiceExt;
pub use rmcp::model;
pub use rmcp::handler;

/// Create a stdio transport for MCP communication.
///
/// Wrapper around `rmcp::transport::stdio()`. This is the standard
/// transport for CLI-based MCP servers.
pub fn transport_stdio() -> rmcp::transport::io::StdioTransport {
    rmcp::transport::stdio()
}
```

Note: The exact types returned by `rmcp::transport::stdio()` may differ. Check rmcp 1.3 docs for the actual return type and adjust the function signature. If `StdioTransport` isn't a public type, use `impl Transport` or return the value without naming the type:

```rust
pub fn transport_stdio() -> impl rmcp::Transport {
    rmcp::transport::stdio()
}
```

The implementer should check rmcp's API and adjust.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo nextest run --features mcp -E 'binary(mcp_test)'
```

Expected: All 2 tests PASS.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --features mcp --all-targets --message-format=short -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

Write `commit.txt`:
```
feat(mcp): add rmcp wrapper with stdio transport helper

Re-exports key rmcp types (ServiceExt, model, handler) and provides
transport_stdio() convenience function for the common MCP-over-stdio
pattern.
```

---

### Task 7: Feature Isolation Verification

**Files:** None (verification only)

- [ ] **Step 1: Verify no-feature compilation**

```bash
cargo check --no-default-features
```

Expected: PASS.

- [ ] **Step 2: Verify each feature in isolation**

```bash
cargo check --features cli
cargo check --features config
cargo check --features logging
cargo check --features shutdown
cargo check --features crash
cargo check --features otel
cargo check --features mcp
```

Expected: All PASS. `otel` implicitly activates `logging` — that's expected.

- [ ] **Step 3: Verify all features together**

```bash
cargo clippy --features cli,config,logging,shutdown,crash,otel,mcp --all-targets --message-format=short -- -D warnings
```

Expected: PASS, no warnings.

- [ ] **Step 4: Run full test suite**

```bash
cargo nextest run --features cli,config,logging,shutdown,crash,otel,mcp
```

Expected: All tests PASS.

- [ ] **Step 5: Run cargo fmt and cargo doc**

```bash
cargo fmt --all
cargo doc --features cli,config,logging,shutdown,crash,otel,mcp --no-deps 2>&1 | grep -c "warning" || true
```

Expected: No format changes. Zero or minimal doc warnings.

- [ ] **Step 6: Commit (if fmt made changes)**

Write `commit.txt`:
```
chore: verify Phase 2 feature isolation and run full test suite

All features compile in isolation and together. Full test suite passes.
```

---

## Implementation Notes

### opentelemetry-otlp Feature Names

The `http-proto` and `hyper-client` feature names need verification against the actual `opentelemetry-otlp` 0.31 crate. The template uses `grpc-tonic`, `http-json`, `reqwest-blocking-client` — the naming pattern suggests `http-proto` exists but `hyper-client` may be named differently (e.g., `hyper`). Check `opentelemetry-otlp`'s `Cargo.toml` on crates.io before writing the dependency.

### tracing_subscriber::registry().init() Is Once-Per-Process

Same landmine as Phase 1. Builder tests that call `start()` with logging or OTEL will conflict if run in the same process. Use `cargo nextest run` (process-per-test). Do not switch to `cargo test`.

### Test Attribute Ordering

`#![allow(missing_docs)]` must come BEFORE `#![cfg(feature = "...")]` in integration test files. Same landmine as Phase 1.

### OTEL Version Alignment

All opentelemetry crates must be version-aligned. `opentelemetry` 0.31, `opentelemetry_sdk` 0.31, `opentelemetry-otlp` 0.31, and `tracing-opentelemetry` 0.32 are the current stable set. If a version mismatch causes compilation errors, check the opentelemetry-rust compatibility matrix.

### Timestamp Duplication

`crash.rs` duplicates the `format_timestamp()` and `days_to_ymd()` functions from `logging.rs`. This is intentional — `crash` is standalone (no `logging` feature dependency). If this bothers you during implementation, extract to a `src/time.rs` utility module (no feature gate) and have both modules use it. But that's optional polish, not a requirement.

### rmcp API Surface

The rmcp 1.3 API may have changed since the template was written. The implementer should check:
- Is `rmcp::transport::stdio()` still the correct function?
- What does `StdioTransport` look like? Is it a named public type?
- Does `ServiceExt` still exist by that name?

Adjust the `mcp.rs` re-exports to match the actual API.

### Builder Version Parameter

The crash and OTEL modules use `env!("CARGO_PKG_VERSION")` which captures rebar's version at compile time. Consumers will want their own version in crash dumps and OTEL resource attributes. Consider adding `.with_version(env!("CARGO_PKG_VERSION"))` to the builder API. This is not required for Phase 2 but is a known gap.
