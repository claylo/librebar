# Migrating a claylo-rs repo to librebar

Written after doing it to bito, not before. Every trap below cost real time.

The companion is [bito-case-study.md](bito-case-study.md) — the numbers, the
diffs, and the bugs that turned up on the way.

## Is this your repo?

Five markers. Four of the five means you're in the right document.

- `.repo.yml` at the root — the copier answers file
- `crates/{app}` and `crates/{app}-core`
- `crates/{app}-core/src/observability.rs`, 500-660 lines
- a figment-based `ConfigLoader` in the core crate
- hand-rolled global clap flags on the binary's CLI struct

Confirm the observability fork in one shot:

```bash
grep -c 'days_to_ymd\|LOG_FILE_SUFFIX' crates/*/src/observability.rs
```

Six each means an unmodified copy of the template. That file is also where the
cwd log fallback lives — see the traps.

## Split the features by process state

librebar goes in **both** crates, divided by what owns global process state.

```toml
# crates/{app}-core/Cargo.toml
librebar = { version = "0.6", default-features = false, features = ["config"] }

# crates/{app}/Cargo.toml
librebar = { version = "0.6", features = [
    "cli", "config", "logging", "crash", "diagnostics", "update",
] }
```

`config` returns a value and touches no globals. That's domain logic, and it
belongs to the core. `cli`, `logging`, `crash`, `diagnostics`, and `update`
each install a process-wide singleton, so they belong to the binary. Cargo
unifies features per build graph, so the split costs nothing.

librebar's own graph already encodes the rule: `diagnostics = ["config",
"logging"]`.

## What comes out

| Delete | Recognize it by | Replaced by |
|---|---|---|
| `{app}-core/src/observability.rs` | `days_to_ymd`, `LOG_FILE_SUFFIX` | `librebar::logging` |
| the figment `ConfigLoader` | `Figment::new().merge(Serialized::defaults(...))` | `librebar::config::ConfigLoader` |
| `ColorChoice` + global flags | six `#[arg(global = true)]` on the root struct | `librebar::cli::CommonArgs` |
| bespoke doctor report structs | hand-rolled text renderer | `librebar::diagnostics::DoctorRunner` |

Config **types** stay. `Config`, `Dialect`, the per-check rule structs — the
core stays authoritative over config semantics. Only the loading mechanism
changes.

## Order

1. Independent live bugs first, so they stay separately revertible
2. Dependencies
3. Config
4. Logging **and CLI together** — see the log-level trap
5. xtask artifacts
6. Doctor
7. Update checks
8. Docs

## Traps

### Nested tokio runtime

`main` annotated `#[tokio::main]` while a subcommand builds its own runtime and
calls `block_on`. That command panics 100% of the time.

```bash
grep -n 'tokio::main' -r src/ && grep -rn 'Runtime::new' src/
```

Both hits together is the bug. It had shipped to bito's `main` unnoticed
because no test invoked `serve`.

### The cwd log fallback

Template code resolves log directories as `/var/log` → platform →
`current_dir()`. That last one litters the user's repo whenever `HOME` is
unset. Invisible on a developer's Mac; bites in CI and containers.
`.gitignore` templates cover `*.log`, never `*.jsonl`.

### `--format auto` changes what pipes carry

Adopting `CommonArgs` flips redirected output from text to JSON. Real break for
existing pipelines. Changelog it.

### Making JSON the default finds every place JSON was wrong

`--format auto` promotes JSON from opt-in to the default for any redirected
stream, so every `if !json { println!(...) }` with no `else` becomes a silently
empty pipe. Grep that shape **before** flipping the default. Two of bito's
three "flag migration" test failures were actually this.

### Check whether the JSON path enforces thresholds at all

The worse version. Every one of bito's six gating commands wrote `if json {
print; return Ok(()) }` above its threshold check, or buried the check in an
`else if` the JSON branch skipped. `--json` printed a report saying the input
failed and exited 0.

Nobody noticed while JSON was opt-in. Flipping the default silently disabled
every CI gate that redirects. Measured before the fix: `readability --max-grade
5` exited 1 as text, **0** as JSON.

The threshold belongs after the format branch, not inside it.

### Your findings exit code probably collides with clap's

clap exits `2` for every usage error and won't let you change it. Pick `2` for
"issues found" and exit 2 now means either "your prose needs work" or "your
command line is malformed."

`validate_metadata` will not catch this — it only compares codes you declare.
Either pick a code clap doesn't use, or do what bito did and let `2` mean
"could not run," which is exactly what a usage error is. That also matches
eslint, ruff, and shellcheck.

### Threshold failures are not errors

A checker reporting "input missed the threshold" through the same exit code as
"I could not run" is unusable by an agent, which can't tell acting on output
from retrying against a broken tool. `OutcomeMetadata` vs `ErrorMetadata` is
the fix.

### Config-to-librebar wiring is stringly typed

The builder reads `log_dir`, `log_level`, and `log_retention_days` off your
config via `serde_json::to_value` and a string key lookup. Rename one and it
compiles clean and silently stops working. Pin the names in a test that
serializes `Config::default()`.

### An `Option<T>` config field has no type to give

Related, and worse before librebar 0.6. The environment overlay typed values
against the serialized defaults, so an `Option<u16>` that no file had set
serialized to `null` and carried no type — the value arrived as a string and
the process exited 2 before running. Every numeric field at once.

0.6 recovers the type from the config type itself. On older librebar the only
workaround is a discovered file naming the field.

Which leads to the nastiest version of this:

### A repo's own config file can hide the bug from its own tests

Config discovery walks up from the working directory. Integration tests inherit
cargo's. So `.config/{app}.yaml` in your repo root supplies types to every test
that runs — including the types whose absence *is* the bug.

Two of bito's three regression tests for the above passed against the broken
librebar. Run environment tests under `-C <tempdir>`.

### `.failure()` is not an exit-code assertion

`assert_cmd`'s `.failure()` passes for any non-zero code. A suite described as
the compatibility gate can be full of them and pin nothing. Use `.code(n)`.

### A regression test that passes before the fix is not a test

Run every new test against the old implementation and read the failure message.

bito's "the live log has a stable name" test asserted `bito.jsonl.exists()` and
passed against the broken code, because the old writability probe opened that
exact path and left an empty stub while the real log went to `bito.jsonl.<date>`
beside it. Assert the property, not a filename: non-empty, and the only file
present.

### Measure a feature's binary cost only after something calls it

Enabling `update` but not wiring `UpdateChecker` grew the stripped binary
73 KB, because the linker dead-strips hyper and rustls when nothing reaches
them. Wired, the same feature costs 3.85 MiB. Off by 53×.

Build the caller first, then measure.

### A network call inside a `DoctorCheck` puts your test suite online

`run()` is synchronous and update checks are not, so the notice has to resolve
elsewhere anyway. Resolve it in `main` and pass it in. Unit tests stay offline,
and the set of commands that touch the network becomes one reviewable list
instead of a property you grep for.

### A notice printed after a JSON document breaks it

`doctor --bundle` announced `wrote <path>` on stdout. Fine in text mode, fatal
to the JSON. Anything a command reports about its own actions goes in the
document or to stderr.

### A "consumed contract" nobody tests is a contract nobody has

The plan named `doctor --json` untouchable and no test covered its field names,
so any rename during the port would have shipped. Pin the shape before
refactoring behind it.

### Dependency removals belong to the task that deletes the last import

The original bito plan dropped figment and the tracing stack in the "add
librebar" commit, three tasks before the code using them was replaced. The
build gate in that same task would have failed. Add first; remove where the
imports die.

### Don't swap one leaked error type for another

A core crate exposing `Result<_, figment::Error>` has figment's version in its
public API. Changing it to `Result<_, librebar::Error>` moves the coupling
without removing it. Keep your own error enum and store the foreign error as
`Box<dyn Error + Send + Sync + 'static>` — librebar's `error.rs` does exactly
this and documents why.

### `#[non_exhaustive]` on error enums, before the first release

librebar marks all of its own. Template-derived crates usually mark none, so
every added variant is a breaking change. Adding it during a migration that's
already breaking is free.

### `CommonArgs` owns flags you might already have

Grep the consuming struct before flattening: `--version-only`, `-C`, `-c`,
`-q`, `-v`, `--color`, `--format`, `--json`. clap reports duplicate argument
names by **panicking at startup**, not by failing to compile. `cargo build`
stays green and every invocation dies.

### Adopt logging and CLI in the same commit

Before 0.6 the log level reached the filter only through config `log_level` or
a `CommonArgs` passed to `with_cli`, and `CommonArgs` can't be built by hand —
one field is crate-private. Adopting logging first killed `-q`/`-v` in between,
and bito bridged the gap by parsing a synthetic argv.

0.6 adds `Builder::with_log_level`. On older librebar, do both at once.

### Help output

Two fixes landed in 0.6 that you'd otherwise hit:

`cli::parse` never applied `with_help_short`, so an app whose `-h` and `--help`
were both compact couldn't keep that. Worse, `with_help_short` added an
argument named `help` beside clap's own and panicked at startup unless you also
set `disable_help_flag` — a precondition nothing documented. It now owns that
setting, and `parse` applies it by default.

`generate_manpages` walked clap's built-in `help` subcommand, which is not
hidden, producing a page per command explaining how help explains that command.
bito generated 35 pages where 20 were `bito-help-*.1`, and its release workflow
copies the man directory wholesale. Fixed in 0.6.

## The regression net

Existing integration tests should pass **unmodified** through refactor-only
tasks, and get updated — plus changelogged — where a task changes behavior
deliberately.

The value is catching the accidental break in a ten-task migration, not
defending a contract. Judge how strict to be by what you're in: a
widely-depended-on library warrants the strict reading, a single-user tool
doesn't.
