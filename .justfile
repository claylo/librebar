set shell := ["bash", "-c"]
set dotenv-load := true

default:
  @just --list

fmt:
  cargo fmt --all -- --config-path .config/rustfmt.toml

fmt-check:
  cargo fmt --all --check -- --config-path .config/rustfmt.toml

clippy:
  cargo clippy --all-targets --all-features --message-format=short -- -D warnings

fix:
  cargo clippy --fix --allow-dirty --allow-staged -- -W clippy::all

# Check dependencies for security advisories and license compliance.
# `--all-features` walks the full dep tree so optional features (hyper-rustls,
# opentelemetry, etc.) are covered — matches the CI invocation.
#
# NOTE: cargo-deny 0.20 moved graph-shaping flags (`--config`, `--all-features`,
# `--workspace`) to global options, ahead of the subcommand. Only report-shaping
# flags (`--deny`, `--warn`, `--allow`) remain on `check`.
deny:
  cargo deny --all-features --config .config/deny.toml check

test:
  cargo nextest run --workspace --all-features

test-ci:
  cargo nextest run --workspace --all-features --profile ci

bench:
  cargo bench --bench cache --features bench,cache

# Doc-tests run with --all-features so feature-gated modules (mcp, dispatch,
# diagnostics, otel, http, etc.) actually compile their examples. Without
# this flag, doc blocks inside `#[cfg(feature = "…")]` modules are skipped
# entirely and can rot unnoticed.
doc-test:
  cargo test --doc --all-features

doc:
  cargo doc --all-features --no-deps

# Compile the default, empty, all-features, and every individual feature set.
# --no-dev-deps prevents dev-dependency feature unification from hiding a
# missing optional dependency edge in a published configuration.
feature-matrix:
  cargo hack check --each-feature --no-dev-deps

msrv-check:
  cargo check --all-targets --all-features

cov:
  @cargo llvm-cov clean --workspace
  cargo llvm-cov nextest --no-report
  @cargo llvm-cov report --html
  @cargo llvm-cov report --summary-only --json --output-path target/llvm-cov/summary.json

# check reformats (fmt, not fmt-check) — local muscle memory expects
# `just check` to FIX formatting, not report it. CI runs fmt-check itself.
check: fmt clippy deny test doc-test doc

# Check for outdated dependencies (root only, no transitive noise)
outdated:
    cargo outdated --workspace --root-deps-only

# Safe update: respects semver constraints, only touches Cargo.lock
#
# NOTE: no `--workspace` here. In `cargo update`, `--workspace` is shorthand
# for `-p <each workspace member>` — it re-resolves only the workspace's own
# packages, which is a no-op for a single-crate workspace. Bare `cargo update`
# is what actually walks the dependency tree.
update:
    cargo update --verbose

# Upgrade Cargo.toml to latest compatible versions
upgrade:
    cargo upgrade
    cargo update

# The nuclear option: upgrade to latest incompatible versions (breaking changes)
upgrade-breaking:
    cargo upgrade --incompatible
    cargo update

# See what WOULD update without doing it
check-updates:
    cargo update --dry-run
