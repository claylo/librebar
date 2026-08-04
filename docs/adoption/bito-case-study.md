# Case study: bito

The first claylo-rs repo moved onto librebar, 2026-08-03 through 2026-08-04.
17 commits. The playbook is [claylo-rs-to-librebar.md](claylo-rs-to-librebar.md).

## What it cost

```
49 files changed, 5,789 insertions(+), 1,503 deletions(-)
```

That headline number is mostly plan and Cargo.lock. The code:

| Scope | Files | Lines |
|---|---|---|
| `src/` across all three crates | 27 | +1,058 / −1,196 |
| `tests/` | 4 | +576 / −8 |
| `record/` + docs | 9 | +3,172 / −11 |
| `Cargo.lock` | 1 | +949 / −155 |

Source shrank by 138 lines net. The design spec estimated "roughly 1,200
deleted" against an actual 1,196 — the one prediction that landed exactly.

Biggest single moves:

| File | Lines |
|---|---|
| `bito-core/src/observability.rs` | −552, deleted outright |
| `bito-core/src/config.rs` | +163 / −299 |
| `bito/src/commands/doctor.rs` | +487 / −114 |
| `bito/src/lib.rs` | +71 / −74 |
| `bito/src/main.rs` | +84 / −53 |

`doctor.rs` grew. Porting eight checks to `DoctorCheck` impls is more lines
than a hand-rolled text renderer, and it buys a JSON path that cannot drift
from the text path.

Tests went 295 → 298, with 576 lines added across four files. A migration that
deletes 1,196 lines of source and adds 576 lines of test is doing the trade you
want.

## Dependencies

`bito-core` before:

```toml
directories = "6.0"
figment = { version = "0.10", features = ["toml", "yaml", "json", "env"] }
tracing-appender = "0.2"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

after:

```toml
librebar = { version = "0.6", default-features = false, features = ["config"] }
```

Four out, one in. figment alone drags toml, serde_yaml, and `pear`'s
proc-macros behind it. Every downstream consumer of `bito-core` pays less.

`bito` gained:

```toml
librebar = { version = "0.6", features = [
    "cli", "config", "logging", "crash", "diagnostics", "update",
] }
```

`config` in both crates, everything process-global in the binary only.

## Bugs that surfaced

### `bito serve` panicked on every invocation

`main` was `#[tokio::main]`; the `Serve` arm built a second runtime and called
`block_on` inside the first.

```
Cannot start a runtime from within a runtime. This happens because a function
attempted to block the current thread while the thread is being used to drive
asynchronous tasks.
```

100% reproducible, shipped to `main`, and no test invoked `serve`. Deleting
`#[tokio::main]` fixed it and stopped `analyze`, `lint`, and `tokens` from
paying for a runtime they never use.

### JSON mode enforced no thresholds

All six gating commands returned early on the JSON branch or put the check in
an `else if` that branch skipped. `--json` printed a report showing the input
had failed and exited 0.

Measured: `readability --max-grade 5` exited 1 as text, **0** as JSON.

Harmless while JSON was opt-in. Two commits earlier, `--format auto` had made
JSON the default for redirected output — so every CI gate that redirects had
silently stopped failing.

### Log file handling, four defects at once

| Behavior | bito before | librebar |
|---|---|---|
| Live file name | `bito.jsonl.2026-08-03`, changes daily | `bito.jsonl`, stable |
| Rotated files | accumulate uncompressed, forever | renamed + zstd, pruned at 7 days |
| Permissions | 0644 | 0600 |
| Directory order | `/var/log` → platform → **cwd** | platform → `/var/log` → stderr |

The stable name is the daily irritant — `tracing_appender::rolling::daily`
dates the *live* file, so there's no fixed path to `tail -f`.

The cwd fallback is the latent bug. With `HOME` unset, `platform_log_dir`
returns `None` and bito wrote `bito.jsonl.*` into the user's repository.
`.gitignore` covered `logs` and `*.log`, never `*.jsonl`. Invisible on a
developer's Mac, live in CI and containers.

0644 matters because the logs carry file paths and content excerpts.

### Every numeric `BITO_*` variable was dead

Found on the last day, checking documentation against the binary rather than
against the code.

librebar typed environment values against the serialized defaults. Every
numeric field on bito's `Config` is an `Option<T>` defaulting to `None`, which
serializes to `null` and carries no type at all. Six variables — `MAX_GRADE`,
`TOKEN_BUDGET`, `PASSIVE_MAX_PERCENT`, `STYLE_MIN_SCORE`, `MAX_INPUT_BYTES`,
`LOG_RETENTION_DAYS` — arrived as strings and the process exited 2 before
running:

```
invalid configuration at max_grade from environment variable BITO_MAX_GRADE
invalid type: string "8", expected f64
```

A regression from figment, whose `Env` provider parses loosely. bito's six
existing environment tests all set `BITO_DIALECT` — a string, and strings were
the one shape that still worked.

Fixed upstream in librebar 0.6 by recovering the type from the config type
itself.

## Decisions worth repeating

**Exit 1 = issues, exit 2 = could not run.** Inverted from the plan,
deliberately. clap hard-codes 2 for usage errors and won't let you change it,
so `EXIT_ISSUES_FOUND = 2` would have made exit 2 mean either "your prose needs
work" or "your flags are wrong." Letting 2 mean "could not run" puts clap's
default in the right bucket for free, and matches eslint, ruff, and shellcheck.

**Kept the `update` feature at +3.85 MiB.** Stripped `aarch64-apple-darwin`:
42.2 MiB → 46.1 MiB, +9.1%. Measured *before* wiring `UpdateChecker` the same
feature reads as 73 KB, because the linker dead-strips hyper and rustls when
nothing reaches them. Off by 53×.

**`log_retention_days` on `Config`** rather than accepting librebar's bare
7-day default. `0` disables; null and absent both mean "use the default."

**Update checks on `info` and `doctor` only.** `main` resolves the notice and
passes it in, so the network-touching commands are one reviewable list and
`cmd_doctor`'s unit tests stay offline. `serve` never checks — stdio is the MCP
channel.

## What it cost librebar

Four defects, all fixed upstream, all shipped in 0.6.0:

| | |
|---|---|
| `Option<T>` config fields unsettable from the environment | found by bito |
| no `Builder::with_log_level` | found by bito |
| `cli::parse` never applied `with_help_short` | found by bito |
| `generate_manpages` walked clap's generated `help` | found by bito |

The manpage one was shipping to users: 35 pages generated where 20 were
`bito-help-*.1`, and `cd.yml` copies the man directory wholesale. After the
fix, 15 pages and none of them junk.

The first real consumer is worth four rounds of internal review.

## Still open

**rmcp 1.3 → 3.1.** librebar re-exports 3.1; bito is pinned at 1.8.0. 1.4.0
does not compile — the `#[tool]` attribute changed shape, 8 errors. 1.8 also
moved `#[tool_handler]`'s default router from `self.tool_router` to
`Self::tool_router()`, which rebuilds the router on every call; `server.rs`
names it explicitly to restore the old behavior. Only a dead_code warning
marked that, and no test covers `call_tool` routing — it was verified by
driving `tools/list` and `tools/call` over stdio.

Its own change, its own testing.
