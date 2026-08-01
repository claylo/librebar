# Expired Cache Entry Pruning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove expired on-disk cache entries whose keys are never read again, while avoiding a directory scan on every write and preserving every live entry.

**Architecture:** Add a public header-only `Cache::prune()` sweep, then invoke it before the first write and at most hourly through a shared atomic timestamp. Make `Cache` cheaply cloneable so async HTTP-cache adapters preserve that cadence when moving handles onto Tokio's blocking pool.

**Tech Stack:** Rust 1.89 standard-library filesystem and atomic APIs, existing v2 cache framing, existing `tracing`, integration tests, Just, cargo-hack.

---

## File Map

- `src/cache.rs` — public pruning API, header-only sweep, shared cadence state,
  cache retention documentation.
- `src/http/cache.rs` — clone the caller's cache handle across blocking-I/O
  boundaries instead of reconstructing it from the directory.
- `tests/cache_test.rs` — public behavior regressions for explicit and
  opportunistic pruning.
- `tests/http_cache_test.rs` — regression proving async adapters preserve the
  caller's shared prune cadence.
- `record/superpowers/specs/2026-08-01-expired-cache-pruning-design.md` —
  approved design record.
- `record/superpowers/plans/2026-08-01-expired-cache-pruning.md` — this
  implementation record.
- `record/audits/2026-08-01-00-full-repo/actions-taken.md` — append-only Cased
  remediation ledger; never stage it.
- `commit.txt` — gitignored conventional commit message consumed by `gtxt`.

### Task 1: Add explicit header-only pruning

**Files:**
- Modify: `tests/cache_test.rs`
- Modify: `src/cache.rs`

- [x] **Step 1: Write the explicit-prune regressions**

Insert these tests after `expired_entry_returns_none` in
`tests/cache_test.rs`:

```rust
#[test]
fn prune_removes_only_expired_v2_entries() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    let unrelated = tmp.path().join("notes.txt");
    let malformed = tmp.path().join("v2-malformed.cache");

    cache.set("expired", b"old", Duration::ZERO).unwrap();
    cache
        .set("live", b"current", Duration::from_secs(60))
        .unwrap();
    std::fs::write(&unrelated, b"keep").unwrap();
    std::fs::write(&malformed, b"not a cache header").unwrap();

    assert_eq!(cache.prune().unwrap(), 1);
    assert!(!cache_entry_path(tmp.path(), "expired").exists());
    assert!(cache_entry_path(tmp.path(), "live").exists());
    assert!(unrelated.exists());
    assert!(malformed.exists());
}

#[test]
fn prune_treats_a_missing_directory_as_empty() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("missing");
    let cache = Cache::new(&missing);

    assert_eq!(cache.prune().unwrap(), 0);
    assert!(!missing.exists());
}
```

The first test deliberately combines one expired v2 entry, one live v2 entry,
one unrelated file, and one malformed v2 candidate. It proves a damaged entry
does not block later candidates and that pruning does not broaden into generic
directory cleanup.

- [x] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test --features cache --test cache_test prune_ -- --nocapture
```

Expected: compilation fails because `Cache::prune` does not exist.

- [x] **Step 3: Add the public sweep and shared header decoder**

In `src/cache.rs`, add this private candidate predicate after
`default_cache_dir` or beside the framing helpers:

```rust
fn is_v2_cache_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("v2-") && name.ends_with(".cache"))
}
```

Add this header reader before `read_entry`:

```rust
fn read_expiry(file: &mut std::fs::File) -> std::result::Result<u64, CacheError> {
    let mut header = [0_u8; CACHE_HEADER_LEN];
    if let Err(error) = file.read_exact(&mut header) {
        return if error.kind() == std::io::ErrorKind::UnexpectedEof {
            Err(CacheError::Format("truncated header".to_string()))
        } else {
            Err(error.into())
        };
    }
    if &header[..CACHE_MAGIC.len()] != CACHE_MAGIC {
        return Err(CacheError::Format(
            "unsupported magic or version".to_string(),
        ));
    }

    Ok(u64::from_be_bytes(
        header[CACHE_MAGIC.len()..]
            .try_into()
            .expect("cache expiry occupies eight bytes"),
    ))
}

fn read_expiry_at(path: &Path) -> std::result::Result<u64, CacheError> {
    let mut file = std::fs::File::open(path)?;
    read_expiry(&mut file)
}
```

Refactor `read_entry` to reuse the decoder and then read the body:

```rust
fn read_entry(path: &Path) -> std::result::Result<(u64, Vec<u8>), CacheError> {
    let mut file = std::fs::File::open(path)?;
    let expires_at = read_expiry(&mut file)?;
    let mut value = Vec::new();
    file.read_to_end(&mut value)?;
    Ok((expires_at, value))
}
```

Add the public method inside `impl Cache`, before `remove`:

```rust
    /// Remove expired entries and return the number removed.
    ///
    /// Only v2 cache files are inspected. Missing directories are treated as
    /// empty, and malformed or individually inaccessible entries are skipped.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Cache`](crate::Error::Cache) if the cache directory
    /// cannot be read.
    pub fn prune(&self) -> Result<usize> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.prune_at(now)
    }

    fn prune_at(&self, now: u64) -> Result<usize> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(CacheError::from(error).into()),
        };
        let mut removed = 0;

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to inspect cache directory entry");
                    continue;
                }
            };
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
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(error = %error, "failed to remove expired cache entry");
                }
            }
        }

        Ok(removed)
    }
```

Do not include the encoded filename or original key in warning fields.

- [x] **Step 4: Run the explicit-prune tests and verify GREEN**

Run:

```bash
cargo test --features cache --test cache_test prune_ -- --nocapture
```

Expected: both prune tests pass. The malformed candidate may emit a warning
only when a tracing subscriber is installed; it remains on disk.

### Task 2: Add opportunistic hourly maintenance shared by clones

**Files:**
- Modify: `tests/cache_test.rs`
- Modify: `src/cache.rs`

- [x] **Step 1: Write automatic-pruning and clone-cadence regressions**

Insert the first test after the explicit-prune tests, run its RED command from
Step 2, then insert the second test and run its RED command. Rust compiles the
entire integration-test binary even when one test name is filtered, so this
order preserves both independent failure observations.

```rust
#[test]
fn first_write_through_a_new_handle_prunes_expired_entries() {
    let tmp = TempDir::new().unwrap();
    let expired_path = cache_entry_path(tmp.path(), "expired");

    let original = Cache::new(tmp.path());
    original
        .set("expired", b"old", Duration::ZERO)
        .unwrap();
    assert!(expired_path.exists());

    let restarted = Cache::new(tmp.path());
    restarted
        .set("fresh", b"current", Duration::from_secs(60))
        .unwrap();

    assert!(!expired_path.exists());
    assert_eq!(
        restarted.get("fresh").unwrap().as_deref(),
        Some(b"current".as_ref())
    );
}

#[test]
fn cloned_handles_share_the_automatic_prune_cadence() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    cache
        .set("seed", b"current", Duration::from_secs(60))
        .unwrap();
    cache.set("expired", b"old", Duration::ZERO).unwrap();
    let expired_path = cache_entry_path(tmp.path(), "expired");
    assert!(expired_path.exists());

    let cloned = cache.clone();
    cloned
        .set("next", b"current", Duration::from_secs(60))
        .unwrap();

    assert!(expired_path.exists());
    assert_eq!(cache.prune().unwrap(), 1);
}
```

The first handle's first write claims its initial sweep before writing the
zero-TTL entry. A newly constructed handle must sweep that backlog. The clone
test proves a clone does not get an independent first-write sweep.

- [x] **Step 2: Run the new tests and verify RED**

Run:

```bash
cargo test --features cache --test cache_test first_write_through_a_new_handle_prunes_expired_entries -- --exact
cargo test --features cache --test cache_test cloned_handles_share_the_automatic_prune_cadence -- --exact
```

Expected: the first test fails because `set` does not prune; the second fails
to compile because `Cache` does not implement `Clone`.

- [x] **Step 3: Add shared cadence state and automatic maintenance**

Add these imports and constant in `src/cache.rs`:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const AUTOMATIC_PRUNE_INTERVAL_SECS: u64 = 60 * 60;
```

Change the cache type and constructor to:

```rust
#[derive(Clone, Debug)]
pub struct Cache {
    dir: PathBuf,
    last_prune: Arc<AtomicU64>,
}

pub fn new(dir: &Path) -> Self {
    Self {
        dir: dir.to_path_buf(),
        last_prune: Arc::new(AtomicU64::new(0)),
    }
}
```

Inside `set_parts`, use one integer timestamp and attempt maintenance after
creating the directory but before writing the requested entry:

```rust
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.prune_if_due(now);
        let expires_at = now.saturating_add(ttl.as_secs());
```

Change public `prune` so a completed explicit sweep also advances the shared
cadence:

```rust
    pub fn prune(&self) -> Result<usize> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let removed = self.prune_at(now)?;
        self.last_prune.store(now, Ordering::Relaxed);
        Ok(removed)
    }
```

Add this helper beside `prune_at`:

```rust
    fn prune_if_due(&self, now: u64) {
        let previous = self.last_prune.load(Ordering::Relaxed);
        if previous != 0 && now.saturating_sub(previous) < AUTOMATIC_PRUNE_INTERVAL_SECS {
            return;
        }
        if self
            .last_prune
            .compare_exchange(previous, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        if let Err(error) = self.prune_at(now) {
            let _ = self.last_prune.compare_exchange(
                now,
                previous,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
            tracing::warn!(error = %error, "automatic cache pruning failed");
        }
    }
```

Relaxed ordering is sufficient because the atomic coordinates scan cadence;
it does not publish cache entry memory. Keep pruning best-effort so an
unrelated maintenance failure cannot turn a valid `set` into an error.

- [x] **Step 4: Document the retention contract**

Update the module introduction to say expired entries are removed on access,
explicit pruning, and periodic write-path maintenance. Expand the `Cache` type
documentation to state:

```rust
/// File-based cache with TTL support.
///
/// Expired entries are removed when read, when [`Cache::prune`] is called, and
/// opportunistically before the first write and at most hourly afterward.
/// The cache does not impose an entry-count or byte-size ceiling.
```

- [x] **Step 5: Run all generic-cache tests and verify GREEN**

Run:

```bash
just fmt
cargo test --features cache --test cache_test
```

Expected: all existing generic-cache tests plus the four pruning regressions
pass.

### Task 3: Preserve cadence across async HTTP cache adapters

**Files:**
- Modify: `src/http/cache.rs`
- Modify: `tests/http_cache_test.rs`

- [x] **Step 1: Write and verify the HTTP adapter RED**

Add a `cache_entry_path` helper matching the v2 filename convention and an
integration test that first claims the caller's sweep cadence, writes a
zero-TTL generic entry, and then persists an HTTP response through
`get_cached`:

```rust
fn cache_entry_path(dir: &std::path::Path, key: &str) -> std::path::PathBuf {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.as_bytes());
    dir.join(format!("v2-{encoded}.cache"))
}

#[tokio::test]
async fn http_cache_writes_share_the_callers_prune_cadence() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    cache
        .set("seed", b"current", Duration::from_secs(60))
        .unwrap();
    cache.set("expired", b"old", Duration::ZERO).unwrap();
    let expired_path = cache_entry_path(cache_dir.path(), "expired");
    assert!(expired_path.exists());

    let (address, requests, server) = spawn_server(1, |_, _| {
        response(
            "200 OK",
            &[("Cache-Control", "max-age=3600")],
            b"version one",
        )
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    client
        .get_cached(&cache, "item", &format!("http://{address}/item"))
        .await
        .unwrap();

    assert!(expired_path.exists());
    requests.recv().unwrap();
    server.join().unwrap();
}
```

Run:

```bash
cargo test --features http-cache --test http_cache_test http_cache_writes_share_the_callers_prune_cadence -- --exact
```

Expected: the test fails at the final `expired_path.exists()` assertion because
the reconstructed persistence handle performs another first-write sweep.

- [x] **Step 2: Replace reconstructed handles with clones**

At the beginning of `load_entry`, `remove_entry`, and
`persist_cached_response`, replace:

```rust
let cache = Cache::new(cache.dir());
```

with:

```rust
let cache = cache.clone();
```

These owned clones satisfy the `'static` blocking-closure requirement while
sharing the automatic-prune timestamp with the caller's handle.

- [x] **Step 3: Prove no reconstruction sites remain**

Run:

```bash
rg -n 'Cache::new\(cache\.dir\(\)\)' src/http/cache.rs
```

Expected: no matches.

- [x] **Step 4: Run focused HTTP-cache verification**

Run:

```bash
cargo test --features http-cache --test http_cache_test
cargo check --no-default-features --features cache
```

Expected: all HTTP-cache integration tests pass, and the standalone cache
feature compiles without Tokio-specific additions.

### Task 4: Verify the complete remediation

**Files:**
- Verify: `src/cache.rs`
- Verify: `src/http/cache.rs`
- Verify: `tests/cache_test.rs`
- Verify: `tests/http_cache_test.rs`
- Verify: `record/superpowers/specs/2026-08-01-expired-cache-pruning-design.md`
- Verify: `record/superpowers/plans/2026-08-01-expired-cache-pruning.md`

- [x] **Step 1: Inspect the focused diff and formatting**

Run:

```bash
git --no-pager diff --check
git --no-pager diff -- src/cache.rs src/http/cache.rs tests/cache_test.rs tests/http_cache_test.rs record/superpowers/specs/2026-08-01-expired-cache-pruning-design.md record/superpowers/plans/2026-08-01-expired-cache-pruning.md
```

Expected: no whitespace errors; changes are limited to pruning, shared cache
handles, tests, and the approved records.

- [x] **Step 2: Run repository checks**

Run:

```bash
just check
```

Expected: formatting, clippy, nextest, and doctests all pass.

- [x] **Step 3: Run feature and MSRV checks**

Run:

```bash
just feature-matrix
RUSTUP_TOOLCHAIN=1.89.0 just msrv-check
```

Expected: all 21 cargo-hack configurations pass, followed by the Rust 1.89
all-targets check.

### Task 5: Record and prepare Clay's single-commit handoff

**Files:**
- Modify but do not stage: `record/audits/2026-08-01-00-full-repo/actions-taken.md`
- Stage: `src/cache.rs`
- Stage: `src/http/cache.rs`
- Stage: `tests/cache_test.rs`
- Stage: `tests/http_cache_test.rs`
- Stage: `record/superpowers/specs/2026-08-01-expired-cache-pruning-design.md`
- Stage: `record/superpowers/plans/2026-08-01-expired-cache-pruning.md`
- Create but do not stage: `commit.txt`

- [x] **Step 1: Append the Cased remediation record**

Update only the ledger front matter counts to `fixed: 18` and `open: 45`,
keeping the other disposition counts unchanged. Append one `fixed` entry for
`cache-has-no-eviction-outside-per-key-reads` with `Commit: pending (working
tree)`, author `Codex`, the explicit and automatic pruning behavior,
the RED/GREEN evidence, and the exact focused/full verification results. Do
not alter earlier entries.

- [x] **Step 2: Stage only remediation files**

Run:

```bash
git --no-pager add src/cache.rs src/http/cache.rs tests/cache_test.rs tests/http_cache_test.rs record/superpowers/specs/2026-08-01-expired-cache-pruning-design.md record/superpowers/plans/2026-08-01-expired-cache-pruning.md
git --no-pager diff --cached --check
git --no-pager diff --cached --name-only
```

Expected: exactly the six listed files are staged. The audit ledger remains
unstaged.

- [x] **Step 3: Write `commit.txt` for `gtxt`**

Create gitignored `commit.txt` with:

```text
fix(cache): prune expired entries during active writes

Add an explicit header-only prune operation and run it before the first cache
write and at most hourly afterward. Share sweep cadence across cloned handles
without imposing live-entry count or byte ceilings.

Release-Note: Prune expired cache entries during active writes
Release-Impact: medium
```

- [x] **Step 4: Review the handoff**

Run:

```bash
git --no-pager diff --cached --stat
git --no-pager diff --cached --check
git --no-pager status --short
git check-ignore -v commit.txt
```

Expected: implementation, tests, spec, and plan are staged; the append-only
audit ledger is unstaged; `commit.txt` is ignored and ready for `gtxt`.
