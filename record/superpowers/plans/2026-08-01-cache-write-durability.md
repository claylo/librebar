# Cache Write Durability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove stable-storage synchronization from disposable cache writes while retaining one durability sync for credential-bearing cookie writes.

**Architecture:** The generic cache writes through a same-directory `tempfile::NamedTempFile` and atomically persists it without syncing. The cookie jar keeps `atomic-write-file`, but relies only on `commit()` for its durability barrier.

**Tech Stack:** Rust 2024, `tempfile` 3.27, `atomic-write-file` 0.3, Cargo feature metadata, Nextest, Just.

---

### Task 1: Lock the dependency boundary with a failing manifest test

**Files:**
- Modify: `tests/default_features_test.rs`

- [x] **Step 1: Extract the package metadata helper**

Replace the inline metadata setup in `manifest_enables_the_application_foundation_by_default` with this shared helper and call it from the existing test:

```rust
fn package_metadata() -> serde_json::Value {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("cargo metadata should run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata should emit JSON");
    metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages
                .iter()
                .find(|package| package["name"] == env!("CARGO_PKG_NAME"))
        })
        .cloned()
        .expect("librebar package should be present")
}
```

The existing test begins with:

```rust
let package = package_metadata();
```

- [x] **Step 2: Write the failing feature-contract test**

Add:

```rust
#[test]
fn manifest_separates_cache_and_cookie_persistence_dependencies() {
    let package = package_metadata();
    let features = package["features"]
        .as_object()
        .expect("feature map should be present");
    let feature_dependencies = |feature: &str| {
        features[feature]
            .as_array()
            .expect("feature dependencies should be present")
            .iter()
            .map(|dependency| dependency.as_str().expect("dependency should be a string"))
            .collect::<Vec<_>>()
    };

    let cache = feature_dependencies("cache");
    assert!(cache.contains(&"dep:tempfile"));
    assert!(!cache.contains(&"dep:atomic-write-file"));

    let cookies = feature_dependencies("http-cookies");
    assert!(cookies.contains(&"dep:atomic-write-file"));
    assert!(!cookies.contains(&"dep:tempfile"));
}
```

- [x] **Step 3: Run the test and confirm RED**

Run:

```bash
cargo test --test default_features_test manifest_separates_cache_and_cookie_persistence_dependencies -- --exact
```

Expected: FAIL because `cache` still selects `dep:atomic-write-file` and does not select `dep:tempfile`.

### Task 2: Split cache and cookie durability policies

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/cache.rs`
- Modify: `src/http/cookies.rs`
- Verify: `Cargo.lock`

- [x] **Step 1: Make `tempfile` the cache writer dependency**

Add the optional runtime dependency under the cache feature dependencies while retaining the dev dependency:

```toml
# Feature: cache
tempfile = { version = "3.27", optional = true }
```

Change the feature edge to:

```toml
cache = ["dep:serde_json", "dep:directories", "dep:base64", "dep:tempfile"]
```

Do not change `http-cookies`; it continues selecting `dep:atomic-write-file`.

- [x] **Step 2: Replace the cache writer**

Remove the `atomic_write_file::AtomicWriteFile` import from `src/cache.rs`. Replace `write_entry` with:

```rust
fn write_entry(path: &Path, header: &[u8], parts: &[&[u8]]) -> std::io::Result<()> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(".librebar-cache-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        builder.permissions(std::fs::Permissions::from_mode(0o600));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut file = builder.tempfile_in(parent)?;
    file.write_all(header)?;
    for part in parts {
        file.write_all(part)?;
    }
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
```

This preserves owner-only creation and same-directory atomic replacement but deliberately performs no file or directory sync.

- [x] **Step 3: Remove the cookie jar's duplicate sync**

Delete the explicit call:

```rust
file.sync_all()
    .map_err(|source| cookie_error("save", path, source))?;
```

Keep `file.commit()` unchanged; it supplies the cookie writer's one durability sync.

- [x] **Step 4: Run focused tests and confirm GREEN**

Run:

```bash
cargo test --test default_features_test manifest_separates_cache_and_cookie_persistence_dependencies -- --exact
cargo test --test cache_test --features cache
cargo test --test http_cookies_test --features http-cookies
cargo hack check --each-feature --no-dev-deps
```

Expected: all commands pass. `Cargo.lock` should remain unchanged because `tempfile` is already locked as a dev dependency; inspect and preserve it if Cargo makes no semantic change.

### Task 3: Verify and record the remediation

**Files:**
- Modify: `record/audits/2026-08-01-00-full-repo/actions-taken.md` (never stage)
- Create: `commit.txt` (gitignored)

- [x] **Step 1: Run the repository gate**

Run:

```bash
just check
```

Expected: formatting, Clippy, dependency policy, all Nextest tests, doctests, and API documentation pass.

- [x] **Step 2: Append the audit action**

Update the front matter to `fixed: 19` and `open: 44`, then append an entry addressing `cache-set-fsync-per-write`. Record `pending (working tree)` until Clay commits it. State that cache entries use same-directory `NamedTempFile::persist` without sync, cookie writes retain the single `AtomicWriteFile::commit` sync, and list the focused plus full verification results.

- [x] **Step 3: Stage the remediation, excluding the ledger**

Run:

```bash
git --no-pager add Cargo.toml src/cache.rs src/http/cookies.rs tests/default_features_test.rs docs/superpowers/plans/2026-08-01-cache-write-durability.md
git --no-pager diff --cached --check
git --no-pager diff --cached --name-only
```

Expected: only implementation, regression test, and plan files are staged. `record/audits/2026-08-01-00-full-repo/actions-taken.md` remains unstaged.

- [x] **Step 4: Write `commit.txt`**

Write:

```text
fix(cache): avoid durable syncs for disposable entries

Persist cache entries through same-directory atomic renames without
forcing stable storage. Keep one durability sync for cookie jars.

Release-Note: Speed up persistent cache writes
Release-Impact: low
```

Clay commits with `gtxt`.
