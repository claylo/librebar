# Consuming DebugBundle Builder Design

**Finding:** `debug-bundle-builder-cannot-be-chained`

## Decision

Make every `DebugBundle::add_*` method consume and return `Self`, matching
`DebugBundle::finish(self)` and the crate's other fluent builders. Mark each
method `#[must_use]` so accidentally discarding the returned builder produces a
compiler warning.

The affected methods are:

- `add_text`
- `add_bytes`
- `add_sanitized_file`
- `add_doctor_results`

`with_redactor(self)` and `finish(self)` retain their existing ownership model.
No parallel `push_*` API will be added.

## Usage

The primary use case becomes a single fluent expression:

```rust
let path = DebugBundle::new("my-app", output_dir)
    .add_text("info.txt", "diagnostic output")
    .add_sanitized_file("logs/app.log", log_path)
    .finish()?;
```

Callers that add entries in a loop rebind the builder:

```rust
let mut bundle = DebugBundle::new("my-app", output_dir);
for (name, content) in entries {
    bundle = bundle.add_text(name, content);
}
let path = bundle.finish()?;
```

This is a deliberate source-breaking correction to the pre-1.0 public API.
Existing callers that ignored the returned `&mut Self` must retain and rebind
the returned `Self` instead.

## Data and Error Flow

Each method mutates its owned builder and returns it. Redaction still happens
when buffered entries are added, sanitized files remain file-backed until
`finish`, and archive creation and error propagation remain unchanged. The
ownership correction does not alter archive contents, permissions, retention,
or redaction behavior.

## Verification

First change an integration test to construct, populate, and finish a bundle in
one expression. It must fail to compile against the current `&mut Self` API with
the move-out-of-borrow error described by the audit. Then change the receivers
and update every in-tree call site, including the loop-based redaction test and
the `doctor-bundle` example.

Run the focused diagnostics tests, the diagnostics example build, the full
repository check, the 21-configuration feature matrix, and the Rust 1.89 MSRV
check. No new dependencies are required.
