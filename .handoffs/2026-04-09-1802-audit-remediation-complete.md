# Handoff: Audit remediation complete, spec implementation done

**Date:** 2026-04-09
**Branch:** main
**State:** Green

> Green = tests pass, safe to continue.

## Where things stand

All 13 findings from the April 9 cased + crustoleum audit are dispositioned (12 fixed, 1 accepted). The full spec implementation across three phases (14 features) is complete and audit-clean. Zero clippy warnings, zero `Box<dyn Error>` in error variants, clean compile across every individual feature in isolation. The crate is at 0.1.0, ready for downstream consumers.

## Decisions made

- **Concrete error types everywhere.** Every error variant is feature-gated and carries its concrete source type. Three per-module enums (`HttpError`, `CacheError`, `ConfigParseError`) handle multi-source variants. `Box<dyn Error>` eliminated entirely from the error enum.
- **BuilderInner + macro deduplication.** Extracted shared builder fields into `BuilderInner`, used `builder_methods!` macro for the 7 shared methods, and `SubsystemInit` struct for the shared `start()` logic. ~140 lines removed.
- **`ConfigMergeDepth` as its own variant.** The deep_merge depth-limit error was previously shoehorned into `ConfigDeserialize` via `String.into()`. Split it out as a proper unit variant since it's a different failure mode.
- **Accepted `span-fields-clone`.** The `fields.values.clone()` in the logging layer is structurally required by the tracing extensions API. No profiling data to justify the Arc refactor.
- **`Response::json<T>()` uses `from_slice`.** Deserializes directly from `&[u8]`, avoiding the UTF-8 validation step that `from_str` requires.

## What's next

1. **Publish.** No remote is set up yet. When ready, push to GitHub and publish to crates.io.
2. **Downstream integration.** Build something on top of rebar to validate the API ergonomics, especially the error types and builder pattern.
3. **Pre-1.0 review.** The `actions-taken.md` ledger (`record/audits/2026-04-09-full-crate/actions-taken.md`) is the living record. All findings closed.

## Landmines

- **Error enum is feature-gated.** Every variant except `Io` has a `#[cfg(feature = "...")]` attribute. Downstream code that matches on `Error` variants must have the corresponding feature enabled. This is correct behavior but will surprise anyone writing a catch-all match arm.
- **`ConfigParse.source` is `Box<ConfigParseError>`.** Boxed to satisfy clippy `result_large_err` (the enum was 128 bytes inlined). Callers matching on parse errors need to dereference through the Box.
- **`DebugBundle::add_text`/`add_bytes` changed return type.** Was `Result<()>`, now `&mut Self`. Any downstream callers using `?` on these will break. No known consumers at 0.1.0.
- **`deep_merge` now returns `Result<()>`.** Was `()`. All internal call sites updated, but any external callers need to add `?`.
- **Two commits on the remediation branch.** `c39db46` has the first pass (10 fixed), `d0daa2e` has the error-type-erasure completion. Both are on main now via fast-forward merge.
