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
