# CommonArgs: consumer friction ledger

**Status:** partially implemented (items 1–5 shipped, 6–8 open)
**Date:** 2026-07-31

Every defect below was found the same way: a real consumer wired `CommonArgs`
into a real binary and hit it. None were found by reading librebar's own tests.
That pattern is the point of this document — `CommonArgs` is the one type in
librebar whose failures only surface downstream, because its whole job is to be
embedded in someone else's clap tree.

## Source of findings

| Consumer | Date | Findings |
|---|---|---|
| `bblm` Phase 0 | 2026-07-25/26 | 1, 2, 3, 4, 7 |
| `receipts` carve-out | 2026-07-30 | 5 |
| `fte` | 2026-07-31 | 5 (reproduced), 6, 8 |

---

## Shipped in 0.3.0

### 1. `--version-only` was declared but inert

`CommonArgs` declared the flag and parsed it into a `bool`. Nothing in librebar
ever read that bool back. README line 132 already promised *"Print version
number and exit"* — so this was documentation the code hadn't caught up to, not
a feature request.

Worse, a consumer couldn't implement it correctly on their own without also
knowing about `Builder::with_version`, which lives on a different type. Omit it
and `app --version` reports *librebar's* version.

**Fixed:** `CommonArgs::apply(version) -> io::Result<Startup>`.

### 2. `apply_color()` / `apply_chdir()` were two mandatory calls

Two calls, both required, and forgetting the second failed *silently* — `-C`
would parse and then do nothing. A flag that parses and no-ops is the worst
shape a bug can take, because the CLI surface tells the user it worked.

All six of librebar's own examples called `apply_color()` + `apply_chdir()` and
none handled `--version-only`. **When the library author reproduces the same
incomplete startup six times, the defect is in the shape of the API, not the
discipline of the callers.**

**Fixed:** folded into `apply()`. `Startup` is `#[must_use]` so the compiler now
enforces what six examples couldn't remember.

### 3. `camino` was in the public API but not re-exported

`config_from_file(&Utf8Path)` and `ConfigSource::File` expose `camino` types,
but librebar didn't re-export the crate. Consumers had to add their own
`[dependencies]` entry and land on the same semver major, or hit the classic
`expected Utf8PathBuf, found Utf8PathBuf`.

**Fixed:** `pub use camino;` under `config`.

### 4. `-C /typo` gave an unactionable error

Reported only `No such file or directory (os error 2)` — never named the flag
or the path.

**Fixed:** now reads `--chdir /nope/nope: No such file or directory (os error 2)`,
preserving `io::ErrorKind`.

---

## Shipped 2026-07-31

### 5. Flattening `CommonArgs` overwrote the consumer's `--help` description

`fte --help` opened with:

```
Common CLI arguments shared across all librebar-based applications.

Embed in your app's CLI struct with `#[command(flatten)]`:

``` use clap::{Parser, Subcommand};
...
```

**Mechanism.** clap's derive emits *both* `about` and `long_about` from a
struct's doc comment inside `Args::augment_args`. `#[command(flatten)]` runs
`augment_args` against the parent `Command`, so librebar's rustdoc — written for
readers of this crate — landed on the consumer's command.

The parent's own `#[command(...)]` attributes are applied *after* the flatten,
which is why this hid so well: a consumer who sets `about` (nearly all of them)
sees a correct `-h` and a broken `--help`. A consumer who sets neither gets
librebar's text in both.

`receipts` worked around it locally with an explicit `long_about`. That fix
doesn't scale — it's a tax on every consumer, for a defect none of them caused.

**Fixed:** `#[command(about = None, long_about = None)]` on `CommonArgs`. The
rustdoc stays (it's a public type and genuinely useful to readers of librebar);
clap just stops adopting it.

Pinned by two tests in `tests/cli_test.rs`:
- `flattening_common_args_does_not_describe_the_consumer` — a consumer with no
  `about` gets `None`, not librebar's text.
- `a_consumer_that_sets_about_keeps_it_in_long_help` — renders `--help` and
  asserts the consumer's description survives and no librebar rustdoc appears.

Those harness structs use `//` rather than `///` deliberately. A doc comment on
the harness would itself become the command's about/long_about and mask the leak
under test — which is exactly how the first draft of these tests passed against
unfixed code.

---

## Open

### 6. `version_only` is the only non-global flag

```rust
#[arg(long)]                      pub version_only: bool,   // not global
#[arg(short = 'C', long, global = true)] pub chdir: ...
#[arg(short, long, global = true)]       pub quiet: bool,
// ...every other field is global
```

So `app --version-only` works but `app sub --version-only` errors, while every
other common flag propagates to subcommands. Inconsistent, and the inconsistency
is invisible until a user tries it.

**Decision needed.** Making it `global = true` is a one-word change and matches
every sibling, but it changes parse behavior for existing consumers — a command
line that used to error would start succeeding. Defensible as a patch under the
README's "adding new public items" policy; not obviously so. Not taken
unilaterally.

### 7. Global flags silently claim short letters from consumers

`CommonArgs` claims `-C`, `-q`, `-v` as `global = true`. A consumer that
declares `-v` on a subcommand gets a clap conflict panic at runtime. `bblm` hit
exactly this: its spec listed `-v` under `gen`, and the redeclaration was a
conflict.

This is inherent to global args and arguably correct — but it's undocumented.
The reserved short letters should be listed in the `CommonArgs` rustdoc and the
README's CLI table so consumers design around them instead of discovering the
conflict at first run.

### 8. `CommonArgs` derives `Parser`, not `Args`

`Parser` implies `CommandFactory`, so `CommonArgs::parse()` compiles and builds
a standalone command named after librebar. That's meaningless for a
flatten-only type. `Args` is the semantically correct derive.

Left alone for now: `#[derive(Parser)]` also emits the `Args` impl, so flatten
works either way, and narrowing it would break any consumer calling
`CommonArgs::parse()`. Worth doing at the next breaking release, not before.

---

## The general lesson

Findings 1, 2 and 5 are the same failure in three costumes: **the arg surface
and the arg behavior live in different places.** `CommonArgs` gives a consumer
the clap *declaration* for free, which makes "for free" read as "handled" — so
`--version-only` parsed and did nothing, `-C` parsed and did nothing, and the
struct's rustdoc silently became someone else's help text.

The test that catches this class is not a unit test of `CommonArgs`. It is a
test that embeds `CommonArgs` the way a consumer would and asserts on the
resulting `Command`. `tests/cli_test.rs` now does that for help text; the same
harness is where any future finding of this shape belongs.
