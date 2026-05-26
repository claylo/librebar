set shell := ["bash", "-c"]
set dotenv-load := true
toolchain := `taplo get -f rust-toolchain.toml toolchain.channel | tr -d '"'`
msrv := "1.89.0"

default:
  @just --list

fmt:
  cargo fmt --all -- --config-path .config/rustfmt.toml

clippy:
  cargo +{{toolchain}} clippy --all-targets --all-features --message-format=short -- -D warnings

fix:
  echo "Using toolchain {{toolchain}}"
  cargo +{{toolchain}} clippy --fix --allow-dirty --allow-staged -- -W clippy::all

# Check dependencies for security advisories and license compliance.
# `--all-features` walks the full dep tree so optional features (hyper-rustls,
# opentelemetry, etc.) are covered — matches the CI invocation.
deny:
  cargo deny --all-features check --config .config/deny.toml

test:
  cargo nextest run --workspace --all-features

test-ci:
  cargo nextest run --workspace --all-features --profile ci

# Doc-tests run with --all-features so feature-gated modules (mcp, dispatch,
# diagnostics, otel, http, etc.) actually compile their examples. Without
# this flag, doc blocks inside `#[cfg(feature = "…")]` modules are skipped
# entirely and can rot unnoticed.
doc-test:
  cargo test --doc --all-features

cov:
  @cargo llvm-cov clean --workspace
  cargo llvm-cov nextest --no-report
  @cargo llvm-cov report --html
  @cargo llvm-cov report --summary-only --json --output-path target/llvm-cov/summary.json

check: fmt clippy deny test doc-test

# Check for outdated dependencies (root only, no transitive noise)
outdated:
    cargo outdated --workspace --root-deps-only

# Safe update: respects semver constraints, only touches Cargo.lock
update:
    cargo update --workspace --verbose

# Upgrade Cargo.toml to latest compatible versions
upgrade:
    cargo upgrade
    cargo update --workspace

# The nuclear option: upgrade to latest incompatible versions (breaking changes)
upgrade-breaking:
    cargo upgrade --incompatible
    cargo update --workspace

# See what WOULD update without doing it
check-updates:
    cargo update --workspace --dry-run

