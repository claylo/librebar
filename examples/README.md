# librebar examples

Runnable scenarios that exercise librebar features in realistic combinations,
past the "compiles with this feature on" bar that the integration tests set.

Every example is a full `main()` you can read top-to-bottom.

## Index

| Example | Scenario | Additional features |
|---------|----------|---------------------|
| [`minimal`](minimal.rs) | Smallest idiomatic librebar app: flags, config, structured logs | None |
| [`service`](service.rs) | Long-running async service: shutdown token, crash dumps, optional OTEL export | `shutdown`, `otel` |
| [`updater`](updater.rs) | GitHub releases check: real HTTPS call, 24h cache, `{APP}_NO_UPDATE_CHECK` gate | `update` |
| [`http-cookies`](http-cookies.rs) | Stateful Hyper client: redirect-time cookie capture and persistent jar | `http-cookies` |
| [`plugin-cli`](plugin-cli/main.rs) | Git-style external subcommand dispatch with a paired plugin binary | `dispatch` |
| [`doctor-bundle`](doctor-bundle.rs) | Health checks with `DoctorRunner` + `DebugBundle` tar.gz for bug reports | None |
| [`mcp-server`](mcp-server.rs) | Minimal MCP server over stdio — single `greet` tool, manual `ServerHandler` impl | `mcp` |

## Running

The default feature set provides `cli`, `config`, `logging`, `crash`, `cache`,
and `diagnostics`. Each example still declares its complete
`required-features` contract in `Cargo.toml`; commands only opt into features
outside that foundation:

```sh
cargo run --example minimal -- --help
cargo run --example service --features "shutdown,otel" -- --help
cargo run --example updater --features "update" -- --help
cargo run --example http-cookies --features "http-cookies" -- /tmp/librebar-cookies.json
cargo run --example plugin-cli --features "dispatch" -- --help
cargo run --example doctor-bundle -- --help
cargo run --example mcp-server --features "mcp" -- --help
```

Every librebar-powered example uses `librebar::cli::parse`, so it also exposes
the generated `schema` and `completions` subcommands before normal startup:

```sh
cargo run --example minimal -- schema
cargo run --example minimal -- completions zsh > _minimal
```

The `plugin-cli` example ships two binaries — the main CLI plus a paired
`plugin-cli-hello-greet` plugin. Build both at once and prepend the
examples directory to PATH so dispatch resolves:

```sh
cargo build --examples --features "dispatch"
PATH="$(pwd)/target/debug/examples:$PATH" \
    ./target/debug/examples/plugin-cli -C examples/plugin-cli hello-greet --name Clay
```

Config discovery walks up from the current directory, so the sample `.toml`
files work when you run from either the repo root or the `examples/` directory:

```sh
# From repo root — finds examples/minimal.toml via -C (change directory):
cargo run --example minimal -- -C examples info

# From examples/ directly:
cd examples && cargo run --example minimal -- info
```

## Verifying

The CI `lint` job runs `cargo clippy --all-targets --all-features`, which
catches compile breakage in every example without running them.

To verify locally:

```sh
cargo clippy --all-targets --all-features -- -D warnings
```
