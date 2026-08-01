# Structured JSON Crash Dumps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make on-disk crash dumps valid JSON matching librebar's documented contract while retaining the human-readable formatter.

**Architecture:** `CrashInfo` remains the single crash-data model and gains `serde::Serialize`. `write_crash_dump_to` streams that model through `serde_json` into the existing owner-only, create-new file before applying the existing retention policy.

**Tech Stack:** Rust, `serde`, `serde_json`, Cargo feature flags, existing crash integration tests.

---

### Task 1: Serialize crash dumps as JSON

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/crash.rs`
- Test: `tests/crash_test.rs`

- [x] **Step 1: Write the failing JSON contract test**

Add a test that writes a `CrashInfo` containing a multiline message, parses the file with `serde_json::from_slice`, and asserts all seven fields:

```rust
#[test]
fn crash_dump_is_structured_json() {
    let tmp = TempDir::new().unwrap();
    let info = crash::CrashInfo {
        message: "first line\nsecond line".to_string(),
        location: Some("src/main.rs:42".to_string()),
        app_name: "test-app".to_string(),
        version: "0.1.0".to_string(),
        timestamp: "2026-04-08T12:00:00.000Z".to_string(),
        os: "macos".to_string(),
        backtrace: "0: test::frame".to_string(),
    };

    let path = crash::write_crash_dump_to(&info, tmp.path()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();

    assert_eq!(value["message"], "first line\nsecond line");
    assert_eq!(value["location"], "src/main.rs:42");
    assert_eq!(value["app_name"], "test-app");
    assert_eq!(value["version"], "0.1.0");
    assert_eq!(value["timestamp"], "2026-04-08T12:00:00.000Z");
    assert_eq!(value["os"], "macos");
    assert_eq!(value["backtrace"], "0: test::frame");
}
```

- [x] **Step 2: Run the test and verify the existing text dump fails JSON parsing**

Run: `cargo test --features crash --test crash_test crash_dump_is_structured_json -- --exact`

Expected: FAIL because `serde_json::from_slice` encounters `=== Crash Report ===`.

- [x] **Step 3: Implement direct JSON serialization**

Enable `serde_json` for the feature:

```toml
crash = ["dep:serde_json"]
```

Derive serialization and write directly to the private file:

```rust
#[derive(Debug, serde::Serialize)]
pub struct CrashInfo {
    pub message: String,
    pub location: Option<String>,
    pub app_name: String,
    pub version: String,
    pub timestamp: String,
    pub os: String,
    pub backtrace: String,
}

let Ok(mut file) = options.open(&path) else {
    return None;
};
if serde_json::to_writer(&mut file, info).is_err() {
    let _ = std::fs::remove_file(&path);
    return None;
}
```

Keep `CrashInfo::format` unchanged for human-readable output and keep the existing `0600`, collision, cleanup-on-error, and ten-file retention behavior.

- [x] **Step 4: Run focused and isolated-feature checks**

Run: `cargo test --features crash --test crash_test`

Expected: all crash integration tests pass.

Run: `cargo check --no-default-features --features crash`

Expected: the isolated crash feature compiles with `serde_json` enabled.

- [x] **Step 5: Run repository verification**

Run: `just check`

Expected: formatting, clippy, dependency policy, tests, doctests, and docs pass.

Run: `just feature-matrix`

Expected: all 21 feature configurations pass.

Run: `RUSTUP_TOOLCHAIN=1.89.0 just msrv-check`

Expected: all targets and features compile on Rust 1.89.

- [x] **Step 6: Prepare the commit**

Stage `Cargo.toml`, `src/crash.rs`, `tests/crash_test.rs`, and this plan, but not
the audit ledger. Write a conventional `fix(crash)` message with the required
scrat release trailers to `commit.txt`, then have Clay run `gtxt`.
