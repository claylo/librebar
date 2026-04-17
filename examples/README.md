# librebar examples

Runnable scenarios that exercise librebar features in realistic combinations,
past the "compiles with this feature on" bar that the integration tests set.

Every example is a full `main()` you can read top-to-bottom.

## Index

| Example | Scenario | Features |
|---------|----------|----------|
| [`minimal`](minimal.rs) | Smallest idiomatic librebar app: flags, config, structured logs | `cli`, `config`, `logging` |
| [`service`](service.rs) | Long-running async service: shutdown token, crash dumps, optional OTEL export | `cli`, `config`, `logging`, `shutdown`, `crash`, `otel` |
| [`updater`](updater.rs) | GitHub releases check: real HTTPS call, 24h cache, `{APP}_NO_UPDATE_CHECK` gate | `cli`, `config`, `logging`, `http`, `cache`, `update` |
| [`plugin-cli`](plugin-cli/main.rs) | Git-style external subcommand dispatch with a paired plugin binary | `cli`, `config`, `logging`, `dispatch` |

## Running

Each example declares its `required-features` in `Cargo.toml`, so you always
pass the feature flags explicitly:

```sh
cargo run --example minimal --features "cli,config,logging" -- --help
cargo run --example service \
    --features "cli,config,logging,shutdown,crash,otel" -- --help
cargo run --example updater \
    --features "cli,config,logging,http,cache,update" -- --help
cargo run --example plugin-cli \
    --features "cli,config,logging,dispatch" -- --help
```

The `plugin-cli` example ships two binaries — the main CLI plus a paired
`plugin-cli-hello-greet` plugin. Build both at once and prepend the
examples directory to PATH so dispatch resolves:

```sh
cargo build --examples --features "cli,config,logging,dispatch"
PATH="$(pwd)/target/debug/examples:$PATH" \
    ./target/debug/examples/plugin-cli -C examples/plugin-cli hello-greet --name Clay
```

Config discovery walks up from the current directory, so the sample `.toml`
files work when you run from either the repo root or the `examples/` directory:

```sh
# From repo root — finds examples/minimal.toml via -C (change directory):
cargo run --example minimal --features "cli,config,logging" -- -C examples info

# From examples/ directly:
cd examples && cargo run --example minimal --features "cli,config,logging" -- info
```

## Verifying

The CI `lint` job runs `cargo clippy --all-targets --all-features`, which
catches compile breakage in every example without running them.

To verify locally:

```sh
cargo clippy --all-targets --all-features -- -D warnings
```
