# DebugBundle Chaining Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `DebugBundle` entry methods compose directly with the consuming `finish(self)` method.

**Architecture:** Convert every `add_*` receiver from `&mut self` to owned `self`, mutate the owned builder internally, and return `Self`. Preserve all redaction, streaming, archive, and error behavior while migrating in-tree callers to retain the returned builder.

**Tech Stack:** Rust, Cargo integration tests, existing `diagnostics` feature, Just, cargo-hack.

---

### Task 1: Prove the fluent chain is currently rejected

**Files:**
- Modify: `tests/diagnostics_test.rs`

- [x] **Step 1: Replace the basic archive test with the desired one-expression API**

```rust
#[test]
fn debug_bundle_can_be_finished_from_a_chain() {
    let tmp = TempDir::new().unwrap();
    let archive_path = DebugBundle::new("test-app", tmp.path())
        .add_text("info.txt", "test content")
        .finish()
        .unwrap();

    assert!(archive_path.exists());
    assert!(archive_path.to_string_lossy().ends_with(".tar.gz"));
}
```

- [x] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test --features diagnostics --test diagnostics_test debug_bundle_can_be_finished_from_a_chain -- --exact
```

Expected: compilation fails with `E0507`, reporting that `finish(self)` cannot
move a `DebugBundle` out of the mutable reference returned by `add_text`.

---

### Task 2: Adopt one consuming builder model

**Files:**
- Modify: `src/diagnostics.rs`
- Modify: `tests/diagnostics_test.rs`
- Modify: `examples/doctor-bundle.rs`

- [x] **Step 1: Convert every entry method to owned `Self`**

Replace the four methods in `DebugBundle` with:

```rust
    /// Add a text file to the bundle after redaction.
    #[must_use]
    pub fn add_text(self, name: &str, content: &str) -> Self {
        self.add_bytes(name, content.as_bytes())
    }

    /// Add binary content to the bundle after redaction.
    ///
    /// Passing an owned [`Vec<u8>`] moves the caller's buffer into this method
    /// instead of cloning it at the API boundary.
    #[must_use]
    pub fn add_bytes(mut self, name: &str, data: impl Into<Vec<u8>>) -> Self {
        let data = data.into();
        let data = self.redactor.redact(name, &data);
        self.entries.push(DebugBundleEntry::Buffered {
            name: name.to_string(),
            data,
        });
        self
    }

    /// Add a file that the caller has already sanitized.
    ///
    /// The file path is retained and its content is streamed into the archive
    /// by [`Self::finish`], so the source must remain available and sanitized
    /// until then. This method deliberately does not run the configured
    /// [`Redactor`]; use [`Self::add_text`] or [`Self::add_bytes`] for content
    /// that has not already crossed a trusted redaction boundary.
    #[must_use]
    pub fn add_sanitized_file(mut self, name: &str, path: &Path) -> Self {
        self.entries.push(DebugBundleEntry::SanitizedFile {
            name: name.to_string(),
            path: path.to_path_buf(),
        });
        self
    }

    /// Add doctor results to the bundle.
    #[must_use]
    pub fn add_doctor_results(self, results: &[NamedResult]) -> Self {
        let report = DoctorRunner::format_report(results);
        self.add_text("doctor-report.txt", &report)
    }
```

- [x] **Step 2: Migrate the streaming test while preserving deferred reads**

```rust
    let bundle = DebugBundle::new("test-app", tmp.path())
        .add_sanitized_file("logs/application.log", &source);

    std::fs::write(&source, "content read at finish").unwrap();
    let archive_path = bundle.finish().unwrap();
```

- [x] **Step 3: Rebind the builder in the loop-based redaction test**

```rust
    let mut bundle = DebugBundle::new("test-app", tmp.path());
    // Keep the existing cases array unchanged.
    for (name, content, _, _) in cases {
        bundle = bundle.add_text(name, content);
    }
```

- [x] **Step 4: Convert the remaining tests to retain the returned builder**

Use fluent expressions for the custom-redactor and permission tests:

```rust
    let bundle = DebugBundle::new("test-app", tmp.path())
        .with_redactor(FixedRedactor)
        .add_bytes("opaque.bin", b"private bytes".to_vec());
```

```rust
    let bundle = DebugBundle::new("test-app", tmp.path())
        .add_text("info.txt", "safe content");
```

Apply the second form to both owner-only tests before calling `finish()`.

- [x] **Step 5: Make the example demonstrate the fluent contract**

Replace the mutable builder block in `run_bundle` with:

```rust
    let path = DebugBundle::new(app.app_name(), &dir)
        .add_doctor_results(&results)
        .add_text("config-sources.json", &sources_json)
        .finish()?;
```

- [x] **Step 6: Run focused tests and example compilation to verify GREEN**

Run:

```bash
cargo test --features diagnostics --test diagnostics_test
cargo check --all-features --example doctor-bundle
```

Expected: all diagnostics integration tests pass and the example compiles
without warnings.

- [x] **Step 7: Format and rerun the focused test**

Run:

```bash
just fmt
cargo test --features diagnostics --test diagnostics_test
```

Expected: formatting makes no unrelated changes and the focused suite remains
green.

---

### Task 3: Verify and prepare the remediation handoff

**Files:**
- Modify: `record/audits/2026-08-01-00-full-repo/actions-taken.md`
- Include: `record/superpowers/specs/2026-08-01-debug-bundle-chaining-design.md`
- Include: `record/superpowers/plans/2026-08-01-debug-bundle-chaining.md`
- Create: `commit.txt` (gitignored)

- [x] **Step 1: Run all repository gates**

Run:

```bash
just check
just feature-matrix
RUSTUP_TOOLCHAIN=1.89.0 just msrv-check
```

Expected: 226 nextest tests and 37 doctests pass, all 21 cargo-hack feature
configurations pass, and all targets compile on Rust 1.89.

- [x] **Step 2: Record the Cased action without staging the ledger**

Append a `fixed` entry for
`debug-bundle-builder-cannot-be-chained`, update the front matter to `fixed: 14`
and `open: 49`, and record the focused and full verification results. Keep the
preceding `8172cc5` landing entry intact.

- [x] **Step 3: Stage only the related implementation and design artifacts**

Run:

```bash
git --no-pager add src/diagnostics.rs tests/diagnostics_test.rs examples/doctor-bundle.rs record/superpowers/specs/2026-08-01-debug-bundle-chaining-design.md record/superpowers/plans/2026-08-01-debug-bundle-chaining.md
git --no-pager diff --cached --check
git --no-pager diff --cached --name-only
```

Expected: the five listed files are staged, while
`record/audits/2026-08-01-00-full-repo/actions-taken.md` is not staged.

- [x] **Step 4: Write the `gtxt` commit message**

Create gitignored `commit.txt` with:

```text
fix(diagnostics): make debug bundles chainable

Use one consuming ownership model across debug-bundle entry methods and
finish. Update in-tree callers to retain the returned builder and exercise
the complete fluent expression.

Release-Note: Make debug bundle builders fully chainable
Release-Impact: high
```

- [x] **Step 5: Review the handoff**

Run:

```bash
git --no-pager diff --cached
git --no-pager status --short
```

Expected: only the implementation, example, test, design, and plan are staged;
the audit directory remains unstaged; `commit.txt` is ignored and ready for
`gtxt`.
