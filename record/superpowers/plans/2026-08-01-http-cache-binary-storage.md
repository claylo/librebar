# Bytes-Native HTTP Cache Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store general cache values and HTTP response bodies as raw bytes with bounded framing overhead and no redundant body clone.

**Architecture:** Replace the generic cache's JSON/base64 envelope with a fixed 16-byte v2 header and multipart atomic writes. Frame HTTP cache values as raw body bytes followed by JSON metadata and a fixed trailer, allowing the read path to recover the body by truncating its owned buffer.

**Tech Stack:** Rust standard I/O, serde/serde_json already present, atomic-write-file, existing Cargo integration tests, Just, cargo-hack.

---

### Task 1: Make `Cache` bytes-native

**Files:**
- Modify: `src/cache.rs`
- Modify: `src/error.rs`
- Test: `tests/cache_test.rs`

- [x] **Step 1: Add failing raw-layout and malformed-header tests**

Add this helper after the imports in `tests/cache_test.rs`:

```rust
fn cache_entry_path(dir: &std::path::Path, key: &str) -> std::path::PathBuf {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.as_bytes());
    dir.join(format!("v2-{encoded}.cache"))
}
```

Add these tests:

```rust
#[test]
fn cache_file_stores_the_raw_binary_value() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    let value = (0_u8..=255).cycle().take(4096).collect::<Vec<_>>();

    cache
        .set("binary", &value, Duration::from_secs(60))
        .unwrap();

    let stored = std::fs::read(cache_entry_path(tmp.path(), "binary")).unwrap();
    assert_eq!(stored.len(), value.len() + 16);
    assert_eq!(&stored[16..], value);
    assert_eq!(cache.get("binary").unwrap().unwrap(), value);
}

#[test]
fn malformed_cache_header_is_a_format_error() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::new(tmp.path());
    std::fs::write(cache_entry_path(tmp.path(), "broken"), b"not a cache header").unwrap();

    let error = cache.get("broken").unwrap_err();

    assert!(
        error.to_string().contains("invalid cache entry format"),
        "{error}"
    );
}
```

- [x] **Step 2: Run both tests and verify RED**

Run:

```bash
cargo test --features cache --test cache_test cache_file_stores_the_raw_binary_value -- --exact
cargo test --features cache --test cache_test malformed_cache_header_is_a_format_error -- --exact
```

Expected: the first test fails because the v2 file does not exist; the second
fails because the current v1 lookup returns `None` rather than a format error.

- [x] **Step 3: Add an explicit cache-format error**

Add this variant to `CacheError` in `src/error.rs` without removing the public
JSON or base64 variants:

```rust
    /// Invalid or unsupported on-disk cache framing.
    #[error("invalid cache entry format: {0}")]
    Format(String),
```

- [x] **Step 4: Replace the outer JSON envelope with a v2 binary header**

In `src/cache.rs`, replace the module description, imports, `CacheEntry`, and
the `set`/`get` internals with the following framing helpers and methods:

```rust
//! XDG cache storage with TTL support.
//!
//! Provides a simple key-value cache backed by owner-only, atomically replaced
//! files. Each v2 entry stores a fixed expiry header followed by the raw value.

use std::io::{Read as _, Write as _};

const CACHE_MAGIC: &[u8; 8] = b"LBRCA02\0";
const CACHE_HEADER_LEN: usize = 16;
```

```rust
    pub fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
        self.set_parts(key, &[value], ttl)
    }

    pub(crate) fn set_parts(
        &self,
        key: &str,
        parts: &[&[u8]],
        ttl: Duration,
    ) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(CacheError::from)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let expires_at = now.as_secs().saturating_add(ttl.as_secs());
        let mut header = [0_u8; CACHE_HEADER_LEN];
        header[..CACHE_MAGIC.len()].copy_from_slice(CACHE_MAGIC);
        header[CACHE_MAGIC.len()..].copy_from_slice(&expires_at.to_be_bytes());

        let path = self.key_path(key);
        write_entry(&path, &header, parts).map_err(CacheError::from)?;

        tracing::debug!(key, expires_at, "cache entry written");
        Ok(())
    }
```

Update `Cache::get`'s `# Errors` text to:

```rust
    /// Returns [`Error::Cache`](crate::Error::Cache) on I/O errors or invalid
    /// cache framing.
```

```rust
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.key_path(key);
        let (expires_at, value) = match read_entry(&path) {
            Ok(entry) => entry,
            Err(CacheError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now >= expires_at {
            tracing::debug!(key, "cache entry expired");
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }

        Ok(Some(value))
    }
```

Use v2 paths and extensions:

```rust
    fn key_path(&self, key: &str) -> PathBuf {
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.as_bytes());
        self.dir.join(format!("v2-{encoded}.cache"))
    }
```

Change `clear`'s extension comparison to `Some("cache")`.

Add these private helpers:

```rust
fn read_entry(path: &Path) -> std::result::Result<(u64, Vec<u8>), CacheError> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0_u8; CACHE_HEADER_LEN];
    if let Err(error) = file.read_exact(&mut header) {
        return if error.kind() == std::io::ErrorKind::UnexpectedEof {
            Err(CacheError::Format("truncated header".to_string()))
        } else {
            Err(error.into())
        };
    }
    if &header[..CACHE_MAGIC.len()] != CACHE_MAGIC {
        return Err(CacheError::Format("unsupported magic or version".to_string()));
    }

    let expires_at = u64::from_be_bytes(
        header[CACHE_MAGIC.len()..]
            .try_into()
            .expect("cache expiry occupies eight bytes"),
    );
    let mut value = Vec::new();
    file.read_to_end(&mut value)?;
    Ok((expires_at, value))
}

fn write_entry(path: &Path, header: &[u8], parts: &[&[u8]]) -> std::io::Result<()> {
    let mut options = AtomicWriteFile::options();
    #[cfg(unix)]
    {
        use atomic_write_file::unix::OpenOptionsExt as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).preserve_mode(false);
    }
    let mut file = options.open(path)?;
    file.write_all(header)?;
    for part in parts {
        file.write_all(part)?;
    }
    file.sync_all()?;
    file.commit()
}
```

- [x] **Step 5: Update path-sensitive cache tests**

Replace the hand-built `v1-{encoded}.json` paths in
`cache_set_replaces_symlink_without_following_it` and
`cache_set_refuses_to_replace_a_directory` with:

```rust
let entry = cache_entry_path(tmp.path(), "key");
```

The helper owns the base64 usage, so remove duplicate local `encoded`
variables but retain `use base64::Engine as _;` at module scope.

- [x] **Step 6: Run the complete cache suite and isolated feature check**

Run:

```bash
cargo test --features cache --test cache_test
cargo check --no-default-features --features cache
```

Expected: all 13 cache integration tests pass and the isolated cache feature
compiles without warnings.

---

### Task 2: Store HTTP bodies outside JSON

**Files:**
- Modify: `src/http/cache.rs`
- Test: `tests/http_cache_test.rs`

- [x] **Step 1: Add the failing 1 MiB amplification regression test**

Add this integration test after the basic fresh-hit test:

```rust
#[tokio::test]
async fn large_cached_body_has_bounded_storage_overhead() {
    const BODY_LEN: usize = 1024 * 1024;

    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(cache_dir.path());
    let (address, requests, server) = spawn_server(1, |_, _| {
        let body = vec![b'x'; BODY_LEN];
        response(
            "200 OK",
            &[("Cache-Control", "max-age=3600")],
            &body,
        )
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();
    let url = format!("http://{address}/large");

    let miss = client.get_cached(&cache, "large", &url).await.unwrap();
    let stored_len = std::fs::read_dir(cache_dir.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .metadata()
        .unwrap()
        .len() as usize;
    let hit = client.get_cached(&cache, "large", &url).await.unwrap();

    assert_eq!(miss.bytes().len(), BODY_LEN);
    assert_eq!(hit.cache_status(), Some(CacheStatus::Hit));
    assert_eq!(hit.bytes().len(), BODY_LEN);
    assert!(
        stored_len <= BODY_LEN + 16 * 1024,
        "{stored_len} bytes stored for a {BODY_LEN}-byte body"
    );
    requests.recv().unwrap();
    server.join().unwrap();
}
```

- [x] **Step 2: Run the size test and verify RED**

Run:

```bash
cargo test --features http-cache --test http_cache_test large_cached_body_has_bounded_storage_overhead -- --exact
```

Expected: FAIL because the remaining inner JSON byte array is approximately
3.45 MiB, far above the 1 MiB plus 16 KiB bound.

- [x] **Step 3: Add v2 HTTP framing and metadata-only serialization**

At the top of `src/http/cache.rs`, add:

```rust
const HTTP_CACHE_MAGIC: &[u8; 8] = b"LBRHT02\0";
const HTTP_CACHE_FOOTER_LEN: usize = 16;
const HTTP_CACHE_FORMAT_VERSION: u8 = 2;
```

Mark the body as out-of-band metadata:

```rust
    #[serde(skip)]
    body: Vec<u8>,
```

Extend `CacheEntryError` with:

```rust
    #[error("invalid cached entry framing: {0}")]
    Format(&'static str),
    #[error("invalid cached entry metadata: {0}")]
    Json(#[from] serde_json::Error),
```

Add the decoder:

```rust
fn decode_entry(mut bytes: Vec<u8>) -> std::result::Result<CachedHttpEntry, CacheEntryError> {
    if bytes.len() < HTTP_CACHE_FOOTER_LEN {
        return Err(CacheEntryError::Format("truncated footer"));
    }

    let magic_start = bytes.len() - HTTP_CACHE_MAGIC.len();
    if &bytes[magic_start..] != HTTP_CACHE_MAGIC {
        return Err(CacheEntryError::Format("unsupported magic or version"));
    }
    let length_start = magic_start - std::mem::size_of::<u64>();
    let metadata_len = u64::from_be_bytes(
        bytes[length_start..magic_start]
            .try_into()
            .expect("metadata length occupies eight bytes"),
    );
    let metadata_len = usize::try_from(metadata_len)
        .map_err(|_| CacheEntryError::Format("metadata length exceeds platform limits"))?;
    let metadata_start = length_start
        .checked_sub(metadata_len)
        .ok_or(CacheEntryError::Format("metadata length exceeds payload"))?;

    let mut entry: CachedHttpEntry =
        serde_json::from_slice(&bytes[metadata_start..length_start])?;
    if entry.format_version != HTTP_CACHE_FORMAT_VERSION {
        return Err(CacheEntryError::UnsupportedFormat(entry.format_version));
    }
    bytes.truncate(metadata_start);
    entry.response.body = bytes;
    entry.response.validate()?;
    Ok(entry)
}
```

- [x] **Step 4: Decode v2 entries and isolate corruption from ordinary I/O**

In `load_entry`, replace the JSON/base64-specific cache-error arms with a
`CacheError::Format` arm that logs, removes, and returns a miss. Keep every
other cache error propagating:

```rust
    let bytes = match cache.get(&namespaced) {
        Ok(bytes) => bytes,
        Err(Error::Cache(CacheError::Format(error))) => {
            tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
            let _ = cache.remove(&namespaced);
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
```

Replace the `serde_json::from_slice` block with:

```rust
    match decode_entry(bytes) {
        Ok(entry) => Ok(Some(entry)),
        Err(error) => {
            tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
            let _ = cache.remove(&namespaced);
            Ok(None)
        }
    }
```

- [x] **Step 5: Persist multipart entries without cloning the body**

Change `persist_entry` to pass ownership and ignore the returned cached value:

```rust
    match CachedResponse::try_from(response) {
        Ok(cached) => {
            let _ = persist_cached_response(client, cache, key, policy, cached, now);
        }
        Err(error) => tracing::warn!(key, error = %error, "failed to encode HTTP cache response"),
    }
```

Replace `persist_cached_response` with:

```rust
fn persist_cached_response(
    client: &HttpClient,
    cache: &Cache,
    key: &str,
    policy: &CachePolicy,
    response: CachedResponse,
    now: SystemTime,
) -> CachedResponse {
    let entry = CachedHttpEntry {
        format_version: HTTP_CACHE_FORMAT_VERSION,
        policy: policy.clone(),
        response,
    };
    let metadata = match serde_json::to_vec(&entry) {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::warn!(key, error = %error, "failed to serialize HTTP cache entry");
            return entry.response;
        }
    };
    let metadata_len = (metadata.len() as u64).to_be_bytes();
    let ttl = policy
        .time_to_live(now)
        .saturating_add(client.config().http_cache_stale_retention);
    let parts: [&[u8]; 4] = [
        entry.response.body.as_slice(),
        metadata.as_slice(),
        &metadata_len,
        HTTP_CACHE_MAGIC,
    ];
    if let Err(error) = cache.set_parts(&namespaced_key(key), &parts, ttl) {
        tracing::warn!(key, error = %error, "failed to persist HTTP cache entry");
    }
    entry.response
}
```

In the 304 path, retain the returned value before serving it:

```rust
            let cached =
                persist_cached_response(client, cache, key, &policy, cached, response_time);
            fresh_response(cached, &calculated_parts.headers, CacheStatus::Revalidated)
                .map_err(corrupt_cache_error)
```

- [x] **Step 6: Bump every HTTP and file-format-sensitive test to v2**

Change `namespaced_key` and its unit test to `http:v2:`:

```rust
fn namespaced_key(caller_key: &str) -> String {
    format!("http:v2:{caller_key}")
}
```

```rust
#[test]
fn cache_keys_are_namespaced() {
    assert_eq!(namespaced_key("item"), "http:v2:item");
}
```

In `cache_write_failure_does_not_discard_network_response`, use:

```rust
    let internal_key = "http:v2:item";
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(internal_key);
    let entry_path = cache_dir.path().join(format!("v2-{encoded}.cache"));
```

In the cookie persistence test, read `http:v2:profile`.

- [x] **Step 7: Run focused HTTP cache verification**

Run:

```bash
cargo test --features http-cache --test http_cache_test
cargo test --features http-cache --lib http::http_cache::tests
cargo check --no-default-features --features http-cache
```

Expected: all HTTP-cache integration and unit tests pass, including the 1 MiB
size bound, and the isolated `http-cache` feature compiles without warnings.

- [x] **Step 8: Format and rerun both focused suites**

Run:

```bash
just fmt
cargo test --features cache --test cache_test
cargo test --features http-cache --test http_cache_test
```

Expected: formatting changes only intended Rust files; both focused suites
remain green.

---

### Task 3: Verify and prepare the remediation handoff

**Files:**
- Modify: `record/audits/2026-08-01-00-full-repo/actions-taken.md`
- Include: `record/superpowers/specs/2026-08-01-http-cache-binary-storage-design.md`
- Include: `record/superpowers/plans/2026-08-01-http-cache-binary-storage.md`
- Create: `commit.txt` (gitignored)

- [x] **Step 1: Run every repository gate**

Run:

```bash
just check
just feature-matrix
RUSTUP_TOOLCHAIN=1.89.0 just msrv-check
```

Expected: 229 nextest tests and 37 doctests pass, all 21 cargo-hack feature
configurations pass, and all targets compile on Rust 1.89.

- [x] **Step 2: Record the Cased action without staging the ledger**

Append a `fixed` entry for `http-cache-entry-body-amplification`, update the
front matter to `fixed: 15` and `open: 48`, and record the red/green size
evidence plus focused and full verification results. Keep the preceding
`7c259d7` landing entry intact.

- [x] **Step 3: Stage only implementation, tests, and design artifacts**

Run:

```bash
git --no-pager add src/cache.rs src/error.rs src/http/cache.rs tests/cache_test.rs tests/http_cache_test.rs record/superpowers/specs/2026-08-01-http-cache-binary-storage-design.md record/superpowers/plans/2026-08-01-http-cache-binary-storage.md
git --no-pager diff --cached --check
git --no-pager diff --cached --name-only
```

Expected: the seven listed files are staged. The audit ledger is not staged.

- [x] **Step 4: Write the `gtxt` commit message**

Create gitignored `commit.txt` with:

```text
fix(cache): store HTTP bodies as raw bytes

Replace nested JSON and base64 cache envelopes with versioned binary framing.
Write response bodies directly through atomic multipart cache writes and reuse
the owned response after persistence.

Release-Note: Store cached HTTP bodies without encoding amplification
Release-Impact: medium
```

- [x] **Step 5: Review the handoff**

Run:

```bash
git --no-pager diff --cached
git --no-pager status --short
git check-ignore -v commit.txt
```

Expected: only the implementation, tests, spec, and plan are staged; the audit
directory remains unstaged; `commit.txt` is ignored and ready for `gtxt`.
