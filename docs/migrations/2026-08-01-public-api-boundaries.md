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

## Update environment config sources

Custom `EnvironmentSource` implementations now receive Librebar's normalized
application prefix and may fail while querying their backing store. They no
longer need to implement `Debug`:

```rust
use std::ffi::OsString;

use librebar::config::EnvironmentSource;
use librebar::error::BoxError;

struct ParameterStore;

impl EnvironmentSource for ParameterStore {
    fn vars(
        &self,
        prefix: &str,
    ) -> Result<Vec<(OsString, OsString)>, BoxError> {
        let _ = prefix; // For example: MY_APP_
        Ok(Vec::new())
    }
}
```

Use the prefix to avoid querying or returning unrelated values. Return the
backing error directly; Librebar wraps it as `Error::ConfigEnvironmentSource`
and preserves it through `std::error::Error::source()`.

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

Error displays no longer append their source's display text. Context-bearing
variants render one concise message, while the top-level HTTP and cache
adapters delegate to their module error. This prevents chain-aware reporters
from repeating the same cause at adjacent levels. Update snapshots or string
comparisons that expected the old combined messages, and walk the source chain
when reporting every cause:

```rust
fn report(error: &(dyn std::error::Error + 'static)) {
    eprintln!("{error}");
    let mut source = error.source();
    while let Some(cause) = source {
        eprintln!("caused by: {cause}");
        source = cause.source();
    }
}
```

Final deserialization of layered configuration now reports
`Error::ConfigValue { path, origin, source }`. The path identifies the failing
Serde field, `origin` names the winning default, file, environment variable, or
programmatic override, and `source` contains the concrete deserializer error.
Successful loads expose the same provenance through
`ConfigSources::origin("database.pool_size")`.

## Distinguish lock contention from lock failures

`Lockfile::try_acquire` now returns `Error::LockContended { path }` only when
another process holds the lock. Match that variant when the application can
skip, retry, or report an already-running instance:

```rust
# fn acquire(lock: &librebar::lockfile::Lockfile) -> librebar::Result<()> {
match lock.try_acquire() {
    Ok(_guard) => Ok(()),
    Err(librebar::Error::LockContended { path }) => {
        eprintln!("already running: {}", path.display());
        Ok(())
    }
    Err(error) => Err(error),
}
# }
```

`Error::Lock` now means the operating system rejected the lock operation for
another reason. Its nested `std::io::Error` preserves the original kind and
message through `std::error::Error::source()`.

## Handle fallible default lock directories

`lockfile::default_lock_dir` now returns `librebar::Result<PathBuf>`. Direct
callers must handle the possibility that no secure per-user lock directory is
available:

```rust
# fn lock_dir() -> librebar::Result<std::path::PathBuf> {
let dir = librebar::lockfile::default_lock_dir("my-app")?;
# Ok(dir)
# }
```

On Linux, the resolver uses `XDG_RUNTIME_DIR`, then `XDG_STATE_HOME` or
`~/.local/state`. It no longer falls back to shared `/tmp`, where another local
user could pre-create or hold the application lock path. `Lockfile::default_for`
already propagates this error, so callers using that constructor need no code
change.

## Replace growable struct literals

Librebar's growable public records are now `#[non_exhaustive]`. Read and mutate
their public fields after construction, but create them through their supported
constructor or builder:

```rust
use librebar::diagnostics::{CheckResult, CheckStatus};
use librebar::http::HttpClientConfig;
use librebar::update::UpdateInfo;

let mut http = HttpClientConfig::new("myapp", "0.4.0");
http.max_redirects = 5;

let check = CheckResult::new(CheckStatus::Ok, "configuration loaded");
let update = UpdateInfo::new("0.4.0", "0.5.0", "https://example.com/releases/0.5.0");
# let _ = (http, check, update);
```

Create crash records with `CrashInfo::new`; use its `with_location`,
`with_timestamp`, `with_os`, and `with_backtrace` methods when importing or
testing a known record. Use `ConfigSources::default`,
`LoggingConfig::from_app_name`, and `OtelConfig::from_app_name` for those
records. CLI schema output records come from `schema_for`; application-supplied
schema metadata keeps its existing constructors and builders.

`HttpClientConfig::http_cache_stale_retention` now exists in every `http`
build, so Cargo feature unification no longer changes the struct's shape. The
setting affects requests only when the `http-cache` feature is enabled.

## Register doctor checks without boxing

Pass concrete checks directly to `DoctorRunner::add`; the runner now owns its
heterogeneous storage detail:

```rust
# use librebar::diagnostics::{CheckResult, CheckStatus, DoctorCheck, DoctorRunner};
# struct ConfigCheck;
# impl DoctorCheck for ConfigCheck {
#     fn name(&self) -> &str { "config" }
#     fn category(&self) -> &str { "configuration" }
#     fn run(&self) -> CheckResult {
#         CheckResult::new(CheckStatus::Ok, "Config valid")
#     }
# }
let mut runner = DoctorRunner::new();
runner.add(ConfigCheck);
```

Remove the old `Box::new(...)` wrapper. `DoctorCheck` also no longer requires
`Send`, so sequential checks may hold thread-local state such as `Rc`.

## Read and compare CLI schema documents

The CLI schema document tree now implements `Deserialize`, `PartialEq`, and
`Eq`. Tools can deserialize a committed schema into `SchemaDocument` and
compare it directly with the current output.

The borrowed wire fields are now owned strings so deserialized documents own
their JSON data. Update explicit field type annotations as follows:

```rust
// Before
let version: &'static str = document.clispec;
let hint: Option<&'static str> = argument.value_hint;

// After
let version: &str = &document.clispec;
let hint: Option<&str> = argument.value_hint.as_deref();
```

The same `String` change applies to `OutputBehavior::tty` and
`OutputBehavior::piped`. Normal string comparisons and field access remain
unchanged.

`ParseOutcome::Schema` now contains `Box<SchemaDocument>` to keep the generic
parse result compact. Pattern matching and field access continue to dereference
automatically; use `*document` when ownership of the document itself is needed.

## Update release checks

Replace the old `UpdateChecker::new(app, version, "owner/repo")` call with the
GitHub shortcut. The constructor now returns a `Result`, and `check()` reports
source failures instead of folding them into `None`:

```rust
# async fn check() -> Result<(), Box<dyn std::error::Error>> {
use librebar::update::UpdateChecker;

let checker = UpdateChecker::github("myapp", "0.4.0", "owner/repo")?;
if let Some(update) = checker.check().await? {
    eprintln!("{}", update.message());
}
# Ok(())
# }
```

For another forge, registry, or release service, implement `ReleaseSource` and
pass it to `UpdateChecker::new`. The source owns its transport and returns a
complete `ReleaseInfo` containing the latest version and release URL. Use
`with_cache` to inject a cache or `without_cache` to disable caching.

GitHub callers that need authentication can construct `GitHubReleaseSource`
with an `HttpClient`, add `with_bearer_token`, and pass that source to
`UpdateChecker::new`.

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
