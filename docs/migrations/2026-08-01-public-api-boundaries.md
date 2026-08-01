# Update dependency imports

Import shared types through the Librebar module that uses them:

```rust
use librebar::cli::clap::{self, Parser};
use librebar::config::serde_json::{self, json};
use librebar::http::{HeaderName, Uri, header};
use librebar::logging::tracing_subscriber;
use librebar::mcp::rmcp;
```

These paths use the same crate versions as Librebar. Keep a direct dependency
when your app also uses that crate on its own.

## Update MCP stdio setup

Pass the stdio value straight to RMCP:

```rust
use librebar::mcp::{ServiceExt as _, transport_stdio};

# async fn serve<T>(server: T) -> Result<(), Box<dyn std::error::Error>>
# where
#     T: librebar::mcp::rmcp::ServerHandler,
# {
let service = server.serve(transport_stdio()).await?;
service.waiting().await?;
# Ok(())
# }
```

Do not split the return value into stdin and stdout. Do not give it the old
`(tokio::io::Stdin, tokio::io::Stdout)` tuple type. Tokio still powers the
transport, but those I/O types are now private.

## Update HTTP imports

Import HTTP types from Librebar instead of Hyper:

```rust
use librebar::http::{AsHeaderName, HeaderMap, HeaderName, Request, Uri, header};
```

The types still come from the `http` and `bytes` crates. Request code does not
change. Hyper now stays behind the client API.

## Update error payload handling

Pattern matching still uses the same variant names. Dependency-backed tuple
variants now contain `librebar::error::BoxError`:

```rust
fn report(error: &librebar::Error) {
    if let librebar::Error::ConfigDeserialize(source) = error {
        eprintln!("config error: {source}");

        if let Some(json) =
            source.downcast_ref::<librebar::config::serde_json::Error>()
        {
            eprintln!("line: {}", json.line());
        }
    }
}
```

Code that constructs one of these variants must box the concrete error:

```rust
# fn wrap(json_error: librebar::config::serde_json::Error) -> librebar::Error {
librebar::Error::ConfigDeserialize(Box::new(json_error))
# }
```

Every wrapped error is also available through `std::error::Error::source()`.
Walking that chain reaches nested sources in the same order they were reported
by the dependency.

## Remove redundant dependencies

Run your normal checks, then look for direct dependencies that are no longer
used:

```sh
cargo machete
cargo test --all-features
```

Remove `clap`, `serde_json`, `tracing-subscriber`, `http`, `bytes`, or `rmcp`
only when all uses now go through Librebar. Keep `tokio` when your app owns its
runtime or uses Tokio for work beyond Librebar's MCP stdio helper.
