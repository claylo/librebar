# Configuration environment overrides

**Date:** 2026-07-31
**Status:** Implemented

## Overview

Librebar will add environment variables as a first-class configuration layer.
Applications get `{APP}_{FIELD}` names and `__` for nested fields. Values stay
typed. The new layer extends the working config loader instead of replacing it.

This is the first slice of the backlog in `scratch/TODO.txt`. It also includes
the config and CLI corrections that are small enough to ship with the new
layer. HTTP methods, cache key encoding, timestamp deduplication, CLI schema,
and the `--json` contract remain separate changes.

## Context

`ConfigLoader` currently merges defaults, user config, project config, and
explicit files. Environment variables are the missing deployment-facing
layer. A consumer can work around that omission field by field, but then every
application invents naming, parsing, nesting, errors, and precedence again.

The required precedence is:

```text
defaults
  < user config
  < project config
  < environment
  < explicit files
  < programmatic overrides from CLI flags
```

Environment variables override passive config discovery. An explicitly named
file, such as `--config foo.toml`, is a deliberate user instruction and wins
over the environment. A domain-specific CLI flag is the final user instruction
and wins over both.

The application name supplies the prefix. `my-app` becomes `MY_APP_`.
`MY_APP_DATABASE_URL` addresses `database_url`, while
`MY_APP_DATABASE__URL` addresses `database.url`. A single underscore remains
part of a field name. A double underscore crosses a struct boundary.

We evaluated Figment and config-rs before creating librebar. Neither fit as the
application foundation. We also evaluated envy for this change. Envy has a good
Serde-native scalar deserializer. It has neither nested separators nor sparse
overlays. Adding both would leave us owning a fork that still does not match the
loader.

## Approach

### Environment source boundary

Environment access sits behind an `EnvironmentSource` interface. The default
implementation reads `std::env::vars_os()`. Tests and specialized consumers can
provide a fixed source without mutating process-global state.

`ConfigLoader` enables the process environment by default. It exposes an
escape hatch to disable the layer and a method to replace its source. The
builder uses the same defaults as direct `ConfigLoader` use.

The source returns OS strings. Prefix filtering and path lookup happen before
the loader decodes a value. A known or explicitly collected variable with a
non-UTF-8 value is an error that names the variable. Unrelated and ignored
unknown variables are never value-decoded.

### Names and paths

The application prefix is ASCII uppercase. Every non-ASCII-alphanumeric
character in the application name becomes `_`, and one trailing `_` separates
the prefix from the field path.

After removing the prefix, the loader:

1. splits the remaining name on `__`;
2. rejects empty path segments;
3. lowercases each segment;
4. leaves single underscores untouched; and
5. inserts the value into a sparse object tree.

Environment paths use the same 64-level ceiling as `deep_merge`. The loader
rejects a path before insertion when it exceeds that limit. Exact duplicate
paths are errors. Parent/child conflicts are errors too. Results never depend
on environment iteration order. For example, setting both `MY_APP_DATABASE`
and `MY_APP_DATABASE__URL` fails.

After normalizing each path, the loader discards unknown paths unless
unknown-path collection is enabled. Discarded variables are not value-decoded,
conflict-checked, or recorded as configuration sources. Non-UTF-8 value,
parsing, duplicate-path, and parent/child conflict checks apply only to known
or collected paths.

### Typed values

The already-merged lower layers provide the value schema. Environment values
are parsed according to the value currently at the destination path:

- strings remain strings;
- booleans accept only the case-sensitive strings `true` and `false`;
- numbers preserve the destination number kind;
- arrays and objects use JSON syntax; and
- null or missing schema positions remain strings.

Boolean aliases are parse errors. Values such as `1`, `0`, `yes`, `no`,
`TRUE`, and `FALSE` are not accepted. This matches Serde's boolean spelling and
keeps application behavior independent of shell or deployment platform.

The null rule is deliberate. `Option<T>` commonly serializes as `null`. That
does not reveal `T` in the merged value tree. Guessing turns identifiers such
as `00123` into numbers and silently changes their meaning. A raw string is the
safe default. When an optional non-string value needs an environment override,
a discovered lower-precedence config file can provide a non-null schema value.

Examples:

```text
# default/file schema: port = 8080
MY_APP_PORT=9090                  -> number 9090

# default/file schema: build_id = null
MY_APP_BUILD_ID=00123             -> string "00123"

# file schema: tags = []
MY_APP_TAGS='["worker","blue"]' -> string array
```

Parsing failures name both the environment variable and the expected schema.

An empty environment value is a value, not an unset variable. String and
null-schema destinations receive `""`. Boolean, numeric, array, and object
destinations report a parse error. Therefore an `Option<String>` receives
`Some("")`, which remains distinct from `None`.

The loader ignores unknown paths by default, even when they match the
application prefix. A stale `MY_APP_TYPO_FIELD` must not break a consumer that
uses `deny_unknown_fields`. A path is known when it exists in the serialized
default or any discovered lower-precedence file layer; a null value still
establishes the field itself, but not children below that field. Explicit files
are applied later and therefore do not provide environment parsing schema.

Consumers with dynamic maps or other open-ended config can opt into collecting
unknown environment paths. Collected unknown values remain strings and then
follow the consumer's Serde policy. Ignored variables are not reported as
loaded configuration sources.

### CLI precedence

Librebar cannot infer application-specific Clap fields, so it will provide a
programmatic override layer above environment variables. A configured builder
and `ConfigLoader` can accept typed values at dotted paths such as
`database.url`. Consumers add only the CLI values that were actually present.

Programmatic values are serialized directly to the merge tree. They do not use
environment string parsing. This preserves the type Clap already produced. It
also makes CLI precedence explicit instead of relying on mutation after
startup.

### Loading and source metadata

`ConfigLoader::load_or_error` will consume `self` and call the same internal
loading path as `load`. It will no longer reconstruct the loader by cloning
every field. An applied environment variable or programmatic override counts
as a configuration source.

`ConfigSources` will record environment variable names and programmatic paths.
It will never record their values. Diagnostics can explain provenance without
leaking secrets.

### Adjacent corrections

This slice also makes the following localized changes from `scratch/TODO.txt`:

- add `Trace` to `LogLevel` and its string conversion;
- derive `clap::Args`, `Clone`, and `Debug` for `CommonArgs` while retaining
  `#[command(about = None, long_about = None)]`;
- fix `Liblibrebar` in the crate documentation; and
- update stale `0.2` dependency examples to `0.3` without changing the crate's
  actual version.

The current `Serialize` bound remains in this slice. Librebar uses the
serialized default value as the initial schema. That schema makes environment
parsing deterministic. Removing the bound requires a replacement schema
source. Deleting it here would trade a visible constraint for ambiguous runtime
coercion.

## Errors

Configuration errors gain enough context to identify the bad source and path.
The loader rejects:

- non-UTF-8 values for known or explicitly collected variables;
- empty or over-deep nested paths;
- duplicate and parent/child path conflicts;
- values that do not parse according to a known schema; and
- programmatic overrides whose values cannot be serialized.

Errors do not include environment values because they may contain credentials.

## Testing

Tests use a fixed `EnvironmentSource`; they do not call `set_var` or serialize
the suite around process-global state.

Coverage includes:

- prefix normalization and filtering;
- flat fields versus single-underscore field names;
- nested `__` paths;
- string, boolean, numeric, enum, array, and object values;
- strict boolean acceptance and rejection cases;
- empty strings for string, null, and non-string schemas;
- null-schema `Option<T>` values remaining strings;
- discovered-file schema for optional non-string values;
- unknown matching-prefix paths ignored by default and collected on request;
- ignored unknown variables skipped before decoding and conflict checks;
- every adjacent pair in the precedence chain;
- disabled and replaced environment sources;
- duplicate, conflicting, empty, over-deep, and invalid values;
- environment/programmatic provenance without values;
- `load_or_error` with files, environment, overrides, and no sources;
- builder integration; and
- `Trace`, `CommonArgs: Args + Clone`, and the existing help-text regression
  tests.

The completion gate is the repository's existing `just check` task.

README examples will quote compound values so they survive shell parsing, for
example `MY_APP_TAGS='["worker", "blue"]'`. The docs will also state the
boolean and empty-string rules and explain why explicit files override the
environment.

## Alternatives considered

### Use envy

Rejected. Envy is small and handles typed flat structs well, but nested
separator support is still absent and it deserializes a complete value rather
than a sparse overlay. A fork would require the two pieces librebar needs most.

### Parse every value as JSON

Rejected. It is pleasantly small and gets booleans and numbers right until a
string happens to look like a boolean or number. `BUILD_ID=00123` should not
need defensive quoting because the loader guessed.

### Implement a complete Serde overlay deserializer

Rejected for this slice. It can ask the destination Rust field for its exact
type and could eventually remove the `Serialize` bound, but it recreates a
substantial part of a configuration framework. The existing serialized default
already provides enough schema except at null positions, where we explicitly
prefer strings.

## Consequences

- Good: standard deployment overrides work through the builder and direct
  loader with no new dependency.
- Good: nested paths and CLI precedence are explicit and testable.
- Good: parsing follows known config shape rather than guessing from the text.
- Good: stale matching-prefix variables do not break strict config structs.
- Good: source metadata becomes complete without recording secret values.
- Cost: optional non-string fields need a non-null lower layer before an env
  value can be parsed as non-string.
- Cost: compound environment values use JSON syntax.
- Cost: consumers with dynamic maps must opt into unknown-path collection.
- Cost: `EnvironmentSource` expands the public config API.
- Deferred: removing the config type's `Serialize` bound needs a separate
  schema or deserialization design.
- Deferred: the remaining independent items in `scratch/TODO.txt` ship as
  focused changes after this slice.

## Related decisions

No ADRs exist for this design. Candidate decisions are the schema-driven value
parser, raw-string handling for null schema positions, and the public
`EnvironmentSource` boundary.
