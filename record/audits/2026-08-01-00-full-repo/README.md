---
audit_date: 2026-08-01
project: librebar
commit: 8fd83b359f9ae1da605a77f0e3a5fb38eb4611a7
scope: Full repository audit at 8fd83b3 — all 19 cargo features, 24 src modules, 19 integration test files, 9 examples
auditor: Claude Opus 5 (cased + crustoleum, 8 analysis agents, 4 verification agents)
findings:
  critical: 0
  significant: 17
  moderate: 17
  advisory: 16
  note: 13
---

# Audit: librebar

`librebar` is an opinionated application foundation for Rust CLIs and services — 7,195
lines across 24 modules behind 19 cargo features — audited at `8fd83b3` ahead of the 0.4.0
release. Its mechanical hygiene is enforced rather than merely claimed: clippy with `all`
plus `nursery` passes clean under `--all-targets --all-features -D warnings`, `cargo audit`
and `just deny` are clean against a codified policy, there is no unsafe code in first-party
source, and all eighteen named features compile in isolation. The recurring theme across
this audit is not missing work — it is controls that are compiled in and then left unarmed.

**The Transport and Cookie Surface** holds the sharpest instance: `cookie_store` is built
with its `public_suffix` feature enabled and the list is never installed, so the crate pays
for supercookie protection it does not receive, while the redirect follower strips a
three-name header blocklist rather than dropping credentials cross-origin and permits an
HTTPS-to-HTTP downgrade. **The Telemetry Surface** has the same shape with a larger blast
radius — the OTLP batch processor cannot drive the selected hyper client, reproduced end to
end as a panic on first export followed by silent total span loss. **The Diagnostics and
Disclosure Surface** inverts the usual threat model: the doctor bundle and crash handler
exist to be handed to strangers, and neither redacts anything nor restricts who can read the
file. **The Process Lifecycle Surface** holds the defects that only appear once something has
already gone wrong, notably a signal task that handles exactly one signal and leaves the
process permanently un-interruptible. **The HTTP Cache Surface** is where measurable cost
lives, with cached bodies routed through two stacked serialization layers for a verified
4.6x inflation. **The Release Boundary Surface** carries one item worth closing before 0.4.0
ships: there is no `[package.metadata.docs.rs]`, so docs.rs will publish the six default
features and hide the other twelve from every reader of the published documentation.

Two things are solid and worth stating plainly. Certificate verification is never disabled
anywhere in this crate, and there is no panic reachable from external input and no unwrap on
untrusted data in library code — for a foundation other people's binaries link against, that
is the outcome that matters most. **The Supply Chain Surface** is likewise in better shape
than most published crates, carrying an explicit license allowlist that documents its own
non-obvious entries and an advisory ignore list that is empty because the one suppression it
held was removed once its cause cleared. Arm the two controls that are already compiled in,
redact the two artifacts designed to be shared, add four lines of docs.rs metadata, and the
urgent work is done.

---

## The Release Boundary Surface

*The delivery pipeline has three gaps that will shape 0.4.0 exactly as they shaped 0.3.0: CI never builds the feature set consumers actually get, docs.rs will publish only that same set, and nothing automated reads README.md.*

### Every CI job builds --all-features; no job builds the default set or any individual feature {#ci-builds-only-all-features}

**significant** · `.github/workflows/ci.yml:68-72` · effort: small · <img src="assets/sparkline-ci-builds-only-all-features.svg" height="14" alt="commit activity" />

librebar declares 19 features gating roughly 37 optional dependencies, and its entire value proposition is that a downstream consumer selects a narrow subset. CI compiles exactly one point in that space. All five feature-bearing steps — clippy (line 42), nextest (line 69), doctests (line 72), cargo-deny (line 97) and the MSRV check (line 158) — pass `--all-features`. Nothing builds the default set, nothing builds `--no-default-features`, and nothing builds a single feature in isolation. `--all-features` is the single weakest configuration for catching feature-gating bugs, because with every feature on, every `#[cfg(feature = ...)]` arm is live and every optional dependency is in scope; a missing `dep:` edge, a `use` statement behind the wrong gate, or a function referenced from a module its own feature does not enable are all invisible. Feature unification hides the same class of bug in reverse: a module that only compiles because some *other* feature happened to pull its dependency in will build green forever under `--all-features` and fail the moment a user picks it alone. The gap is measurable rather than theoretical — the recon pass had to generate `.crustoleum/feature-matrix.txt` locally to learn that all 18 named features compile in isolation. That result is real (18/18 PASS) but it is a one-off developer artifact at one commit, not a gate; nothing prevents the next commit from breaking it, and the breakage surfaces as a downstream user's build error against a published crate.

```yaml .github/workflows/ci.yml:68-72
- name: nextest
  run: cargo nextest run --workspace --all-features --profile ci

- name: doctests
  run: cargo test --doc --all-features
```

Also at `.github/workflows/ci.yml:41-42`, `.github/workflows/ci.yml:154-158`, `Cargo.toml:144-148`.

Related: [ci-reimplements-justfile-recipes](#ci-reimplements-justfile-recipes), [serde-saphyr-exact-pin-on-default-path](#serde-saphyr-exact-pin-on-default-path).

**Remediation:** Add a feature-matrix job to `ci.yml` that runs `cargo check --no-default-features --features <F>` for each of the 19 features plus a bare `--no-default-features` and a default-features build. `cargo-hack` does this in one line (`cargo hack check --each-feature --no-dev-deps`) and is the ecosystem-standard tool; `--feature-powerset --depth 2` additionally catches pairwise interactions if build minutes allow. The existing `.github/actions/setup-cargo-tools` composite action already installs cargo tooling via binstall, so the new job reuses the established pattern. Keep the `--all-features` jobs — that is the right configuration for clippy and the test suite — and add the matrix alongside rather than in place of them. Adding a matching `just feature-matrix` recipe keeps the local and CI stories aligned with the project's existing convention.

<div>&hairsp;</div>

### No `[package.metadata.docs.rs]` and no `doc_auto_cfg`: 12 of 18 named features are invisible on docs.rs {#docs-rs-publishes-only-default-features}

**significant** · `Cargo.toml:144-163` · effort: trivial · <img src="assets/sparkline-docs-rs-publishes-only-default-features.svg" height="14" alt="commit activity" />

`grep -n 'metadata' Cargo.toml` returns nothing — there is no `[package.metadata.docs.rs]` table, and in fact no `[package.metadata.*]` table of any kind, in the manifest's 217 lines. docs.rs therefore builds with default features only, so `librebar::shutdown`, `otel`, `mcp`, `lockfile`, `http` (including `http::cache` and `http::cookies`), `update`, `dispatch`, and `bench` render as nothing at all. src/lib.rs advertises all of them in its front-page feature tables with intra-doc links like `[`http`]` and `[`mcp`]`, so on docs.rs those table rows point at modules the reader cannot open. Separately, a repo-wide grep for `doc_auto_cfg|doc_cfg|docsrs` across every `.rs` and `.toml` file returns zero hits — src/lib.rs:142 is a bare `#![deny(unsafe_code)]` — so there is not even a half-wired intent here, and even for the six default features nothing in the rendered docs tells a reader which Cargo feature gates which item. For a crate whose entire design is 19 feature flags, docs.rs is currently showing roughly a third of the API with no feature labels on any of it. This is invisible to every local and CI gate: `.crustoleum/feature-matrix.txt` records all 18 named features plus a bare `--no-default-features` build as PASS, which proves they compile — a different property from whether they are documented — and neither `.justfile` nor any workflow ever runs `cargo doc`.

```toml Cargo.toml:144-163
[features]
default = ["cli", "config", "logging", "crash", "cache", "diagnostics"]
cli = ["dep:clap", "dep:clap_complete", "dep:clap_mangen", "dep:owo-colors", "dep:serde_json"]
config = ["dep:toml", "dep:serde-saphyr", "dep:serde_json", "dep:camino", "dep:directories"]
logging = ["dep:tracing-subscriber", "dep:tracing-appender", "dep:serde_json", "dep:directories"]
shutdown = ["dep:tokio"]
crash = []
otel = ["logging", "dep:opentelemetry", "dep:opentelemetry_sdk", "dep:opentelemetry-otlp", "dep:tracing-opentelemetry", "dep:tokio"]
otel-grpc = ["otel", "opentelemetry-otlp/grpc-tonic"]
mcp = ["dep:rmcp", "dep:tokio"]
lockfile = []
http = ["dep:hyper", "dep:hyper-util", "dep:http-body-util", "dep:hyper-rustls", "dep:rustls", "dep:tokio", "dep:serde_json", "dep:tower", "dep:tower-http"]
http-cookies = ["http", "dep:cookie_store", "dep:atomic-write-file", "dep:url"]
http-cache = ["http", "cache", "dep:http-cache-semantics", "dep:sha2"]
cache = ["dep:serde_json", "dep:directories", "dep:base64", "dep:atomic-write-file"]
update = ["http", "cache", "dep:serde_json"]
dispatch = ["cli", "dep:which"]
diagnostics = ["config", "logging", "dep:flate2", "dep:tar"]
bench = ["dep:divan"]
bench-gungraun = ["dep:gungraun"]
```

Also at `Cargo.toml:1-13`, `src/lib.rs:142-142`.

**Remediation:** Add a `[package.metadata.docs.rs]` table with `all-features = true` and `rustdoc-args = ["--cfg", "docsrs"]`, and add `#![cfg_attr(docsrs, feature(doc_auto_cfg))]` at the top of src/lib.rs so every gated item renders with its feature badge. Verify locally with `RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --all-features --no-deps` before the next publish — this must land in the same release as the version bump above, or the newly published docs will still hide most of the crate.

<div>&hairsp;</div>

### crates.io publishing is fully manual — no automated workflow, no Trusted Publishing, no attestation {#no-attested-publish-path}

**moderate** · `.config/scrat.toml:1-8` · effort: medium · <img src="assets/sparkline-no-attested-publish-path.svg" height="14" alt="commit activity" />

librebar is published to crates.io as infrastructure — its own SECURITY.md names "Official distribution channels (crates.io, GitHub releases)" as in scope, and every downstream binary inherits whatever is uploaded. Yet no automation performs that upload. Grepping `.github/` for `cargo publish`, `CARGO_REGISTRY_TOKEN`, `id-token` and trusted-publishing markers returns nothing; the three workflows are CI, a Dependabot-alerts-to-issues job, and a PR-title linter; and the release tool's own config sets `no_publish = true`. Publication therefore happens from a developer workstation using a long-lived crates.io API token in `~/.cargo/credentials.toml`. That token is a standing bearer credential with no expiry and no scope beyond the crates it owns: anything that can read the developer's home directory — a malicious build script from any of the 30 dependencies that ship one, a compromised editor extension, a stolen laptop — can publish a librebar version that every downstream `cargo update` accepts. There is also no provenance record tying a published .crate file to the commit it was built from, so a malicious upload is not detectable by inspection after the fact. The rest of this repo's supply-chain posture is notably careful by comparison — all GitHub Actions are SHA-pinned (`ci.yml:24`, `:27`, `:94`), workflow permissions default to `contents: read`, `pull_request_target` is deliberately locked down with an explanatory comment, cargo-deny gates every PR, and the advisory policy is actively curated — which makes the publish step the weakest link in an otherwise well-defended chain.

```toml .config/scrat.toml:1-8
[project]
type = "rust"

[commands]
test = "just test"

[ship]
no_publish = true
```

Also at `SECURITY.md:44-49`.

**Remediation:** Adopt crates.io Trusted Publishing, the direct analogue of the OIDC flows already standard on PyPI and npm, which needs no stored secret. Register the repository and a `publish` workflow name in the crate's crates.io settings, then add a tag-triggered workflow granting `id-token: write` that exchanges the GitHub OIDC token for a short-lived (30-minute) crates.io token and runs `cargo publish`. Revoke the existing long-lived token once the flow is proven. Two additions make the result auditable rather than merely safer: gate the publish job on the existing CI jobs so nothing ships that has not passed cargo-deny, and emit build provenance with `actions/attest-build-provenance` so a published artifact traces to a commit. If manual publishing must be retained for release-timing control, keep `no_publish = true` but move the token to a hardware-backed store and document the trust assumption in SECURITY.md.

<div>&hairsp;</div>

### The doc-rot gate stops at `src/`: README.md's 16 Rust blocks are compiled by nothing {#readme-code-blocks-outside-the-doc-test-gate}

**advisory** · `.justfile:35-40` · effort: medium · <img src="assets/sparkline-readme-code-blocks-outside-the-doc-test-gate.svg" height="14" alt="commit activity" />

This project takes doc rot seriously and has already closed the harder half of the problem: `.justfile:35-40` documents exactly why `--all-features` is on the `doc-test` recipe, and CI runs that same invocation inline at ci.yml:72, so every PR compiles the rustdoc examples inside `mcp`, `dispatch`, `diagnostics`, `otel`, and `http`. All five components of the `just check` composite gate (`fmt clippy deny test doc-test`) are reimplemented in `.github/workflows/ci.yml` and do run on every PR. The gap is narrower and specific: README.md is not a doctest target at all, so no flag on `cargo test --doc` can reach it. A `grep -rn 'include_str' src/ tests/ examples/` returns nothing, so there is no `#![doc = include_str!("../README.md")]` bridging the two. That matters here more than in most crates because README.md is the single most-churned file in the repo — 17 commits, six of them in the five days before HEAD. I extracted all 16 ```rust blocks and compiled the headline one (README.md:5-53) against HEAD with default features; it passes, so this is a missing guardrail rather than a currently-broken snippet. The remaining 15 are fragments no tooling has ever type-checked; they reference `CommonArgs::apply`, `ResolvedOutputFormat`, `SchemaMetadata`/`CommandMetadata`/`ErrorMetadata`/`OutputField`/`Stability`, `parse_with`, `render_manpage`, `generate_manpages`, `with_help_short`, `UnknownEnvironment::Collect`, and `with_config_override` — every one of which I confirmed exists at HEAD with the documented shape, but nothing keeps that true through the next refactor. Nothing automated reads README.md at all, including its version strings.

```just .justfile:35-40
# Doc-tests run with --all-features so feature-gated modules (mcp, dispatch,
# diagnostics, otel, http, etc.) actually compile their examples. Without
# this flag, doc blocks inside `#[cfg(feature = "…")]` modules are skipped
# entirely and can rot unnoticed.
doc-test:
  cargo test --doc --all-features
```

Also at `.github/workflows/ci.yml:71-72`, `src/lib.rs:1-3`, `README.md:5-53`.

**Remediation:** Extend the existing gate rather than adding a new one. Add `#![cfg_attr(doc, doc = include_str!("../README.md"))]` to src/lib.rs, or a `#[cfg(doctest)] #[doc = include_str!("../README.md")] mod readme_doctests;` which keeps the README out of the rendered front page while still compiling it. Convert the fragment blocks to `no_run` doctests with hidden `#`-prefixed scaffolding, matching what src/lib.rs:88-122 already does. The existing `just doc-test` recipe and its CI counterpart then cover them with no change to either. Blocks that are genuinely illustrative pseudo-code should be marked ```text so the intent is explicit.

<div>&hairsp;</div>

### CI never invokes just; it reimplements each recipe as a cargo command, and the drift hazard is self-documented {#ci-reimplements-justfile-recipes}

**note** · `.github/workflows/ci.yml:82-92` · effort: small · <img src="assets/sparkline-ci-reimplements-justfile-recipes.svg" height="14" alt="commit activity" />

The policy is enforced — this is worth stating plainly, because the opposite would be the serious finding. `.justfile:48` defines `check: fmt clippy deny test doc-test`, and CI covers all five: `cargo fmt --check` (line 39), clippy (42), cargo-deny via the Embark action (94), nextest (69) and doctests (72). Coverage parity is real. What is absent is a shared definition. CI never invokes `just`; the sole occurrence of the word in the workflows is inside a comment. Each step is transcribed by hand, so the two descriptions of "what checking this project means" must be kept in agreement by memory. The cargo-deny step shows the cost already being paid: it needs an 11-line comment explaining that `--config` placement moved between cargo-deny 0.19 and 0.20, that the action pin therefore determines flag order, and that the pin and `.justfile`'s recipe must stay in step. That comment exists because the drift already happened once — `ci: fix cargo-deny invocation, bump all action pins (#23)` on 2026-07-25 — and it documents a hazard rather than removing it. The exposure is bounded and the current state is correct; the point is that correctness here rests on a contributor reading a comment, and the divergence would be silent, appearing as CI checking something subtly different from what `just check` checks locally.

```yaml .github/workflows/ci.yml:82-92
# The action composes `cargo-deny <arguments> <command> <command-arguments>`.
#
# `--config` position is cargo-deny-version-specific and the two forms are
# mutually exclusive: <=0.19.x accepts it ONLY after the subcommand, 0.20+
# ONLY before it. So the action release matters — v2.1.1 is the first that
# bundles cargo-deny 0.20.2, matching the version `just deny` runs locally.
# Keep this pin and .justfile's `deny` recipe in step; if they drift apart,
# one of the two starts failing on the `--config` placement.
#
# `arguments` defaults to `--all-features`, restated here because setting
# it overrides the default.
```

Also at `.justfile:26-27`, `.justfile:48-48`.

Related: [ci-builds-only-all-features](#ci-builds-only-all-features).

**Remediation:** Make `.justfile` the single definition and have CI call it, installing `just` through the existing `setup-cargo-tools` composite action and replacing each `run:` with `just clippy`, `just deny`, `just test-ci`, `just doc-test`. Then the flag-placement comment describes one invocation instead of coordinating two, and it can be trimmed. The cargo-deny step is the one place to weigh a tradeoff rather than convert reflexively: the Embark action pins its own cargo-deny binary by SHA, which is a genuine supply-chain benefit that `just deny` (resolving whatever version is installed) does not provide. Keeping the action for that reason is defensible — in which case pin the cargo-deny version in `.justfile` too, so local and CI agree by construction rather than by comment. Either resolution is better than the present split; leaving it as-is is the only option that keeps paying the synchronisation cost.

<div>&hairsp;</div>

### Five unresolved intra-doc links to `Error::Cache` / `Error::Http` in public `# Errors` sections {#unresolved-intra-doc-error-links}

**note** · `src/cache.rs:66-71` · effort: trivial · <img src="assets/sparkline-unresolved-intra-doc-error-links.svg" height="14" alt="commit activity" />

`cargo doc --all-features --no-deps` emits exactly five warnings, all of the same shape: `warning: unresolved link to `Error::Cache`` at src/cache.rs:70, 98, 132, 146 and `warning: unresolved link to `Error::Http`` at src/http.rs:519. Neither module imports `Error` into scope — src/cache.rs:34 imports `use crate::error::{CacheError, Result};` and uses `crate::Error` nowhere — so rustdoc cannot resolve the path and silently degrades each one to literal text. These sit in the `# Errors` section of five public methods (`Cache::set`, `Cache::get`, `Cache::remove`, `Cache::clear`, `HttpClient::get`), which is precisely where a reader goes to find out what to match on. The variants do exist and the docs are otherwise accurate; only the navigation is broken. Worth noting the contrast: the crate is clean on `missing_docs` — a `cargo check --all-features` with `missing_docs = "warn"` set produces zero missing-documentation warnings across the entire public API — so this is an isolated blemish rather than a pattern of doc neglect. It survives because `cargo doc` is run nowhere: not by any `.justfile` recipe (the file has `fmt`, `clippy`, `deny`, `test`, `test-ci`, `doc-test`, `cov`, and the dependency recipes, but no `doc`) and not by any workflow in `.github/workflows/`.

```rust src/cache.rs:66-71
/// Store a value with a TTL.
///
/// # Errors
///
/// Returns [`Error::Cache`] if the entry cannot be written.
pub fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
```

Also at `src/cache.rs:98-98`, `src/cache.rs:132-132`, `src/cache.rs:146-146`, `src/http.rs:519-519`.

Related: [docs-rs-publishes-only-default-features](#docs-rs-publishes-only-default-features).

**Remediation:** Change the five links to `[`crate::Error::Cache`]` and `[`crate::Error::Http`]`, or add `use crate::Error;` to each module. Then add `#![deny(rustdoc::broken_intra_doc_links)]` to src/lib.rs so a regression fails the build, and add a `doc` recipe (`cargo doc --all-features --no-deps`) to `.justfile` wired into the `check` gate, with the matching step in the CI lint job. That closes the blind spot that let these five accumulate, and is the same gate that would verify the docs.rs metadata fix above.

<div>&hairsp;</div>

### No CONTRIBUTING.md and no status badges on a published crate with full issue/PR template scaffolding {#missing-contributing-and-status-badges}

**note** · `README.md:1-3` · effort: small · <img src="assets/sparkline-missing-contributing-and-status-badges.svg" height="14" alt="commit activity" />

The repo already carries most of the 2026 published-crate furniture: SECURITY.md with a disclosure process, both LICENSE-APACHE and LICENSE-MIT matching the `license = "Apache-2.0 OR MIT"` declaration, `.github/ISSUE_TEMPLATE/` with bug and feature forms plus a config, four PR templates, and dependabot. Two pieces are absent. There is no CONTRIBUTING.md — a repo-wide `find . -iname '*contributing*'` returns nothing, so it is absent from the root, from `.github/`, and from `.config/`, and nothing links to it (no 404) — but a drive-by contributor arriving at four PR templates has no document explaining which to use, the conventional-commit requirement that `.github/workflows/lint-pr.yml` enforces, or that `just check` is the gate. And `grep -n '!\[' README.md` returns nothing: no badges at all, so a reader cannot see build status, the current crates.io version, or the MSRV without leaving the page. The MSRV claim itself is sound — README.md:591-596 says 1.89.0 is "pinned in Cargo.toml's `rust-version` field and tested against in CI", and the `msrv` job in ci.yml does exactly that, including setting `RUSTUP_TOOLCHAIN` explicitly so rust-toolchain.toml's 1.97.1 build channel cannot shadow it. All three README relative links (LICENSE-APACHE, LICENSE-MIT, src/error.rs) resolve.

```markdown README.md:1-3
# Librebar

Opinionated application foundation for Rust CLIs and services. Add one dependency and get an agent-ready CLI, layered config with environment overrides, structured logging, crash dumps, file caching, and a diagnostics bundle — out of the box.
```

**Remediation:** Add a short CONTRIBUTING.md covering the `just check` gate, the conventional-commit format that lint-pr.yml enforces, and which of the four PR templates to pick; link it from README.md and from `.github/ISSUE_TEMPLATE/config.yml`. Add CI, crates.io, docs.rs, and MSRV badges under the README title — the MSRV badge in particular makes the 1.89.0 floor visible at a glance rather than 590 lines down.

<div>&hairsp;</div>

*Verdict: Fix the docs.rs metadata before 0.4.0 ships. It is four lines of manifest, it is invisible to every local check because `just check` passes without it, and it decides what every reader of the published documentation is able to see — currently the six default features and nothing else. The CI gap is the structural one: every job builds `--all-features`, so no pipeline has ever compiled the default set a stock consumer gets. That the per-feature matrix passes 18/18 today is a fact about this commit, not a property the pipeline enforces. The remaining items are small and none of them block a release.*

<div>&nbsp;</div>

---

## The Diagnostics and Disclosure Surface

*The doctor bundle and the crash handler both exist to be shared with strangers, and neither redacts anything or restricts who can read the file.*

### Debug bundle performs no redaction and writes a 0644 archive intended for public bug reports {#debug-bundle-ships-unredacted-content-world-readable}

**significant** · `src/diagnostics.rs:211-215` · effort: medium · <img src="assets/sparkline-debug-bundle-ships-unredacted-content-world-readable.svg" height="14" alt="commit activity" />

DebugBundle is the crate's answer to "produce an archive the user attaches to a bug
report" — the doctor-bundle example writes config-sources.json into it and prints the path
for the user to attach, and the crate description advertises "a diagnostics bundle" as a
headline feature. Neither add_text, add_bytes, nor add_doctor_results inspects or
transforms the content: bytes go straight into self.files and then into the tar. There is
no Sanitize trait, no denylist of key names, and no pattern scan for tokens or connection
strings — grep across src/ finds no redaction machinery at all. The project's own design
documents state the opposite: record/superpowers/plans/2026-04-08-rebar-phase3.md:1650
says "collects sanitized config, logs, doctor output into tar.gz", and line 2251 defers
field-level redaction to the consumer without providing any hook for it. So the shipped
API silently produces an unsanitized artifact under a name that promises sanitization.
Anything a caller naturally feeds it — a merged config value, a log tail, an env dump,
a cache file — carries API keys, bearer tokens, and database URLs straight into a file
whose entire purpose is to be uploaded to a public issue tracker. Second problem: the
archive is created with std::fs::File::create (diagnostics.rs:241), giving 0666 & ~umask
(typically 0644), while cache.rs:184 and cookies.rs:43 both set 0o600 explicitly. On a
shared host any local user can read the bundle out of the writer's log or cache directory
before it is deleted. For completeness: DebugBundle only writes tar archives — there is no
Archive::unpack or entries() call anywhere in the crate — so tar-slip and decompression-bomb
exposure on the read side does not apply here.

```rust src/diagnostics.rs:211-215
pub fn add_text(&mut self, name: &str, content: &str) -> &mut Self {
    self.files
        .push((name.to_string(), content.as_bytes().to_vec()));
    self
}
```

Also at `src/diagnostics.rs:218-221`, `src/diagnostics.rs:241-241`.

> I do not have to breach anything here. I file a plausible bug report, and I ask, in the friendliest possible terms, whether you could attach the output of your doctor command. You will. It is what the command is for. Everything it swept up — config, environment, log tails — arrives in a public issue thread, indexed, permanent, and volunteered.

Enabled by [crash-dump-world-readable-and-unbounded](#crash-dump-world-readable-and-unbounded), [request-uri-with-credentials-recorded-in-log-spans](#request-uri-with-credentials-recorded-in-log-spans).

**Remediation:** Give the bundle a redaction step it cannot be used without. Two concrete options, best
combined: (1) add a Redactor trait plus a default implementation that scrubs values for
keys matching a token/secret/password/key/authorization pattern in JSON, TOML, YAML, and
dotenv-shaped text, and route every add_* call through it; (2) make add_text/add_bytes
take an explicit Sensitivity marker so the caller must state that a payload was already
sanitized. Additionally, create the archive with an explicit 0o600 mode (OpenOptions with
std::os::unix::fs::OpenOptionsExt::mode) rather than File::create, and set entry headers
to 0o600 rather than 0o644 so the permissions survive extraction.

<div>&hairsp;</div>

### Full request URI, including userinfo and query string, is recorded as a tracing span field {#request-uri-with-credentials-recorded-in-log-spans}

**significant** · `src/http.rs:639-643` · effort: small · <img src="assets/sparkline-request-uri-with-credentials-recorded-in-log-spans.svg" height="14" alt="commit activity" />

http::Uri's Display impl (http-1.4.2/src/uri/mod.rs:1032-1049) writes the scheme, then
the authority through its own Display impl at uri/authority.rs:423-427, which is
f.write_str(self.as_str()) — and as_str() returns the raw authority data with userinfo
intact, which is why the crate's own host() helper at :429 has to rsplit on '@' to strip
it — then the path, then "?" and the raw query. So `url = %request.uri()` captures both `https://user:pass@host/` credentials
and query-string bearer material such as `?token=`, `?access_token=`, `?sig=`, and
presigned-URL signatures. Every request through HttpClient goes through send(), including
get/post/put/patch/delete, the conditional helpers, and get_cached. The span is created at
INFO, so its fields are recorded whenever logging is enabled at info or below. JsonLogLayer
then flattens every enclosing span's fields into each event it writes (logging.rs:399-405),
so the debug event at http.rs:660 ("response received") emits a JSONL line carrying the
full URL. That fires on the documented -v flag and on RUST_LOG=debug. The sink is a
tracing_appender daily-rolling file opened with OpenOptions::new().append(true).create(true)
and no mode (tracing-appender-0.2.5/src/rolling.rs:790-791), i.e. 0666 & ~umask — typically
0644 and world-readable, in contrast with cache.rs and cookies.rs, which both set 0o600
explicitly. Worse, candidate #3 in resolve_log_target_with is std::env::current_dir(), so
when the platform log dir and /var/log are not writable the JSONL lands in the working
directory — a repo checkout or a CI workspace that is later archived or uploaded. Attacker:
anyone with read access to the log file or to an artifact/backup containing it, and any
support workflow where a user is asked to attach logs to a public issue.

```rust src/http.rs:639-643
#[tracing::instrument(
    skip(self, request),
    fields(method = %request.method(), url = %request.uri())
)]
pub async fn send(&self, mut request: Request<Bytes>) -> Result<Response> {
```

Also at `src/logging.rs:399-405`, `src/logging.rs:212-214`.

> Query strings are where tokens go when someone is in a hurry: `?token=`, `?key=`, `?signature=`. The span records the whole URI at INFO, which is the level everybody ships. I do not need your process — I need your log aggregator, your shipped log file, or the terminal scrollback in the screenshot you paste into chat.

Related: [crash-dump-world-readable-and-unbounded](#crash-dump-world-readable-and-unbounded), [response-debug-impl-exposes-body-and-set-cookie](#response-debug-impl-exposes-body-and-set-cookie).

**Remediation:** Record a sanitized URL. Build the span field from the URI's scheme, host, port, and path
only — drop the query entirely and never emit the authority's userinfo — e.g.
fields(method = %request.method(), url = %sanitized_uri(request.uri())) where
sanitized_uri reassembles from uri.scheme_str(), uri.host(), uri.port_u16(), and
uri.path(). If the query is diagnostically necessary, log only its parameter names. Also
set an explicit 0o600 mode on the log file (tracing_appender::rolling::Builder does not
expose it, so this needs a custom MakeWriter or a post-create chmod), and drop
current_dir() from the log-target candidate list or gate it behind an explicit opt-in.

<div>&hairsp;</div>

### Crash dumps are written with default permissions and never pruned {#crash-dump-world-readable-and-unbounded}

**moderate** · `src/crash.rs:120-133` · effort: small · <img src="assets/sparkline-crash-dump-world-readable-and-unbounded.svg" height="14" alt="commit activity" />

The crash report body is `CrashInfo::format()`, which embeds the raw panic payload
and a `Backtrace::force_capture()` (captured unconditionally, regardless of
`RUST_BACKTRACE`). Panic payloads routinely carry the data that caused the panic —
a rejected token, a request body, a path — and forced backtraces carry the build
machine's absolute source paths. `std::fs::write` creates the file with mode
`0o666 & ~umask`, so on a default umask the report lands world-readable in
`~/Library/Caches/{app}/crashes/` or `$XDG_CACHE_HOME/{app}/crashes/`.

The same crate is deliberately stricter elsewhere: `src/cache.rs:178-190`
(`write_entry`) sets `options.mode(0o600).preserve_mode(false)` for cache entries,
and `src/http/cookies.rs:38-47` does the same for cookie jars. Crash dumps, which
carry strictly more sensitive content than a cache entry, are the one persistence
path that does not.

Filenames are `{app}-{timestamp}.crash` with millisecond resolution and there is no
retention policy, cap, or cleanup anywhere in the module. A service that crash-loops
under a supervisor writes one forced-backtrace file per restart forever.

```rust src/crash.rs:120-133
pub fn write_crash_dump_to(info: &CrashInfo, dir: &Path) -> Option<PathBuf> {
    if std::fs::create_dir_all(dir).is_err() {
        return None;
    }

    // Use timestamp chars that are safe in filenames
    let ts = info.timestamp.replace([':', '.'], "-");
    let filename = format!("{}-{}.crash", info.app_name, ts);
    let path = dir.join(&filename);

    let content = info.format();
    std::fs::write(&path, content).ok()?;
    Some(path)
}
```

Also at `src/cache.rs:178-190`.

> A crash dump is a snapshot of a program at its least composed moment, and this one lands with default permissions in a directory nothing ever prunes. Any local account can read it. I do not need to cause the crash; I only need to be patient, because nothing removes yesterday's.

Related: [crash-dumps-documented-as-json-are-free-text](#crash-dumps-documented-as-json-are-free-text), [crash-hook-print-turns-panics-into-aborts](#crash-hook-print-turns-panics-into-aborts).

**Remediation:** Write the dump through the same restricted-mode path used by `cache::write_entry`
(`OpenOptions::mode(0o600)` on unix, `create_new` to avoid clobbering), and add a
bounded retention policy — keep the N most recent `.crash` files, or drop dumps
older than a configurable age, pruned on install or on write. Document in the module
header that dumps may contain panic payloads so consumers know what they are
persisting.

<div>&hairsp;</div>

### DebugBundle holds every file in RAM and copies each one twice, with no streaming API {#debug-bundle-buffers-entire-archive-in-memory}

**moderate** · `src/diagnostics.rs:192-221` · effort: medium · <img src="assets/sparkline-debug-bundle-buffers-entire-archive-in-memory.svg" height="14" alt="commit activity" />

`DebugBundle` accumulates `files: Vec<(String, Vec<u8>)>` and only writes the tar stream in `finish()` (`src/diagnostics.rs:245-253`), so the uncompressed contents of every file in the bundle are resident in memory simultaneously. Both entry points copy rather than take ownership: `add_text` calls `content.as_bytes().to_vec()` and `add_bytes` calls `data.to_vec()`, so the caller's buffer and the bundle's buffer coexist for the duration of each call. There is no streaming input path — the public API offers only `add_text`, `add_bytes`, and `add_doctor_results`, none of which accept a filesystem path. This matters because the module's stated purpose is packaging diagnostics for a long-running service, and the crate's own `logging` module writes daily-rotating JSONL (`src/logging.rs:278`) with no size cap or retention policy. A caller attaching a day of logs must first read the whole file into a `Vec<u8>`, then hand it to `add_bytes`, which copies it again — peak usage is roughly twice the total bundle size, on a code path most likely to run on a machine already in trouble. The safe alternative is available and unused: `tar::Builder` in the pinned tar 0.4 exposes `append_path`, `append_path_with_name`, `append_file`, and `append_dir_all`, all of which stream from disk into the encoder.

```rust src/diagnostics.rs:192-221
pub struct DebugBundle {
    app_name: String,
    dir: PathBuf,
    files: Vec<(String, Vec<u8>)>,
}

impl DebugBundle {
    /// Create a new debug bundle builder.
    ///
    /// The archive will be written to `dir`.
    pub fn new(app_name: &str, dir: &Path) -> Self {
        Self {
            app_name: app_name.to_string(),
            dir: dir.to_path_buf(),
            files: Vec::new(),
        }
    }

    /// Add a text file to the bundle.
    pub fn add_text(&mut self, name: &str, content: &str) -> &mut Self {
        self.files
            .push((name.to_string(), content.as_bytes().to_vec()));
        self
    }

    /// Add a binary file to the bundle.
    pub fn add_bytes(&mut self, name: &str, data: &[u8]) -> &mut Self {
        self.files.push((name.to_string(), data.to_vec()));
        self
    }
```

**Remediation:** Add a path-taking entry point (for example `add_file(&mut self, name: &str, path: &Path)`) that records the path instead of the bytes, and have `finish()` dispatch to `tar::Builder::append_path_with_name` or `append_file` for those entries so file contents stream through the gzip encoder without ever being fully buffered. Keep `add_text`/`add_bytes` for small in-memory content such as the doctor report. Change `add_bytes` to accept `Vec<u8>` (or `impl Into<Vec<u8>>`) so callers that already own a buffer can move it in rather than forcing a copy. Consider a documented ceiling on total buffered bytes so an oversized bundle fails loudly rather than exhausting memory.

<div>&hairsp;</div>

### Response derives Debug over the full body and header map, including Set-Cookie {#response-debug-impl-exposes-body-and-set-cookie}

**advisory** · `src/http/response.rs:57-63` · effort: small · <img src="assets/sparkline-response-debug-impl-exposes-body-and-set-cookie.svg" height="14" alt="commit activity" />

Response and ResponseMetadata both derive Debug with no redacting impl. The derived output
contains body (up to the 16 MiB max_response_size, so an entire API response — tokens,
PII, whatever the endpoint returns) and headers, which for a login or session-refresh call
is exactly where Set-Cookie lives. tracing is a core dependency and the crate teaches
structured logging as the default idiom, so `tracing::error!(?response, "unexpected
status")` and `tracing::debug!(?response)` are the natural things for a downstream author
to write when a request misbehaves. That writes the whole body into the JSONL log via
JsonVisitor::record_debug (logging.rs:473-478), which formats any Debug value into a
string with no filtering. Same story for a panic message built with {response:?}, which
then lands in a crash dump. This is defense-in-depth rather than a reachable-from-untrusted-input
flaw — librebar itself never Debug-formats a Response — but for a foundation crate whose
whole premise is "we made the production-minded choices for you," a footgun this easy to
pull is worth closing. ConditionalResponse and ModificationCheck inherit the same exposure
through their variants.

```rust src/http/response.rs:57-63
#[derive(Debug)]
pub struct Response {
    pub(super) metadata: ResponseMetadata,
    pub(super) body: Vec<u8>,
    #[cfg(feature = "http-cache")]
    pub(super) cache_status: Option<CacheStatus>,
}
```

Also at `src/http/response.rs:7-13`.

> Someone will `dbg!` this. Someone will put it in an error path, or a `tracing::debug!`, or a test assertion that prints on failure in CI. The derive means the entire body and every header — `Set-Cookie` included — renders wherever that happens, and CI logs are read by more people than production ones.

Related: [request-uri-with-credentials-recorded-in-log-spans](#request-uri-with-credentials-recorded-in-log-spans).

**Remediation:** Hand-write Debug for Response and ResponseMetadata. Print status, version, header names
with their values elided (or values shown only for a safe allowlist such as content-type,
content-length, date, etag), and the body as a length rather than its bytes — e.g.
f.debug_struct("Response").field("status", &self.status()).field("body_len",
&self.body.len()).finish_non_exhaustive(). Keep the full data reachable through the
existing bytes(), text(), and headers() accessors so nothing is lost for callers who
deliberately want it.

<div>&hairsp;</div>

### Crash dumps are documented as structured JSON but are unparseable free text {#crash-dumps-documented-as-json-are-free-text}

**advisory** · `src/crash.rs:50-61` · effort: medium · <img src="assets/sparkline-crash-dumps-documented-as-json-are-free-text.svg" height="14" alt="commit activity" />

Two pieces of shipped documentation promise machine-readable crash output.
`README.md:64` states "structured JSON crash dumps on panic, written to XDG cache",
and the feature table in `src/lib.rs:21` repeats "Structured JSON crash dumps on
panic". The implementation writes a fixed-width text banner with a free-form
backtrace appended, and `CrashInfo` derives only `Debug` — there is no `Serialize`
impl anywhere in the module.

Anyone who builds crash triage on the documented contract — a log shipper, a
`doctor` check, a bundle collector that folds `.crash` files into the tar.gz built
by `src/diagnostics.rs` — gets text that no JSON parser accepts. The panic message
itself is interpolated unescaped into the `Message:` line, so a multi-line panic
payload also breaks any line-oriented parse of the text format.

```rust src/crash.rs:50-61
pub fn format(&self) -> String {
    let location = self.location.as_deref().unwrap_or("<unknown location>");

    let mut report = format!(
        "=== Crash Report ===\n\
         App:       {} {}\n\
         Timestamp: {}\n\
         OS:        {}\n\
         Location:  {}\n\
         Message:   {}\n",
        self.app_name, self.version, self.timestamp, self.os, location, self.message,
    );
```

Also at `README.md:64-64`, `src/lib.rs:21-21`.

Related: [crash-dump-world-readable-and-unbounded](#crash-dump-world-readable-and-unbounded).

**Remediation:** Derive `serde::Serialize` on `CrashInfo` and write the dump as a single JSON object
(message, location, app_name, version, timestamp, os, backtrace), which is what both
documents already describe. `serde` is a non-optional core dependency with `derive`
enabled (Cargo.toml:28), so the derive itself costs nothing — but `serde_json` is
optional (Cargo.toml:41) and `crash = []` (Cargo.toml:150) enables no dependencies at
all, so the change must also widen the feature to `crash = ["dep:serde_json"]`.
Without that, `cargo check --no-default-features --features crash` — a cell the
project's own 18/18 per-feature matrix exercises — does not build. Keep `format()` as
the human-readable rendering used for the terminal message. If the text format must
stay on disk, correct `README.md:64` and `src/lib.rs:21` instead — but the documented
contract is the JSON one.

<div>&hairsp;</div>

### DebugBundle mixes &mut Self chaining with a consuming finish() {#debug-bundle-builder-cannot-be-chained}

**moderate** · `src/diagnostics.rs:210-231` · effort: trivial · <img src="assets/sparkline-debug-bundle-builder-cannot-be-chained.svg" height="14" alt="commit activity" />

The three `add_*` methods return `&mut Self`, which signals a fluent builder, but `finish` takes `self` by value. `DebugBundle::new(app, &dir).add_text("a", "b").finish()` therefore does not compile: `.add_text(..)` yields a `&mut DebugBundle` borrowed from a temporary, and calling `finish` on it is a move out of a mutable reference. The chain is only usable if it is bound to a `let mut` first and `finish` is called as a separate statement — which is exactly what both in-tree call sites do (examples/doctor-bundle.rs:143-147 and tests/diagnostics_test.rs:76). Callers reading only the signatures will try the one-liner and hit a borrow-check error whose message points at ownership rather than at the API's shape. The crate's other builders do not have this problem: `HttpClientBuilder` (src/http.rs:252-336), `SchemaMetadata` (src/cli/schema.rs:297-330) and `ErrorMetadata` (src/cli/schema.rs:225-256) all take and return `self` by value.

```rust src/diagnostics.rs:210-231
    /// Add a text file to the bundle.
    pub fn add_text(&mut self, name: &str, content: &str) -> &mut Self {
        self.files
            .push((name.to_string(), content.as_bytes().to_vec()));
        self
    }

    /// Add a binary file to the bundle.
    pub fn add_bytes(&mut self, name: &str, data: &[u8]) -> &mut Self {
        self.files.push((name.to_string(), data.to_vec()));
        self
    }

    /// Add doctor results to the bundle.
    pub fn add_doctor_results(&mut self, results: &[NamedResult]) -> &mut Self {
        let report = DoctorRunner::format_report(results);
        self.add_text("doctor-report.txt", &report)
    }

    /// Write the tar.gz archive and return its path.
    pub fn finish(self) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.dir).map_err(Error::Diagnostic)?;
```

Also at `examples/doctor-bundle.rs:143-147`, `tests/diagnostics_test.rs:76-76`.

Related: [doctor-check-registration-forces-caller-boxing](#doctor-check-registration-forces-caller-boxing).

**Remediation:** Make the `add_*` methods take and return `self` by value with `#[must_use]`, matching `HttpClientBuilder` and `SchemaMetadata`, so the fluent one-liner works and `finish(self)` is reachable from the chain. The two in-tree call sites need their `let mut` removed. If the `&mut` form is worth keeping for callers that add files in a loop, keep it under distinct `push_*` names so the two styles are not confusable from the signature.

<div>&hairsp;</div>

*Verdict: This surface inverts the usual threat model: nothing here is attacked, everything here is volunteered. A debug bundle is created precisely so it can be attached to a public issue, which makes unredacted config and environment content a disclosure path that requires no adversary at all — only a helpful user. The 0644 permissions on both the bundle and the crash dumps are the sharper edge, because they expose the data to every local account before the user decides to share anything. Redaction is the substantial work; the permission bits are a one-line fix and should not wait for it.*

<div>&nbsp;</div>

---

## The HTTP Cache Surface

*The cache is correct about freshness and careless about everything else it writes to disk: what the bytes cost, who can read them, and when they are removed.*

### Cached HTTP bodies are written through two serialization layers, inflating them 4.6x and costing 10 ms of CPU per 1 MiB cache hit {#http-cache-entry-body-amplification}

**significant** · `src/http/cache.rs:561-584` · effort: medium · <img src="assets/sparkline-http-cache-entry-body-amplification.svg" height="14" alt="commit activity" />

`CachedResponse.body` is a plain `Vec<u8>` (src/http/cache.rs:81). serde has no bytes-transparent representation for `Vec<u8>`, so `serde_json::to_vec` at line 572 renders every byte as a decimal integer inside a JSON array. That blob is then handed to `Cache::set`, which base64-encodes it (src/cache.rs:81) and serializes the result into a *second* JSON document whose `value` field is that base64 string (src/cache.rs:42-49, 85). The read path runs the same chain in reverse: read file → parse outer JSON allocating the full base64 String → base64-decode → parse the inner JSON array integer by integer. I measured the exact pipeline against a 1 MiB JSON-API-shaped body on this machine (M3 Pro, release build): the on-disk entry is 4,821,095 bytes (4.60x), the write path costs 6.76 ms, and the read path — which is the *cache hit*, the operation whose entire purpose is to beat the network — costs 10.3 ms of pure CPU against a 21.5 µs memcpy baseline. On a LAN origin or a warm CDN edge, a cache hit is now slower than the request it replaced. The `body.clone()` at line 569 adds one more full copy on top, and it is unconditional: `persist_entry` already built a `CachedResponse` by cloning the body once (src/http/cache.rs:93), so a stored response is copied twice before encoding begins.

```rust src/http/cache.rs:561-584
    let entry = CachedHttpEntry {
        format_version: 1,
        policy: policy.clone(),
        response: CachedResponse {
            status: response.status,
            version: response.version,
            headers: response.headers.clone(),
            trailers: response.trailers.clone(),
            body: response.body.clone(),
        },
    };
    let encoded = match serde_json::to_vec(&entry) {
        Ok(encoded) => encoded,
        Err(error) => {
            tracing::warn!(key, error = %error, "failed to serialize HTTP cache entry");
            return;
        }
    };
    let ttl = policy
        .time_to_live(now)
        .saturating_add(client.config().http_cache_stale_retention);
    if let Err(error) = cache.set(&namespaced_key(key), &encoded, ttl) {
        tracing::warn!(key, error = %error, "failed to persist HTTP cache entry");
    }
```

Also at `src/http/cache.rs:75-82`, `src/cache.rs:79-86`, `src/cache.rs:42-49`.

Related: [cache-set-fsync-per-write](#cache-set-fsync-per-write).

**Remediation:** Stop routing binary bodies through JSON twice. The narrow fix is to give `CachedResponse.body` a bytes-aware serde representation (serde_bytes, or a manual `serialize_bytes`/`deserialize_bytes` impl) so the inner document stops emitting a decimal-integer array — that alone removes the 3.45x. The complete fix is to give `Cache` a bytes-native store: keep the JSON header for `expires_at` but write the payload as raw bytes after a length prefix or separator, so `Cache::set`/`get` never base64 anything. Base64 buys nothing here — the file is opaque, binary-safe, and already mode 0600. If the JSON envelope must stay for format stability, bump `format_version` and migrate. Finally, remove the second `body.clone()` at line 569 by having `persist_cached_response` take the `CachedResponse` by value. This needs the revalidation path reordered first: at src/http/cache.rs:484-485 `cached` is passed to the persist call and then moved into `fresh_response` on the next line, so a by-value signature fails to compile there (E0382). Either reorder those two statements or have the function hand the value back. The other call site (src/http/cache.rs:548) is already fine.

<div>&hairsp;</div>

### Async HTTP cache and update-check paths perform blocking filesystem I/O, including two fsyncs per write {#blocking-fsync-on-async-cache-paths}

**significant** · `src/http/cache.rs:421-444` · effort: small · <img src="assets/sparkline-blocking-fsync-on-async-cache-paths.svg" height="14" alt="commit activity" />

`HttpClient::get_cached` (src/http.rs:536) and `UpdateChecker::check` (src/update.rs:91) are public `async fn`s, but every cache interaction they reach is synchronous `std::fs`. `persist_entry` -> `persist_cached_response` -> `Cache::set` runs `std::fs::create_dir_all` (src/cache.rs:72) and then `write_entry` (src/cache.rs:178-190), which calls `file.sync_all()` on line 188 and `file.commit()` on line 189 — and `AtomicWriteFile::commit` itself calls `sync_all()` again before renaming (atomic-write-file 0.3.0 `src/lib.rs:611-618`), so each cache write issues two fsyncs where one would do. On macOS `File::sync_all` is a full `F_FULLFSYNC` barrier, routinely tens to hundreds of milliseconds. The read side is the same: `load_entry` -> `Cache::get` -> `std::fs::read` (src/cache.rs:101), and `UpdateChecker::check` calls `cache.get` at src/update.rs:99 and `cache.set` at src/update.rs:137. None of these are wrapped in `spawn_blocking`. This matters more than usual here because librebar's tokio feature set is `["rt", "macros", "signal", "sync", "time"]` with no `rt-multi-thread`, and the shipped `examples/service.rs` documents `#[tokio::main(flavor = "current_thread")]` as the intended shape — so the fsync stalls the process's only worker: timers stop, the shutdown signal task cannot be polled, and the whole-operation `tokio::time::timeout` wrapping `HttpClient::send` (src/http.rs:709) cannot fire while the thread is parked in the kernel. `UpdateChecker::check`'s own doc comment on src/update.rs:88 asserts "This is non-blocking and best-effort", which is not true of the cache read or the cache write.

```rust src/http/cache.rs:421-444
async fn fetch_and_maybe_store(
    client: &HttpClient,
    cache: &Cache,
    key: &str,
    wire_request: Request<Bytes>,
) -> Result<Response> {
    let request_for_policy = policy_request(&wire_request)?;
    let response = client.send(wire_request).await?;
    let response_time = SystemTime::now();
    let policy_response = response_head(&response)?;
    let policy = CachePolicy::new_options(
        &request_for_policy,
        &policy_response,
        response_time,
        private_cache_options(),
    );

    if policy.is_storable() {
        persist_entry(client, cache, key, &policy, &response, response_time);
    } else if let Err(error) = cache.remove(&namespaced_key(key)) {
        tracing::warn!(key, error = %error, "failed to remove non-storable HTTP cache entry");
    }
    Ok(response.with_cache_status(CacheStatus::Miss))
}
```

Also at `src/cache.rs:178-190`, `src/update.rs:88-104`.

Enables [signal-task-exits-after-first-signal](#signal-task-exits-after-first-signal).

Related: [cache-expiry-unlink-races-concurrent-write](#cache-expiry-unlink-races-concurrent-write).

**Remediation:** Move the filesystem work off the runtime thread — wrap the `Cache::get` / `Cache::set` / `Cache::remove` calls reached from `get_cached`, `fetch_and_maybe_store`, `revalidate`, and `UpdateChecker::check` in `tokio::task::spawn_blocking`, or expose an async cache trait so callers can plug in a non-blocking store. `spawn_blocking` is gated on tokio's `rt` feature alone (tokio-1.53.1/src/task/blocking.rs:82, inside `cfg_rt!`), which librebar already enables at Cargo.toml:50, and the blocking pool exists on a current_thread runtime — so this fix is additive and does not change the runtime flavor librebar requires. Independently, drop the redundant explicit `file.sync_all()` in `write_entry` since `AtomicWriteFile::_commit` already calls `sync_all()` before renaming (atomic-write-file-0.3.0/src/lib.rs:611-617), halving the stall. Correct the `check()` doc comment so it no longer claims to be non-blocking.

<div>&hairsp;</div>

### HTTP cache fingerprints three credential headers and writes every other request header to disk verbatim {#http-cache-persists-unrecognized-credential-headers}

**moderate** · `src/http/cache.rs:17-18` · effort: small · <img src="assets/sparkline-http-cache-persists-unrecognized-credential-headers.svg" height="14" alt="commit activity" />

policy_request clones the entire wire header map into the policy view
(cache.rs:374) and then calls fingerprint_credentials, which rewrites only the three
names above. http-cache-semantics stores the whole request header map inside CachePolicy
(http-cache-semantics-3.0.0/src/lib.rs:182 and :208, `let req = req.headers().clone()`), and
persist_cached_response serializes that CachePolicy into the cache entry JSON
(cache.rs:561-563), which cache.rs::set writes to
~/Library/Caches/{app}/librebar/v1-*.json. The three-name allowlist is the right idea and
is covered by a passing test (cache.rs:629-661) — but any caller using the documented
custom-header escape hatch with X-Api-Key, PRIVATE-TOKEN, X-Auth-Token, or similar has
that credential written to disk in cleartext, where it survives token rotation for the
entry's TTL plus the 7-day http_cache_stale_retention default. The file itself is
correctly created at 0o600 via AtomicWriteFile (cache.rs:178-190), so this is not a
same-host disclosure to other users; it is a persistence-of-secrets problem — the token
now appears in Time Machine and rsync backups, in container image layers, and in any
doctor bundle whose caller sweeps the cache directory. Related, and correctly handled:
Cache-Control: no-store and private are honored via is_storable with shared: false, URL
mismatch is caught by CachePolicy::request_matches (lib.rs:386) and evicts rather than
serving the wrong body, and base64 in the cache is used purely as a byte-safe encoding
for JSON, not as obfuscation. The one identity gap that remains is that get_cached's
caller-supplied key does not incorporate the requesting identity, so when an origin omits
Vary: Authorization, a response fetched under profile A is served to profile B under the
same key; the doc comment tells callers to encode "tenant, locale, or media-type" in the
key but does not mention credentials.

```rust src/http/cache.rs:17-18
const SENSITIVE_REQUEST_HEADERS: [HeaderName; 3] =
    [AUTHORIZATION, PROXY_AUTHORIZATION, hyper::header::COOKIE];
```

Also at `src/http/cache.rs:374-375`, `src/http/cache.rs:561-563`.

> The fingerprint accounts for three header names. Everything else in the request goes to disk verbatim, including whichever header actually carried the credential. A cache is a file that outlives the process that wrote it, which makes it the most patient place to leave a secret.

Related: [request-uri-with-credentials-recorded-in-log-spans](#request-uri-with-credentials-recorded-in-log-spans).

**Remediation:** Replace the fixed three-name array with a predicate: fingerprint any header whose name is
in the known set or matches a credential-shaped pattern
((?i)^(x-)?(api|auth|access|private|session)[-_]?(key|token)$, plus authentication,
x-amz-security-token, x-goog-iam-authorization-token). Safer still, invert the rule —
fingerprint every request header except a small allowlist that the caching policy
genuinely needs (accept, accept-encoding, accept-language, host, user-agent, range,
content-type, plus any name appearing in the response's Vary). Separately, extend the
get_cached doc comment to state that the key must incorporate the requesting identity
when the client sends credentials, since the policy only enforces that when the origin
sets Vary.

<div>&hairsp;</div>

### On-disk cache never prunes: expired entries are removed only when the same key is read again {#cache-has-no-eviction-outside-per-key-reads}

**moderate** · `src/cache.rs:99-126` · effort: medium · <img src="assets/sparkline-cache-has-no-eviction-outside-per-key-reads.svg" height="14" alt="commit activity" />

TTL is stored inside each entry file rather than in any index, so expiry can only be observed by opening that specific file. The sole automatic removal happens in `get()` when the caller asks for a key that has already expired. An entry that is written once and never requested again — because the key rotated, the URL changed, or the program simply stopped asking — is never revisited and remains on disk forever. A grep across `src/` for `prune`, `evict`, `sweep`, `purge`, `max_entries`, and `max_size` returns nothing: there is no background sweep, no entry-count ceiling, no byte ceiling, and no sweep-on-write. The only bulk removal is `clear()` (`src/cache.rs:147-161`), which deletes everything and must be called explicitly by the application. The `http-cache` feature makes this concrete rather than theoretical: `get_cached` keys entries by a caller-supplied string (`src/http.rs:536-543`), and its own documentation tells callers to fold tenant, locale, and media-type distinctions into that key — precisely the pattern that produces a high-cardinality, rotating key space. Every retired key leaves a file behind. Because `key_path` base64-encodes the key into the filename (`src/cache.rs:168-175`), orphans are not even human-identifiable during manual cleanup.

```rust src/cache.rs:99-126
pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    let path = self.key_path(key);
    let data = match std::fs::read(&path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CacheError::from(e).into()),
    };

    let entry: CacheEntry = serde_json::from_slice(&data).map_err(CacheError::Json)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if now >= entry.expires_at {
        tracing::debug!(key, "cache entry expired");
        // Best-effort cleanup: stale entry will be overwritten on next set().
        let _ = std::fs::remove_file(&path);
        return Ok(None);
    }

    let value = base64::engine::general_purpose::STANDARD
        .decode(&entry.value)
        .map_err(CacheError::from)?;

    Ok(Some(value))
}
```

Enables [http-cache-entry-body-amplification](#http-cache-entry-body-amplification).

**Remediation:** Give `Cache` an explicit eviction path and call it. A `prune()` method that walks the directory, reads each entry's `expires_at`, and unlinks expired files is the minimum; invoking it opportunistically from `set()` on a sampled basis (or after N writes) makes it self-maintaining without a background thread. Add configurable ceilings — maximum entry count and maximum total bytes — enforced on write with oldest-expiry-first eviction. Storing the expiry in the filename or in a small sidecar index would let a sweep run without opening and JSON-parsing every file. Document the retention behaviour on the `Cache` type so callers know the cache is currently grow-only.

<div>&hairsp;</div>

### Every cache write pays a full-drive fsync (~5 ms measured) for data the code already treats as disposable {#cache-set-fsync-per-write}

**moderate** · `src/cache.rs:178-190` · effort: small · <img src="assets/sparkline-cache-set-fsync-per-write.svg" height="14" alt="commit activity" />

`Cache::set` is the single write path for the `cache`, `http-cache`, and `update` features. It ends in an fsync, and then `AtomicWriteFile::commit()` performs a second one — commit's implementation is `self.sync_all()?; self.temporary_file.rename_file()` (atomic-write-file 0.3.0 lib.rs:611-618), so the explicit call at line 188 is redundant. Measured on this machine (APFS, 4 KiB payloads, 20 iterations): plain write, no sync = 60 µs/write; write + sync_all = 5.08 ms/write; write + rename with no fsync = 115 µs/write. That is ~44x, or ~5 ms of wall clock added to every `get_cached` miss, every 304 revalidation that refreshes an entry, and every `UpdateChecker::check` that stores a version — the last of which sits directly on CLI startup. The durability being bought is not needed: this is a cache, and the read path already handles torn or corrupt entries gracefully by discarding them (src/http/cache.rs:383-392 and 411-418 both log and delete). The code pays 5 ms per write to avoid a failure mode it explicitly recovers from. `CookieJar::save_to` has the same doubled fsync at src/http/cookies.rs:70-73.

```rust src/cache.rs:178-190
fn write_entry(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = AtomicWriteFile::options();
    #[cfg(unix)]
    {
        use atomic_write_file::unix::OpenOptionsExt as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).preserve_mode(false);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    file.commit()
}
```

Also at `src/http/cookies.rs:70-73`.

Related: [http-cache-entry-body-amplification](#http-cache-entry-body-amplification).

**Remediation:** Drop the explicit `file.sync_all()` at line 188 and at src/http/cookies.rs:70 — it is strictly redundant with `commit()`. For the remaining fsync, note that atomic-write-file 0.3.0 exposes no way to skip it (`OpenOptions` offers only `read` plus the unix `mode`/`preserve_mode` extensions), so removing it means writing to a temp file in the same directory and calling `std::fs::rename` directly. Rename is already atomic within a filesystem, which is the only property a cache needs; keep the 0600 mode via `OpenOptions::mode`. If durability is wanted for cookies specifically (a credential store, unlike a cache), keep the sync there and drop it only in `Cache`.

<div>&hairsp;</div>

### Six cache eviction results discarded with let _ =, while the same call is logged elsewhere {#http-cache-eviction-results-discarded}

**advisory** · `src/http/cache.rs:337-347` · effort: trivial · <img src="assets/sparkline-http-cache-eviction-results-discarded.svg" height="14" alt="commit activity" />

`Cache::remove` returns `Result<()>` and treats a missing file as success, so the
only errors it can produce are real ones — a read-only cache directory, permission
denial, an immutable file. Six call sites drop that result with a bare `let _ =`
and no explanatory comment: lines 343, 361, 385, 390, 415, and 490. Criterion 5.3
requires an explicit comment for an intentional discard, and the crate observes that
rule elsewhere — `src/cache.rs:116` and `src/cache.rs:155` both carry a
"Best-effort" comment.

Two call sites in the same file handle the identical call properly:
`src/http/cache.rs:440-441` and `:501-502` log `tracing::warn!("failed to remove
non-storable HTTP cache entry")`. Nothing distinguishes those from the six that do
not.

The consequence when eviction fails on a corrupt entry (line 415) is a stable loop:
every `get_cached` call reads the entry, warns "discarding corrupt HTTP cache
entry", fails to remove it, re-fetches over the network, and fails to overwrite it.
The cache is silently disabled while appearing to operate, and the log names the
corruption but never the reason it persists.

```rust src/http/cache.rs:337-347
match entry.policy.before_request(&policy_request, now) {
    BeforeRequest::Fresh(parts) => {
        match fresh_response(entry.response, &parts.headers, CacheStatus::Hit) {
            Ok(response) => Ok(response),
            Err(error) => {
                tracing::warn!(key, error = %error, "discarding corrupt HTTP cache entry");
                let _ = cache.remove(&namespaced_key(key));
                fetch_and_maybe_store(client, cache, key, wire_request).await
            }
        }
    }
```

Also at `src/http/cache.rs:360-363`, `src/http/cache.rs:383-392`, `src/http/cache.rs:413-417`, `src/http/cache.rs:488-492`, `src/http/cache.rs:438-443`.

Related: [update-check-drops-errors-it-documents-as-logged](#update-check-drops-errors-it-documents-as-logged).

**Remediation:** Route all six through the same `if let Err(error) = cache.remove(...) {
tracing::warn!(...) }` shape already used at lines 440 and 501 — a small helper such
as `evict(cache, key)` would make the treatment uniform and remove the duplication.
Where a discard really is intended, add the explanatory comment the crate's own
`src/cache.rs` sites use.

<div>&hairsp;</div>

### Expiry cleanup unlinks the cache path unconditionally, discarding a concurrent writer's fresh entry {#cache-expiry-unlink-races-concurrent-write}

**note** · `src/cache.rs:109-119` · effort: trivial · <img src="assets/sparkline-cache-expiry-unlink-races-concurrent-write.svg" height="14" alt="commit activity" />

`Cache::get` reads the entry at line 101, decides it is expired, and then unlinks the path. Writes go through `AtomicWriteFile` and land by `rename` (src/cache.rs:189), so the window between the read and the unlink is enough for another process — or another task in the same process — to commit a fresh entry at that path. The unlink names the path, not the inode that was read, so it deletes the replacement. The same shape recurs in the HTTP cache, which calls `cache.remove` on the corrupt-entry and non-storable paths (src/http/cache.rs:343, 361, 385, 390, 415, 440, 490, 501). Consequence is bounded — a cache miss and a redundant refetch, never corruption, since the writer's rename is atomic — which is why this is recorded rather than raised. Worth knowing because librebar's own `UpdateChecker` and `get_cached` are the two documented multi-process consumers of this cache, and both write to a shared per-user cache directory.

```rust src/cache.rs:109-119
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now >= entry.expires_at {
            tracing::debug!(key, "cache entry expired");
            // Best-effort cleanup: stale entry will be overwritten on next set().
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }
```

Related: [blocking-fsync-on-async-cache-paths](#blocking-fsync-on-async-cache-paths).

**Remediation:** Either drop the opportunistic unlink entirely and let the next `set()` overwrite the stale entry — which the comment already assumes happens — or make the delete conditional on the file still being the one that was read, by comparing the inode/mtime captured at read time before unlinking. Sweeping expired entries from `clear()` or a dedicated prune path avoids the race altogether.

<div>&hairsp;</div>

*Verdict: Two independent agents reached this code from different directions and found the same defect, which is the strongest corroboration in the audit. The body amplification is measurable and large — two serialization layers stacked on binary data, 4.6x on disk and roughly 10 ms of CPU per MiB retrieved. The credential-header persistence is the one to fix first, though: it is smaller, quieter, and writes authentication material to disk that the cache-key fingerprint does not account for. Eviction is the structural gap — nothing prunes the cache except a repeat read of the same key, so a cache of one-shot keys grows without bound and is never revisited.*

<div>&nbsp;</div>

---

## The Transport and Cookie Surface

*TLS itself is configured correctly and never weakened, but the two controls layered above it — cookie scoping and redirect handling — are each compiled in and then left unarmed.*

### Cookie jar compiles the public-suffix feature but never installs the list, accepting supercookies {#cookie-jar-never-installs-public-suffix-list}

**significant** · `src/http/cookies.rs:20-23` · effort: medium · <img src="assets/sparkline-cookie-jar-never-installs-public-suffix-list.svg" height="14" alt="commit activity" />

Cargo.toml:75 enables cookie_store's public_suffix feature, and HttpClientBuilder exposes
with_cookie_jar() and with_cookie_jar_from(). Both paths produce a CookieStore with no
suffix list: the derived Default calls CookieStore::default(), and load_from calls
cookie_store::serde::json::load, which routes to CookieStore::from_cookies. Both
constructors hardcode public_suffix_list: None (cookie_store-0.22.1/src/cookie_store.rs
462-472). librebar never calls with_suffix_list or new_with_public_suffix, so the RFC 6265
section 5.3 public-suffix rejection block at cookie_store.rs:267-289 is compiled but
dead — it is guarded by `if let Some(ref psl) = self.public_suffix_list`. The only
remaining check is `cookie.domain.matches(request_url)`, which a registrable-suffix
domain satisfies. Attacker: any host the client talks to under a shared public suffix.
A response from attacker.github.io carrying `Set-Cookie: session=x; Domain=.github.io` is
accepted into the jar, and CookieJar::request_header then attaches it to every subsequent
request to victim.github.io in the same jar. The same holds for .s3.amazonaws.com,
.herokuapp.com, .pages.dev, .co.uk, and any multi-tenant suffix. Result: cross-tenant
cookie injection and session fixation against every other host the CLI reaches. Secure,
HttpOnly, path, and expiry handling are correct — the gap is specifically the missing
suffix list. The persisted jar is written correctly at 0600 via AtomicWriteFile
(cookies.rs:38-44), so this is not a disclosure issue, it is an acceptance issue.

```rust src/http/cookies.rs:20-23
#[derive(Clone, Debug, Default)]
pub struct CookieJar {
    inner: Arc<RwLock<cookie_store::CookieStore>>,
}
```

Also at `src/http/cookies.rs:28-29`, `Cargo.toml:75-75`.

> Without the suffix list there is nothing between `example.co.uk` and `co.uk` except a string comparison that says one ends with the other. I set a cookie for the registry suffix and every site underneath it hands the cookie back to me. The feature flag that would have stopped this is enabled in your manifest — it just never got handed a list, so the check it was supposed to perform silently returns true.

Related: [cross-origin-redirect-forwards-non-blocklisted-credentials](#cross-origin-redirect-forwards-non-blocklisted-credentials).

**Remediation:** Install a suffix list at construction. Add publicsuffix (or psl) as a dependency of the
http-cookies feature, embed a list at build time, and construct via
CookieStore::new_with_public_suffix(Some(list)) in Default, and via
CookieStore::from_cookies(...).with_suffix_list(list) in load_from. If shipping an
embedded list is unacceptable for a foundation crate, drop the public_suffix feature from
the cookie_store dependency and document in HttpClientBuilder::with_cookie_jar that the
jar performs no public-suffix rejection, so callers do not assume a protection that is
not there.

<div>&hairsp;</div>

### Redirect follower strips only three header names and permits HTTPS-to-HTTP downgrade {#cross-origin-redirect-forwards-non-blocklisted-credentials}

**significant** · `src/http.rs:365-371` · effort: small · <img src="assets/sparkline-cross-origin-redirect-forwards-non-blocklisted-credentials.svg" height="14" alt="commit activity" />

RedirectPolicy delegates credential handling to tower-http's FilterCredentials::default().
That default blocks cross-origin hops (block_cross_origin: true) and, on a blocked hop,
removes exactly the three names in tower-http-0.7.0/src/follow_redirect/policy/
filter_credentials.rs:41-46: AUTHORIZATION, COOKIE, PROXY_AUTHORIZATION. Every other
header survives the hop verbatim, because on_request takes the `else if
self.remove_blocklisted` branch rather than headers.clear(). librebar's own module docs
(http.rs:36-51) advertise Request::builder().header(...) plus send() as the supported way
to set custom headers, so the credential a caller actually uses is very often outside that
blocklist: X-Api-Key, PRIVATE-TOKEN (GitLab), X-Auth-Token, Api-Key, X-Amz-Security-Token,
X-Hub-Signature. Second half: the connector is built with https_or_http() (http.rs:460),
and FilterCredentials always returns Action::Follow — it never refuses a hop. eq_origin
(tower-http .../policy/mod.rs:283-295) returns false when schemes differ, so an
https -> http redirect is classified as cross-origin, which correctly drops the three
blocklisted headers but still follows the redirect into cleartext. Concrete path: an
origin at https://api.example.com (compromised, or merely a redirector the attacker
controls a path on) answers 302 with `Location: http://api.example.com/x`. The client
follows, and the caller's X-Api-Key header is transmitted in plaintext to any on-path
observer. The same shape, pointed at a foreign host, leaks the key to the attacker's
server directly. This is the CVE-2018-1000007 pattern generalized past the standard
Authorization header. Note the rest of the redirect handling is solid: loop detection by
(method, uri) set, an enforced hop maximum, and body cloning are all present and correct.

```rust src/http.rs:365-371
#[derive(Clone, Debug)]
struct RedirectPolicy {
    maximum: usize,
    remaining: usize,
    visited: HashSet<(Method, hyper::Uri)>,
    credentials: FilterCredentials,
}
```

Also at `src/http.rs:406-408`, `src/http.rs:457-462`, `src/http.rs:36-51`.

> A blocklist of three header names is an invitation to use a fourth. Your API key does not travel in `Authorization` — it is in `X-Api-Key`, or `Api-Token`, or whatever the vendor chose. So I answer your request with a 302 to a host I control and read the header off the wire. If I would rather not bother with TLS at all, I redirect to `http://` instead, which you will also follow.

Related: [cookie-jar-never-installs-public-suffix-list](#cookie-jar-never-installs-public-suffix-list), [webpki-root-store-is-compiled-in](#webpki-root-store-is-compiled-in).

**Remediation:** Two changes. (1) Refuse scheme downgrades: in RedirectPolicy::redirect, compare
attempt.previous().scheme() to attempt.location().scheme() and return an error (add a
RedirectError::Downgrade variant alongside Loop and TooMany) when the previous hop was
https and the next is http. (2) Strip caller-supplied credentials on any blocked hop
rather than only the three known names — either call
FilterCredentials::default().remove_all() and re-add the client's own User-Agent in
on_request, or extend the removal with an explicit list of common API-key header names
plus anything matching (?i)^(x-)?(api|auth|access|private)[-_]?(key|token). Document
whichever policy is chosen on HttpClient::send, since callers currently have no way to
know custom headers cross origins.

<div>&hairsp;</div>

### Cookie jar read/write lock failures silently drop cookies from requests and responses {#cookie-jar-failures-are-silent}

**moderate** · `src/http/cookies.rs:77-91` · effort: small · <img src="assets/sparkline-cookie-jar-failures-are-silent.svg" height="14" alt="commit activity" />

Every read of the cookie jar on the request/response hot path converts failure into
"no cookies" with no error, no log, and no way for the caller to observe it:

`src/http/cookies.rs:79` — `self.inner.read().ok()?` on a poisoned `RwLock`. The
request goes out with no `Cookie` header. Because poisoning is permanent, one panic
while a writer held the lock silently un-authenticates every subsequent request for
the life of the process.

`src/http/cookies.rs:89` — if the joined cookie string is not a valid header value,
`.ok()` drops *all* cookies rather than the offending one.

`src/http/cookies.rs:112` — `if let Ok(mut store) = self.inner.write()` with no
`else`: a poisoned lock silently discards every `Set-Cookie` in the response, so a
login round-trip appears to succeed while storing nothing.

`src/http/cookies.rs:97` and `:176` — `if let Ok(url) = Url::parse(...)` with no
`else`: a URI that `url` cannot parse means cookies are neither sent nor stored,
silently.

`CookieJar::save_to` (line 50) does the opposite — it surfaces poisoning as
`HttpError::CookieJar`. So the jar reports the problem only at persistence time,
long after requests have been going out unauthenticated. The resulting failure mode
is a 401 from the origin with nothing in the logs pointing at the jar.

```rust src/http/cookies.rs:77-91
fn request_header(&self, url: &Url) -> Option<hyper::header::HeaderValue> {
    let value = {
        let store = self.inner.read().ok()?;
        store
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    };
    if value.is_empty() {
        None
    } else {
        hyper::header::HeaderValue::from_str(&value).ok()
    }
}
```

Also at `src/http/cookies.rs:104-115`, `src/http/cookies.rs:93-102`, `src/http/cookies.rs:167-181`.

**Remediation:** Recover from poisoning rather than treating it as empty: `RwLock::is_poisoned` plus
`into_inner()`/`PoisonError::into_inner()` gives back the store, which for a cookie
jar is the right call since a partially updated jar is still usable. At minimum emit
`tracing::warn!` on each of the four silent paths so the condition is observable,
and log the specific cookie that failed header encoding instead of dropping the
whole header.

<div>&hairsp;</div>

### Cookie jar enforces no per-domain or total cookie limit on origin-supplied Set-Cookie headers {#cookie-jar-accepts-unbounded-cookie-count}

**advisory** · `src/http/cookies.rs:104-115` · effort: small · <img src="assets/sparkline-cookie-jar-accepts-unbounded-cookie-count.svg" height="14" alt="commit activity" />

Every syntactically valid `Set-Cookie` header on every response is inserted into the shared store with no cap on cookie count, per-domain count, or aggregate size. The pinned cookie_store 0.22.1 does evict on expiry — its `insert` treats an already-expired incoming cookie as a deletion and drops stored cookies once expired — so this is not an expired-entry leak. What is unbounded is the population of live cookies: an origin that returns many distinct cookie names with distant expiries grows `Arc<RwLock<CookieStore>>` monotonically for the lifetime of the client, and `save_to` (`src/http/cookies.rs:36`) then persists all of them via `iter_unexpired()`, carrying the growth onto disk and back into memory on the next `load_from`. RFC 6265 §6.1 explicitly directs user agents to impose limits — commonly 50 cookies per domain, 3000 in total, 4096 bytes per cookie — and neither librebar nor cookie_store applies any of them. Per-response growth is bounded by hyper's header limits, so this is a slow accumulation rather than a single-response blowup; the exposure is a long-lived service holding one client across many requests to a hostile or merely careless origin, which is a use case the crate explicitly targets.

```rust src/http/cookies.rs:104-115
fn store_response(&self, url: &Url, response: &Response<ResponseBody>) {
    let cookies = response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| cookie_store::RawCookie::parse(value.to_owned()).ok())
        .map(cookie_store::RawCookie::into_owned);
    if let Ok(mut store) = self.inner.write() {
        store.store_response_cookies(cookies, url);
    }
}
```

> There is no cap per domain and none in total, and I control how many `Set-Cookie` headers my server returns. Every one of them is parsed, retained, and written back to your jar file on disk. I am not trying to steal anything here — I am just filling it.

**Remediation:** Apply RFC 6265 §6.1 ceilings in `store_response` before delegating to `store_response_cookies`: reject cookies whose name plus value exceeds the per-cookie byte limit, and enforce per-domain and total counts by evicting the least-recently-used or nearest-to-expiry entries once a threshold is crossed. Surface the limits on `HttpClientBuilder` so callers who need larger jars can raise them deliberately. Emit a `tracing::warn!` when a cookie is dropped for exceeding a limit so the behaviour is diagnosable rather than silent.

<div>&hairsp;</div>

### TLS trust anchors are compiled into the binary and only refresh on a dependency bump {#webpki-root-store-is-compiled-in}

**note** · `Cargo.toml:65-65` · effort: trivial · <img src="assets/sparkline-webpki-root-store-is-compiled-in.svg" height="14" alt="commit activity" />

Recording the TLS posture, which is otherwise tight, plus one operational caveat.
Verified clean: build_inner calls with_provider_and_webpki_roots, which resolves to
ClientConfig::builder_with_provider(...).with_safe_default_protocol_versions()
.with_webpki_roots().with_no_client_auth() (hyper-rustls-0.27.9/src/connector/builder.rs
167-177). That means real certificate verification against the Mozilla root program,
TLS 1.2 and 1.3 only with no 1.0/1.1 fallback, and no client auth. A repo-wide grep for
dangerous(), ServerCertVerifier, set_certificate_verifier, danger_accept_invalid_certs,
and insecure returns nothing across src, tests, and examples — verification is never
disabled, not even behind a test-only flag. The crate is unsafe_code = "deny" with zero
unsafe and no FFI, and cargo audit is clean. The caveat: webpki-tokio bakes the root store
into the binary at compile time. A librebar-based CLI shipped as a static binary keeps
trusting whatever roots were current when it was built — it will not pick up a root
distrusted by Mozilla after ship, and it will not learn a newly added root, so a
long-lived installed binary can start failing valid connections or keep trusting a
withdrawn CA. That is a deliberate trade (no system OpenSSL dependency, as the module doc
at http.rs:4 says) but it belongs in the consuming project's release cadence, not just in
librebar's. Separately, https_or_http() means plain http:// URLs are permitted; that is
the correct default for a general-purpose client, but it is what makes the downgrade half
of cross-origin-redirect-forwards-non-blocklisted-credentials reachable.

```toml Cargo.toml:65-65
hyper-rustls = { version = "0.27", features = ["http1", "http2", "ring", "webpki-tokio"], default-features = false, optional = true }
```

Also at `src/http.rs:457-459`.

Related: [cross-origin-redirect-forwards-non-blocklisted-credentials](#cross-origin-redirect-forwards-non-blocklisted-credentials).

**Remediation:** No code change required. Document the trade-off in the http module docs: state that trust
anchors are compiled in, that consumers should rebuild and re-release on webpki-roots
updates, and that librebar will bump hyper-rustls promptly when the root program changes.
If some consumers need OS trust instead, add an optional feature that swaps
with_provider_and_webpki_roots for rustls-platform-verifier behind a
with_platform_verifier() builder method — this is exactly the pluggability-first shape the
rest of the crate follows, and it keeps webpki as the default.

<div>&hairsp;</div>

*Verdict: Certificate verification is never disabled anywhere in this crate, and that deserves saying plainly. The failures here are one level up. `cookie_store` is built with its `public_suffix` feature enabled and the list is then never installed, so the crate pays the dependency cost for supercookie protection it does not receive. The redirect follower strips a three-name blocklist rather than dropping credentials on cross-origin hops, and permits an HTTPS-to-HTTP downgrade — the shape of curl's CVE-2018-1000007. Both are small, self-contained fixes to code that already exists.*

<div>&nbsp;</div>

---

## The Process Lifecycle Surface

*The crash hook, the signal task, and the shutdown path each hold a failure mode that only appears when something has already gone wrong.*

### Panic hook prints with eprintln!, converting any panic into SIGABRT when stderr is broken {#crash-hook-print-turns-panics-into-aborts}

**significant** · `src/crash.rs:101-113` · effort: trivial · <img src="assets/sparkline-crash-hook-print-turns-panics-into-aborts.svg" height="14" alt="commit activity" />

`crash` is a default feature and `install()` replaces the process panic hook. The
hook writes its user-facing line with `eprintln!`, which panics on write failure
("failed printing to stderr") rather than ignoring it. Rust's own default hook
ignores stderr write errors, so replacing it with this one changes how the process
dies whenever stderr is not writable — a broken pipe (`myapp 2>&1 | head`, a
supervisor that closed the pipe), a closed descriptor, or ENOSPC. A panic raised
inside a panic hook is a double panic, which aborts immediately: the crash message
is lost, `prev_hook(panic_info)` on line 112 never runs so the standard panic
message and backtrace are never printed, and the process terminates with SIGABRT
instead of the normal panic exit.

Verified empirically on this machine (rustc 1.97.1, aarch64-apple-darwin) with two
minimal programs whose stderr is a fifo with no reader: a plain `panic!()` with the
default hook exits 101; the same panic with this hook's shape installed dies with
"Abort trap: 6", exit status 134. Every consumer of librebar that calls
`.crash_handler()` inherits that exit-status change, which is what CI runners,
systemd `Restart=` policies, and process supervisors branch on.

The crash dump itself is written before the `eprintln!`, so the dump survives; what
is lost is the message, the chained default hook, and the exit code.

```rust src/crash.rs:101-113
    let dump_dir = crash_dump_dir(&app_name);
    if let Some(path) = write_crash_dump_to(&info, &dump_dir) {
        eprintln!(
            "\n{} crashed. Crash report written to: {}\n",
            app_name,
            path.display()
        );
    } else {
        eprintln!("\n{} crashed. (Could not write crash report.)\n", app_name);
    }

    prev_hook(panic_info);
}));
```

Related: [crash-dump-world-readable-and-unbounded](#crash-dump-world-readable-and-unbounded), [print-macros-panic-where-errors-cannot-propagate](#print-macros-panic-where-errors-cannot-propagate).

**Remediation:** Never use `eprintln!`/`println!` inside a panic hook. Write through a fallible path
and discard the error explicitly, e.g. `let _ = writeln!(std::io::stderr(), ...)`
with `use std::io::Write`, matching what the std default hook does. Consider also
wrapping the whole hook body in `std::panic::catch_unwind` (or at minimum keeping
it allocation-light) so that a panic anywhere inside the dump path cannot escalate
to abort, and always call `prev_hook(panic_info)` even when the librebar-specific
work fails.

<div>&hairsp;</div>

### Signal task handles exactly one signal, leaving the process permanently un-interruptible {#signal-task-exits-after-first-signal}

**significant** · `src/shutdown.rs:69-96` · effort: small · <img src="assets/sparkline-signal-task-exits-after-first-signal.svg" height="14" alt="commit activity" />

The spawned task awaits a single `tokio::select!` and then returns. Both signal registrations are permanent at the OS level and are never restored: tokio's own documentation states "Once a signal handler is registered with the process the underlying libc signal handler is never unregistered" (tokio-1.53.1 `src/signal/unix.rs:383-384`) and, for SIGINT specifically, "Even if this `Signal` instance is dropped, subsequent `SIGINT` deliveries will end up captured by Tokio, and the default platform behavior will NOT be reset" (`src/signal/ctrl_c.rs:29-31`). So once this task exits, the kernel's default terminate-on-SIGINT/SIGTERM action is gone and nothing in the process is listening: every subsequent Ctrl-C and every subsequent `kill` is captured by tokio's global registry and delivered nowhere. If the application's own drain does not finish — and librebar supplies no bounded drain window, `ShutdownToken::cancelled` (src/shutdown.rs:117) has no deadline and the `App` exposes no "wait for shutdown with timeout" entry point — the operator's only remaining option is SIGKILL. There is no escalation path: no "second signal exits immediately", no forced-exit timer, and no documentation warning that the usual Ctrl-C escape hatch has been taken away. This is squarely the failure mode a graceful-shutdown module exists to prevent.

```rust src/shutdown.rs:69-96
    pub fn register_signals(&self) -> crate::Result<()> {
        let runtime = tokio::runtime::Handle::try_current().map_err(crate::Error::NoRuntime)?;

        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(crate::Error::ShutdownInit)?;

        let handle = self.clone();

        tracing::debug!("registering shutdown signal handlers");
        runtime.spawn(async move {
            let ctrl_c = tokio::signal::ctrl_c();

            #[cfg(unix)]
            tokio::select! {
                _ = ctrl_c => {},
                _ = sigterm.recv() => {},
            }

            #[cfg(not(unix))]
            ctrl_c.await.ok();

            tracing::info!("shutdown signal received");
            handle.shutdown();
        });

        Ok(())
    }
```

> The interesting property is not that the first Ctrl-C works. It is that the second one does not, and neither does the third. If I can arrange for the first signal to arrive while the process is doing something it will not finish, you are left holding a terminal that has stopped answering, and the only way out is a signal your handler never installed.

Enabled by [blocking-fsync-on-async-cache-paths](#blocking-fsync-on-async-cache-paths).

Related: [ctrl-c-registration-error-triggers-shutdown](#ctrl-c-registration-error-triggers-shutdown).

**Remediation:** Keep the signal task alive: loop on the select rather than returning after the first delivery, and on a second SIGINT/SIGTERM escalate — log a warning and `std::process::exit` with the conventional 128+signo code. Pair that with an opt-in bounded drain (a `ShutdownHandle::wait_with_timeout(Duration)` or a documented forced-exit deadline started when shutdown is first triggered), and state in the module docs that registering handlers permanently overrides the platform default so callers know what they are trading away.

<div>&hairsp;</div>

### A failed ctrl_c handler registration is discarded and read as a received signal {#ctrl-c-registration-error-triggers-shutdown}

**moderate** · `src/shutdown.rs:79-93` · effort: trivial · <img src="assets/sparkline-ctrl-c-registration-error-triggers-shutdown.svg" height="14" alt="commit activity" />

`tokio::signal::ctrl_c()` is `async fn ctrl_c() -> io::Result<()>` whose body is `os_impl::ctrl_c()?.recv().await` (tokio-1.53.1 `src/signal/ctrl_c.rs:59-62`) — the `?` means a handler-registration failure returns `Err` on the very first poll, without ever waiting for a signal. Both arms here discard that value: the `select!` pattern `_ = ctrl_c => {}` binds and drops the `Result`, and the non-unix path calls `.ok()`. So if SIGINT registration fails — the documented causes are a lower-level C call failing or a previous initialization of that signal having failed, which is what a restricted seccomp/sandbox profile or a conflicting handler produces — the branch completes instantly, the task logs `"shutdown signal received"`, and `handle.shutdown()` fires milliseconds after startup. A long-running service shuts itself down at boot, and the one log line emitted names a cause that did not happen, so the operator has nothing to go on. Note the contrast with the SIGTERM path on line 73, which correctly propagates its registration failure as `Error::ShutdownInit` before anything is spawned.

```rust src/shutdown.rs:79-93
        runtime.spawn(async move {
            let ctrl_c = tokio::signal::ctrl_c();

            #[cfg(unix)]
            tokio::select! {
                _ = ctrl_c => {},
                _ = sigterm.recv() => {},
            }

            #[cfg(not(unix))]
            ctrl_c.await.ok();

            tracing::info!("shutdown signal received");
            handle.shutdown();
        });
```

Related: [signal-task-exits-after-first-signal](#signal-task-exits-after-first-signal).

**Remediation:** Register SIGINT eagerly alongside SIGTERM, before the spawn, using `tokio::signal::unix::signal(SignalKind::interrupt())` and the same `map_err(crate::Error::ShutdownInit)?` treatment, so a registration failure surfaces synchronously from `register_signals` instead of being reinterpreted as a signal. Where a `Result` must still be awaited, match on it and log the error rather than triggering shutdown.

<div>&hairsp;</div>

### eprintln!/println! used inside Drop, a tracing Layer callback, and a fallible startup path {#print-macros-panic-where-errors-cannot-propagate}

**moderate** · `src/otel.rs:100-106` · effort: trivial · <img src="assets/sparkline-print-macros-panic-where-errors-cannot-propagate.svg" height="14" alt="commit activity" />

`eprintln!` and `println!` panic when the underlying write fails (confirmed
empirically: a stderr write to a reader-less fifo returns `BrokenPipe`, and
`catch_unwind` around `eprintln!` reports PANICKED). librebar uses them in three
places where a panic is worse than a lost message:

`src/otel.rs:100-106` — `OtelGuard::drop` prints on shutdown failure. `App` owns
`_otel_guard`, so this `Drop` runs while the stack unwinds from any panic in the
consumer's `main`. A panic in `Drop` during unwinding aborts the process (criterion
11.3), turning a diagnostic message into a crash.

`src/logging.rs:411-419` — `JsonLogLayer::on_event` prints to stderr when the log
sink write fails. The comment correctly notes that Layer callbacks cannot return
errors, but the chosen fallback can panic. This fires from inside `tracing::event!`,
i.e. potentially from anywhere — including `LockGuard::drop` at
`src/lockfile.rs:143-147`, which logs at debug level. Broken log sink plus a drop
during unwinding is the same abort. It is also unbounded: one stderr line per event
for as long as the sink stays broken.

`src/cli.rs:242-253` — `CommonArgs::apply` returns `std::io::Result<Startup>` yet
emits `--version-only` with `println!`, so a stdout write failure panics instead of
being returned through the `io::Result` the signature already promises.

```rust src/otel.rs:100-106
impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(e) = self.provider.shutdown() {
            eprintln!("Error shutting down tracer provider: {e}");
        }
    }
}
```

Also at `src/logging.rs:411-419`, `src/cli.rs:242-253`, `src/lockfile.rs:143-147`.

Related: [crash-hook-print-turns-panics-into-aborts](#crash-hook-print-turns-panics-into-aborts).

**Remediation:** Replace all three with explicit fallible writes. In `Drop` and in `on_event`, use
`let _ = writeln!(std::io::stderr(), ...)` so a failed diagnostic stays a failed
diagnostic. In `CommonArgs::apply`, use `writeln!(std::io::stdout(), "{version}")?`
so the already-declared `io::Result` carries the error. For `on_event`, consider
latching the "sink is broken" state so the fallback prints once rather than per
event.

<div>&hairsp;</div>

### wait_to_retry decrements an unsigned counter without checking it, invariant undocumented {#retry-counter-decrement-relies-on-caller-invariant}

**note** · `src/http.rs:896-902` · effort: trivial · <img src="assets/sparkline-retry-counter-decrement-relies-on-caller-invariant.svg" height="14" alt="commit activity" />

All three current call sites guard correctly — `src/http.rs:662`, `:681`, and `:699`
each test `remaining > 0` before calling — so this cannot underflow today. It is
recorded as a note because the invariant lives entirely in the callers and is stated
nowhere: no doc comment, no `debug_assert!`, no name that implies it.

The failure mode if a fourth call site is added without the guard is
version-dependent and unpleasant in the version that matters: debug builds panic on
overflow, release builds wrap `remaining` to `usize::MAX` and retry until
`config.timeout` fires. A silent switch from "three retries" to "retry for the full
30-second timeout" is precisely the kind of behavior change that survives review.

This is the only reachable arithmetic in `src/` whose safety is not locally
evident. The seven `expect()` calls in non-test library code — `src/cli/parse.rs:99` (clap
`required(true)` at line 148), `src/config.rs:541` and `:551` (target coerced to an
object two lines earlier), `src/config/environment.rs:79` (keys filtered by prefix
at line 49), `src/http.rs:908` (`is::<RedirectError>()` checked at line 905),
`src/http/cache.rs:188` (hex digest) and `:267` (`to_str()` already validated the
byte set) — are all provably infallible with the invariant stated in the expect
message. There are no `panic!`, `todo!`, `unimplemented!`, `unreachable!`, or
variable-index slice accesses in non-test `src/` code, no bare `.unwrap()`
outside `#[cfg(test)]` blocks. `config::deep_merge` carries an explicit depth limit
of 64 (`MERGE_DEPTH_LIMIT` at src/config.rs:151, enforced at :163); `config::set_path`
has no depth constant at all — its recursion is bounded only by the caller-supplied
path length.

```rust src/http.rs:896-902
async fn wait_to_retry(remaining: &mut usize, next_delay: &mut Duration) {
    *remaining -= 1;
    let delay = *next_delay;
    *next_delay = next_delay.saturating_mul(2).min(Duration::from_secs(1));
    tracing::debug!(?delay, remaining = *remaining, "retrying HTTP request");
    tokio::time::sleep(delay).await;
}
```

**Remediation:** Take the guard inside the function — `*remaining = remaining.saturating_sub(1)`, or
have `wait_to_retry` return `bool` and own the `remaining > 0` decision so callers
cannot get it wrong. Either way add a one-line doc comment stating the precondition.

<div>&hairsp;</div>

*Verdict: This is the surface where the defects are hardest to notice and most expensive when they land, because every one of them is reached only on a path the developer never exercises. A panic hook that prints with `eprintln!` converts a panic into `SIGABRT` whenever stderr is closed — a pipe that exited, a daemonized process — turning a recoverable unwind into an abort with no message. The signal task handles exactly one signal and then exits, leaving the process permanently un-interruptible afterward, which is the finding most likely to be experienced as a hang by a real user. None of these are exotic; all of them require deliberate testing to see.*

<div>&nbsp;</div>

---

## The Telemetry Surface

*The OTLP pipeline is assembled from parts that do not fit each other, and the configuration surface around it advertises knobs that nothing reads.*

### OTLP span export is wired to a batch processor that cannot drive the selected hyper HTTP client {#otel-batch-processor-cannot-drive-hyper-exporter}

**significant** · `src/otel.rs:139-153` · effort: medium · <img src="assets/sparkline-otel-batch-processor-cannot-drive-hyper-exporter.svg" height="14" alt="commit activity" />

`with_batch_exporter` builds the default `BatchSpanProcessor`, which opentelemetry_sdk 0.32.1 runs on a plain OS thread (`src/trace/span_processor.rs:365` `thread::Builder::new().spawn(...)`) and drives with `futures_executor::block_on(export)` (`:563`) — a thread with no Tokio runtime attached. Cargo.toml line 55 pins opentelemetry-otlp with `default-features = false, features = ["http-proto", "hyper-client", "trace"]`, and Cargo.lock contains no reqwest, so the exporter's client-selection ladder (opentelemetry-otlp-0.32.0 `src/exporter/http/mod.rs:242-246`) resolves to `HyperClient::with_default_connector`. That client's `send_bytes` (opentelemetry-http-0.32.0 `src/lib.rs:202`) calls `tokio::time::timeout(...)`, which eagerly constructs a `Sleep` (tokio-1.53.1 `src/time/timeout.rs:94`) whose `Sleep::new_timeout` calls `scheduler::Handle::current()` (`src/time/sleep.rs:254`) and panics when no runtime is present. The upstream `BatchSpanProcessor` doc comment states this combination outright: "async HTTP clients like `reqwest-client` and `hyper-client` are not supported by this default processor." Net effect with `otel` enabled and `OTEL_EXPORTER_OTLP_ENDPOINT` set: the first scheduled export (default 5 s in) panics the `OpenTelemetry.Traces.BatchProcessor` thread, the span channel disconnects, and no span is ever delivered — while the application keeps running and reports nothing wrong. Because `crash::install` registers a process-global panic hook (src/crash.rs:84) and `crash` is in the default feature set, that panic also writes a bogus `.crash` dump and prints "{app} crashed" to stderr for a process that did not crash. `OtelGuard::drop` (src/otel.rs:100-106) then reports "Error shutting down tracer provider". The `rt-tokio` feature on opentelemetry_sdk (Cargo.toml:54) is enabled but referenced nowhere in `src/`, and it would not be enough on its own: it expands to `["tokio/rt", "tokio/time", "tokio-stream", "experimental_async_runtime"]` and supplies the `runtime::Tokio` adapter only. The runtime-driven `BatchSpanProcessorWithAsyncRuntime` that would work here sits behind a different feature, `experimental_trace_batch_span_processor_with_async_runtime` (opentelemetry_sdk-0.32.1 `src/trace/mod.rs:19-21`), which `rt-tokio` does not turn on.

```rust src/otel.rs:139-153
    let exporter = build_exporter(endpoint, &protocol)?;

    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    // TracerProvider trait must be in scope for .tracer()
    use opentelemetry::trace::TracerProvider as _;
    let tracer = provider.tracer(cfg.service.clone());

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let boxed: BoxedLayer = Box::new(layer);
    Ok((Some(boxed), Some(OtelGuard { provider })))
```

Also at `Cargo.toml:53-55`.

**Remediation:** Pick a processor that matches the exporter's runtime requirements. Either keep the dedicated-thread `BatchSpanProcessor` and hand it an explicitly constructed blocking HTTP client, or switch to the async-runtime batch span processor so the hyper client runs inside the Tokio runtime it needs. The second branch costs more than it looks: `BatchSpanProcessorWithAsyncRuntime` requires adding `experimental_trace_batch_span_processor_with_async_runtime` to the opentelemetry_sdk features at Cargo.toml:54, since the already-enabled `rt-tokio` supplies only the `runtime::Tokio` adapter — and that feature is upstream-experimental, which argues for the blocking-client option. If the async-runtime processor is chosen, `build_otel_layer` must document that it has to be called from within a runtime, and `OtelGuard`'s blocking `provider.shutdown()` must move off the runtime thread or be replaced by an explicit async shutdown entry point — dropping `App` inside `async fn main` on the `flavor = "current_thread"` runtime that `examples/service.rs` prescribes would otherwise block the sole worker for up to the 5 s shutdown timeout. Add an integration test that stands up a local OTLP/HTTP receiver and asserts a span actually arrives; the current test suite never exercises export.

<div>&hairsp;</div>

### `OTEL_EXPORTER_OTLP_PROTOCOL=http/json` is documented but the `http-json` exporter feature is never enabled {#otel-http-json-protocol-not-buildable}

**moderate** · `src/otel.rs:156-175` · effort: small · <img src="assets/sparkline-otel-http-json-protocol-not-buildable.svg" height="14" alt="commit activity" />

src/otel.rs:24 documents `OTEL_EXPORTER_OTLP_PROTOCOL` as accepting "`http/protobuf` (default), `http/json`, or `grpc`", and the comment at src/otel.rs:168 repeats that `http/json` is served by the fallback arm. It is not. Cargo.toml:55 declares `opentelemetry-otlp = { version = "0.32", default-features = false, features = ["http-proto", "hyper-client", "trace"], optional = true }` — `http-json` is a distinct feature in opentelemetry-otlp 0.32 (verified in the vendored manifest: `http-json = ["serde_json", "prost", "opentelemetry-http", "opentelemetry-proto/gen-tonic-messages", "opentelemetry-proto/with-serde", "http", "trace", "metrics"]`) and it is not enabled. In the upstream source the JSON codec arm `crate::Protocol::HttpJson => self.with_http().build()` at span.rs:90-91 is itself `#[cfg(feature = "http-json")]`. Because librebar calls `.with_http()` explicitly, it lands directly on `SpanExporterBuilder<HttpExporterBuilderSet>::build` (span.rs:106-113), which never consults `OTEL_EXPORTER_OTLP_PROTOCOL` at all. The result is silent rather than loud: an operator who sets `http/json` because their collector expects it gets a protobuf body on the wire with no error, no warning, and no log line. This is the same class of gap as the `grpc` arm — which is handled correctly, gated on the `otel-grpc` feature that pulls `opentelemetry-otlp/grpc-tonic` — so the pattern to follow already exists two lines above.

```rust src/otel.rs:156-175
/// Build the span exporter based on the protocol string.
fn build_exporter(endpoint: &str, protocol: &str) -> Result<opentelemetry_otlp::SpanExporter> {
    use opentelemetry_otlp::WithExportConfig as _;

    match protocol {
        #[cfg(feature = "otel-grpc")]
        "grpc" => opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .map_err(crate::Error::OtelInit),

        // http/protobuf, http/json, or anything else — use HTTP transport
        _ => opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()
            .map_err(crate::Error::OtelInit),
    }
}
```

Also at `src/otel.rs:24-24`, `Cargo.toml:55-55`.

Related: [otel-config-env-var-name-fields-unread](#otel-config-env-var-name-fields-unread), [otel-grpc-feature-has-no-test](#otel-grpc-feature-has-no-test).

**Remediation:** Mirror the `otel-grpc` design: add an `otel-http-json` feature that enables `opentelemetry-otlp/http-json`, and add a `#[cfg(feature = "otel-http-json")] "http/json" =>` arm that selects the JSON codec. Document the new feature in the README and src/lib.rs feature tables alongside `otel-grpc`. If shipping the JSON codec is not wanted, the alternative is to make the mismatch visible rather than silent — reject or warn on an unrecognised protocol string instead of falling through — but per this project's "documentation wins" rule the default choice is to implement what src/otel.rs:24 already promises.

<div>&hairsp;</div>

### Every log event clones the complete field map of every enclosing span, then immediately destroys the clone {#log-event-clones-span-field-map}

**moderate** · `src/logging.rs:399-409` · effort: trivial · <img src="assets/sparkline-log-event-clones-span-field-map.svg" height="14" alt="commit activity" />

`SpanFields.values` is a `serde_json::Map`, i.e. a `BTreeMap<String, Value>`. Cloning it allocates a fresh BTreeMap node arena and deep-clones every key `String` and every `Value`; `extend` then walks that new map, moves each pair into `map`, and drops the clone. The intermediate map is pure waste. The cost scales as (events) x (span depth) x (fields per span): for a single event inside one `#[tracing::instrument]` span carrying three fields, that is one BTreeMap allocation plus six String allocations that exist for the duration of one `extend` call. librebar instruments its own hot paths this way — `HttpClient::send` opens a span with `method` and `url` (src/http.rs:639-642) and then emits `tracing::debug!` events inside it (src/http.rs:660, 900) — so a service running at debug level pays this per request per retry. The event path is reached only after filtering, so no `format!` is evaluated for disabled levels; the cost here is entirely the redundant clone. `JsonVisitor` compounds it by calling `field.name().to_string()` on every field (src/logging.rs:439, 444, 449, 455, 461, 475) even though `Field::name()` returns `&'static str`.

```rust src/logging.rs:399-409
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if let Some(fields) = span.extensions().get::<SpanFields>() {
                    map.extend(fields.values.clone());
                }
            }
        }

        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);
        map.extend(visitor.values);
```

**Remediation:** Replace `map.extend(fields.values.clone())` with a borrowing loop that clones only what is inserted — `for (key, value) in &fields.values { map.insert(key.clone(), value.clone()); }` — which removes the intermediate BTreeMap entirely for a one-line change. For the visitor, the `&'static str` field names can be kept as-is only if the map key type stays `String`; if this path is measured to matter, serializing directly to the output buffer with `serde_json::Serializer` instead of materializing a `Value::Object` removes both the map and the per-field key allocations.

<div>&hairsp;</div>

### `OtelConfig::env_var_protocol` and `env_var_endpoint` are public, documented as configuration, and read by no code path {#otel-config-env-var-name-fields-unread}

**advisory** · `src/otel.rs:45-51` · effort: trivial · <img src="assets/sparkline-otel-config-env-var-name-fields-unread.svg" height="14" alt="commit activity" />

A `grep -rn 'env_var_protocol|env_var_endpoint|env_var_env' src/ tests/ examples/` shows the asymmetry precisely. `env_var_env` is real: `from_app_name` builds it at src/otel.rs:60 and reads it at src/otel.rs:66. The other two are write-only. `from_app_name` assigns the literals `"OTEL_EXPORTER_OTLP_ENDPOINT"` and `"OTEL_EXPORTER_OTLP_PROTOCOL"` at src/otel.rs:76-77, and no code ever reads the fields back — `build_otel_layer` calls `std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL")` against a hardcoded string literal at src/otel.rs:135, and the endpoint is read from the literal at src/otel.rs:62 before the struct exists. Because `OtelConfig` is a plain public struct with public fields and a `#[derive(Clone, Debug)]`, the obvious reading of the doc comment "Env var name for the OTLP protocol" is that assigning it changes which variable is consulted — a vendored CLI that wants `MYAPP_OTLP_PROTOCOL` rather than the global OTEL name would reasonably try it. Nothing happens, silently. tests/otel_test.rs:42-44 asserts the field *values* but never that setting them changes behavior, so the test suite locks in the appearance of configurability without the substance.

```rust src/otel.rs:45-51
    /// Env var name for the OTLP endpoint.
    pub env_var_endpoint: String,
    /// Env var name for the OTLP protocol.
    pub env_var_protocol: String,
    /// Env var name for the deployment environment (e.g., `MY_TOOL_ENV`).
    pub env_var_env: String,
}
```

Also at `src/otel.rs:135-139`, `src/otel.rs:62-64`.

Related: [otel-http-json-protocol-not-buildable](#otel-http-json-protocol-not-buildable).

**Remediation:** Make `build_otel_layer` read `cfg.env_var_protocol` instead of the hardcoded literal at src/otel.rs:135, and have `from_app_name` (or a `with_env_var_protocol` setter) resolve the endpoint through `cfg.env_var_endpoint` so both fields behave the way their doc comments read. Add a test that overrides one of them and asserts the alternate variable is honoured. If per-app override is not wanted, the fields should not be `pub` — demote them to associated constants or private state so the API stops advertising a knob that does nothing.

<div>&hairsp;</div>

### `otel-grpc` is the only one of the 19 features with no integration test exercising it {#otel-grpc-feature-has-no-test}

**note** · `src/otel.rs:160-166` · effort: trivial · <img src="assets/sparkline-otel-grpc-feature-has-no-test.svg" height="14" alt="commit activity" />

Mapping the 19 tests/ files against the 19 features leaves exactly one hole. Seventeen named features have a dedicated file gated on themselves — bench, bench-gungraun, cache, cli, config, crash, diagnostics, dispatch, http, http-cache, http-cookies, lockfile, logging, mcp, otel, shutdown, update — and the eighteenth key, `default`, is covered by the ungated tests/default_features_test.rs; that accounts for 18 of the 19. (tests/builder_test.rs is a feature-combination file for cli+config+logging, not a feature's own test.) `otel-grpc` has none. tests/otel_test.rs carries `#![cfg(feature = "otel")]` on line 2, under `#![allow(missing_docs, unsafe_code)]` on line 1, so it compiles identically whether or not `otel-grpc` is on, and its only mention of the protocol is `std::env::remove_var("OTEL_EXPORTER_OTLP_PROTOCOL")` at line 23 and an equality assertion on the field name at line 43 — neither reaches the `"grpc"` arm. The practical exposure is modest, since `.crustoleum/feature-matrix.txt` records `otel-grpc PASS` under `cargo check --no-default-features --features otel-grpc` and the tonic transport is upstream's code, but the arm's guard condition (that the literal string `"grpc"` is what `OTEL_EXPORTER_OTLP_PROTOCOL` must be set to) is librebar's own contract and is currently unverified — a typo there would be caught by nothing.

```rust src/otel.rs:160-166
match protocol {
    #[cfg(feature = "otel-grpc")]
    "grpc" => opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(crate::Error::OtelInit),
```

Also at `tests/otel_test.rs:1-2`.

Related: [otel-http-json-protocol-not-buildable](#otel-http-json-protocol-not-buildable).

**Remediation:** Add a `#![cfg(feature = "otel-grpc")]` test file that sets `OTEL_EXPORTER_OTLP_PROTOCOL=grpc` alongside an endpoint and asserts `build_otel_layer` returns `Some(layer)` without error, mirroring the shape of the existing tests/otel_test.rs cases. This also gives the `http/json` fix above a natural place to land its own coverage.

<div>&hairsp;</div>

### The log-directory writability probe creates a file the appender never writes to, leaving a zero-byte file behind on every run {#log-writability-probe-creates-unused-file}

**note** · `src/logging.rs:309-322` · effort: trivial · <img src="assets/sparkline-log-writability-probe-creates-unused-file.svg" height="14" alt="commit activity" />

`resolve_log_target_with` probes candidate directories by creating them and opening `{service}{LOG_FILE_SUFFIX}` for append, then discarding the handle. `build_log_writer` subsequently calls `tracing_appender::rolling::daily(&target.dir, &target.file_name)` (src/logging.rs:278), which treats `file_name` as a *prefix* and appends the date. The two names therefore never coincide. I confirmed this on disk after one `librebar::init(...).logging().start()`: the log directory contained both `probeapp.jsonl` at 0 bytes and `probeapp.jsonl.2026-08-01` at 195 bytes. The wasted work is a `create_dir_all` plus an `open`/`close` per process start, measured at 33 µs of a 2.1 ms startup — small, but it is entirely wasted rather than merely cheap, and the probe does not validate the file the appender actually opens. Every run of every librebar CLI also leaves the stray empty file behind.

```rust src/logging.rs:309-322
/// Verify a directory is writable by creating it (if needed) and opening a file for append.
fn ensure_writable(dir: &Path, file_name: &str) -> std::result::Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create log directory {}: {e}", dir.display()))?;

    let path = dir.join(file_name);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Failed to open log file {}: {e}", path.display()))?;

    Ok(())
}
```

Also at `src/logging.rs:278-279`.

**Remediation:** Probe with the name the appender will really use, or probe the directory itself (`create_dir_all` plus a temp file that is removed) rather than creating a permanent one. Either removes the stray file and makes the writability check test the real target. If the probe file is kept deliberately, remove `create(true)` so a genuinely unwritable directory is still detected without manufacturing a file as a side effect.

<div>&hairsp;</div>

*Verdict: The batch processor cannot drive the selected hyper client, which means spans may never leave the process — the failure mode of an observability stack that reports nothing is indistinguishable from an application that has nothing to report, and that is what makes it serious. Around it sit three smaller gaps that share one cause: the `otel` feature grew faster than the code that consumes it. Two public config fields are documented as configuration and read by no code path, a documented protocol value cannot be built because its exporter feature is never enabled, and `otel-grpc` is the only feature of the nineteen with no integration test.*

<div>&nbsp;</div>

---

## The Public API Surface

*For a foundation library the public API is the product, and this one leaks its dependencies into signatures its callers cannot name.*

### Dependency types appear in public signatures without a matching re-export {#unreachable-dependency-types-in-public-api}

**significant** · `src/lib.rs:198-206` · effort: medium · <img src="assets/sparkline-unreachable-dependency-types-in-public-api.svg" height="14" alt="commit activity" />

lib.rs:198-206 articulates exactly the right rule — a dependency type that appears in librebar's public API must be reachable through librebar so the consumer provably names the same type librebar compiled against. That rule is applied only partially, and only to some of the crate's dependencies: `camino` (src/lib.rs:205-206), `divan` and `gungraun` (src/bench.rs:47,50), `rmcp`'s `ServiceExt`, `handler` and `model` (src/mcp.rs:21-23), and hyper item-by-item (src/http.rs:113,114,126 re-export `HeaderMap`, `HeaderValue`, `Method`, `Request`, `StatusCode`, `Version`, `Bytes`). No other dependency gets it, the rmcp re-export does not extend to the `tokio::io` types in the same module, and several public items name types the consumer cannot reach: `clap::Command` and `clap::Error` (src/cli.rs:307, src/cli/parse.rs:33,61), `clap::CommandFactory` as a bound (src/cli/artifacts.rs:38,56 and src/cli/schema.rs:419), `tracing_subscriber::filter::EnvFilter` as both a parameter and a return type (src/logging.rs:103,150), `tracing_subscriber::Layer` and `Registry` inside the public alias `otel::BoxedLayer` (src/otel.rs:30-31), `hyper::header::AsHeaderName` as a generic bound with neither it nor `HeaderName` re-exported (src/http/response.rs:46,91), `tokio::io::Stdin`/`Stdout` (src/mcp.rs:31), and `serde_json::Value` as the return type of the documented parse escape hatches (src/config.rs:161,186,199,211,223). `cli` is a default feature and `CommonArgs` is derived with `clap::Args`, so every default-featured consumer already has to declare its own clap dependency and keep it in lockstep with librebar's. When librebar moves to clap 5 or tracing-subscriber 0.4, every consumer breaks at the same instant with no re-export to smooth the transition, and until then a consumer resolving a different major of any of these gets a type-mismatch error whose cause is invisible from librebar's docs.

```rust src/lib.rs:198-206
/// Re-export of [`camino`].
///
/// `Utf8Path` and `Utf8PathBuf` appear in librebar's own public API — see
/// [`Builder::config_from_file`] — and are the natural path type for a
/// consumer's config struct. Reaching them through this re-export guarantees
/// they are the same types librebar was compiled against, rather than a
/// second copy from an independently resolved `camino` dependency.
#[cfg(feature = "config")]
pub use camino;
```

Also at `src/cli.rs:307-307`, `src/otel.rs:30-31`, `src/logging.rs:103-103`, `src/http/response.rs:46-46`, `src/mcp.rs:31-31`, `src/config.rs:186-186`.

Related: [dependency-error-payloads-are-unwrappable](#dependency-error-payloads-are-unwrappable).

**Remediation:** Decide per type: wrap it, or re-export it with the camino rationale attached. Wrap where the type is incidental — `logging::init` can take `&str`/a librebar filter type rather than an `EnvFilter`, and `Response::header` can take `&str` instead of `K: AsHeaderName`. Re-export where the type is genuinely the consumer's to build: `pub use clap;` under `cli`, `pub use serde_json;` under `config`, `pub use tokio;` under `mcp`/`shutdown`, and extend src/http.rs:113 to include `HeaderName`, `AsHeaderName` and `Uri`. Whichever way each goes, state the policy in the crate docs next to the versioning section so the next added dependency is decided rather than defaulted.

<div>&hairsp;</div>

### Public error variants carry third-party error types the caller cannot name {#dependency-error-payloads-are-unwrappable}

**significant** · `src/error.rs:191-208` · effort: medium · <img src="assets/sparkline-dependency-error-payloads-are-unwrappable.svg" height="14" alt="commit activity" />

All four public error enums are correctly `#[non_exhaustive]`, and README.md:573-576 explains that this makes *adding a variant* additive. But `#[non_exhaustive]` says nothing about the type inside a variant, and eleven foreign error types are structural parts of librebar's public API: `toml::de::Error`, `serde_saphyr::Error` and `serde_json::Error` (ConfigParseError, lines 198/201/204), `opentelemetry_otlp::ExporterBuildError`, `tracing_subscriber::util::TryInitError` and `tokio::runtime::TryCurrentError` (Error, lines 66/71/81), `rustls::Error`, `hyper::http::uri::InvalidUri`, `hyper::http::Error` and `hyper::header::InvalidHeaderValue` (HttpError, lines 125/128/131/134), and `base64::DecodeError` (CacheError, line 188). None of these crates is re-exported, so a caller who matches on `ConfigParseError::Yaml(e)` and wants anything from `e` beyond its `Display` must add serde-saphyr to their own Cargo.toml at exactly librebar's version. That version is `0.0.29`: under Cargo's rules every `0.0.x` release is its own incompatible major, so a routine dependency bump inside librebar silently changes the type of a public error payload. The README's "what counts as breaking" list (README.md:566-580) does not cover this case, so the hazard is invisible to the release process. Three variants already get this right and are the model for the rest: `HttpError::Request` (line 137), the `CookieJar` source field (:156) and `HttpError::Body` (:166) hold `tower::BoxError`, which is a plain alias for `Box<dyn Error + Send + Sync>` (tower-0.5.3/src/lib.rs:228) — a std type a caller can use in full without adding tower to their manifest.

```rust src/error.rs:191-208
/// Errors from config file parsing.
#[cfg(feature = "config")]
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ConfigParseError {
    /// TOML parse error.
    #[error("{0}")]
    Toml(#[from] toml::de::Error),
    /// YAML parse error.
    #[error("{0}")]
    Yaml(#[from] serde_saphyr::Error),
    /// JSON parse error.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// I/O error reading the file.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}
```

Also at `src/error.rs:63-81`, `src/error.rs:119-137`, `src/error.rs:179-189`.

Related: [growable-public-structs-lack-non-exhaustive](#growable-public-structs-lack-non-exhaustive), [unreachable-dependency-types-in-public-api](#unreachable-dependency-types-in-public-api).

**Remediation:** Split the payloads by intent. For the ones callers only ever print, hold them as an opaque boxed source (`#[source] Box<dyn Error + Send + Sync>`, exactly what the three `tower::BoxError` variants already do) or as a librebar-owned struct carrying the rendered message plus the structured fields that matter — line/column for the parse errors — and the public type stops moving when the dependency does. For any payload where callers genuinely need the foreign type's methods, re-export the crate alongside it as camino is re-exported (src/lib.rs:198-206). Then extend README.md:566-580 with a rule that changing a variant's payload type is breaking, so the decision is enforced at release time.

<div>&hairsp;</div>

### Config-shaped public structs are exhaustive, and one has a cfg-gated field {#growable-public-structs-lack-non-exhaustive}

**significant** · `src/http.rs:130-148` · effort: small · <img src="assets/sparkline-growable-public-structs-lack-non-exhaustive.svg" height="14" alt="commit activity" />

The error enums are `#[non_exhaustive]`; none of the config-shaped structs are, and all of their fields are `pub`, so downstream code can build them with struct literals and any added field is breaking. git blame shows this is not theoretical for `HttpClientConfig` — it went from two fields to six in fad63c3d (2026-07-31) and to seven in 8fd83b35 (2026-08-01), two breaking changes in two days under the crate's own rules (README.md:566-580). The seventh field is the sharp edge: `http_cache_stale_retention` is behind `#[cfg(feature = "http-cache")]`, so the struct's shape depends on a feature flag. Cargo features are additive and unified across the whole graph, which means an unrelated crate somewhere in a consumer's dependency tree turning on `librebar/http-cache` changes the field set of a struct that consumer constructs by literal, and their build breaks with no change on their side. The same exhaustive-with-pub-fields shape covers `ConfigSources`, `OtelConfig`, `LoggingConfig`, `LogTarget`, `CheckResult`, `NamedResult`, `DoctorSummary`, `CrashInfo`, `UpdateInfo`, and the entire CLI Spec document tree in src/cli/schema.rs — which is explicitly a versioned wire format (`CLI_SPEC_VERSION = "0.2"`, src/cli/schema.rs:11) and therefore guaranteed to grow. Note the crate already knows the alternative: `RetryPolicy` (src/http.rs:182-229) keeps its fields private behind `retries()` and `retries_all_methods()` accessors.

```rust src/http.rs:130-148
/// Configuration for [`HttpClient`].
#[derive(Debug)]
pub struct HttpClientConfig {
    /// Value sent as the `User-Agent` header on every request.
    pub user_agent: String,
    /// Whole-operation timeout, including redirects and retry backoff.
    pub timeout: Duration,
    /// Maximum number of redirects to follow. Zero disables redirect following.
    pub max_redirects: usize,
    /// Whether gzip and Brotli responses are requested and decompressed.
    pub decompression: bool,
    /// Retry behavior for transient failures.
    pub retry_policy: RetryPolicy,
    /// Maximum decoded response body retained in memory. Zero disables the limit.
    pub max_response_size: usize,
    /// How long stale HTTP entries remain available for revalidation.
    #[cfg(feature = "http-cache")]
    pub http_cache_stale_retention: Duration,
}
```

Also at `src/cli/schema.rs:14-38`, `src/config.rs:87-104`, `src/otel.rs:34-51`, `src/logging.rs:47-57`.

Related: [dependency-error-payloads-are-unwrappable](#dependency-error-payloads-are-unwrappable), [schema-wire-types-are-serialize-only](#schema-wire-types-are-serialize-only).

**Remediation:** Add `#[non_exhaustive]` to every public struct that is expected to grow, starting with `HttpClientConfig` and the src/cli/schema.rs document types, and keep a constructor plus builder methods as the supported construction path (`HttpClientConfig::new` and the `with_*` methods already exist). For `HttpClientConfig` specifically, either make the field unconditional with a value that is simply unused without the feature, or move it behind the accessor pattern `RetryPolicy` already uses, so the struct's shape stops depending on feature unification. Then extend README.md:582-589 to say that adding a field to a `#[non_exhaustive]` struct is additive — which is the property that makes the annotation worth having.

<div>&hairsp;</div>

### UpdateChecker builds its own HTTP client and cache and hardcodes GitHub {#update-checker-hardcodes-github-and-its-collaborators}

**significant** · `src/update.rs:91-109` · effort: medium · <img src="assets/sparkline-update-checker-hardcodes-github-and-its-collaborators.svg" height="14" alt="commit activity" />

`update` is the module that most directly touches an external system, and it is the one with no seam. `check()` constructs both of its collaborators itself: the filesystem cache at line 98 and an `HttpClient` with default configuration at lines 108-109. The release source is a GitHub URL literal at line 107 and a GitHub response shape at lines 131-133 (`tag_name`, `html_url`). `UpdateChecker`'s fields are private (lines 48-51) and `new` takes three `&str`, so there is no constructor, builder, or trait through which a caller can substitute any of it. Concretely this means: no GitLab, Forgejo, crates.io, private artifact server, or Homebrew tap can be used as a release source; the 24-hour cache cannot be redirected for tests or for a sandbox with no writable cache dir; and no `Authorization` header can be attached, which pins every consumer to GitHub's 60-requests-per-hour-per-IP unauthenticated limit — on shared CI egress that limit is routinely exhausted, and because every failure path returns `None` (lines 113-117, 119-122) the check silently does nothing. The codebase already demonstrates the pattern it is missing here: `EnvironmentSource` (src/config/environment.rs:10-13) lets a caller replace the process environment, and `DoctorCheck` (src/diagnostics.rs:36-45) lets a caller supply diagnostics. `update` is the module where the same treatment matters most and it is absent.

```rust src/update.rs:91-109
    pub async fn check(&self) -> Option<UpdateInfo> {
        if self.is_suppressed() {
            tracing::debug!("update check suppressed by env");
            return None;
        }

        // Check cache first
        if let Some(cache) = crate::cache::Cache::default_for(&self.app_name)
            && let Ok(Some(cached)) = cache.get(CACHE_KEY)
            && let Ok(version) = String::from_utf8(cached)
        {
            tracing::debug!(cached_version = %version, "using cached version check");
            return self.compare_versions(&version);
        }

        // Fetch from GitHub
        let url = format!("https://api.github.com/repos/{}/releases/latest", self.repo);
        let client =
            crate::http::HttpClient::from_app(&self.app_name, &self.current_version).ok()?;
```

Also at `src/update.rs:47-52`, `src/update.rs:131-133`, `src/config/environment.rs:10-13`.

Related: [environment-source-trait-over-constrains-implementors](#environment-source-trait-over-constrains-implementors).

**Remediation:** Define a `ReleaseSource` trait — one async method returning the latest version and its release URL — and make the GitHub implementation one backend behind it, the way `ProcessEnvironment` implements `EnvironmentSource`. Give `UpdateChecker` builder methods to supply a `ReleaseSource`, a `&Cache`, and a pre-configured `HttpClient`, keeping `new(app, version, repo)` as the shorthand that wires the GitHub backend with defaults so existing callers are unaffected. The injected client is what lets a consumer add an auth token and lift the rate limit; the injected cache is what makes the module testable without touching the user's real cache directory.

<div>&hairsp;</div>

### EnvironmentSource requires Debug and forces implementors to materialize everything {#environment-source-trait-over-constrains-implementors}

**moderate** · `src/config/environment.rs:9-13` · effort: small · <img src="assets/sparkline-environment-source-trait-over-constrains-implementors.svg" height="14" alt="commit activity" />

This is the crate's best pluggability seam, and two details in its signature push cost onto every implementor. First, the `Debug` supertrait is not there for the trait's own sake — `ConfigLoader` holds `Option<Arc<dyn EnvironmentSource>>` (src/config.rs:260) and derives `Debug` (src/config.rs:253), and a `dyn` field can only satisfy that derive if the trait requires `Debug`. The bound is a container's convenience charged to every downstream implementor, including one wrapping a client type that is not itself `Debug` and would need a hand-written impl. Second, `vars()` returns an owned `Vec<(OsString, OsString)>` with no argument, while the only caller immediately narrows it: src/config/environment.rs:45-50 collects every pair, converts, then filters by the `{APP}_` prefix. For `ProcessEnvironment` that is fine. For the implementations the trait exists to enable — a secrets manager, a remote parameter store, an SSM or Vault backend — the contract requires fetching and allocating the entire keyspace to hand back a set that the library discards nearly all of, and the implementor has no way to see the prefix that would let it scope the request.

```rust src/config/environment.rs:9-13
/// Source of process-style configuration variables.
pub trait EnvironmentSource: std::fmt::Debug {
    /// Return environment key/value pairs.
    fn vars(&self) -> Vec<(OsString, OsString)>;
}
```

Also at `src/config.rs:253-263`, `src/config/environment.rs:45-50`.

Related: [update-checker-hardcodes-github-and-its-collaborators](#update-checker-hardcodes-github-and-its-collaborators).

**Remediation:** Drop the `Debug` supertrait and give `ConfigLoader` a hand-written `Debug` impl that prints a placeholder for the environment source — a few lines, and the bound stops propagating. Pass the prefix into the method (`fn vars(&self, prefix: &str)`) so an implementor can scope its fetch, and keep `ProcessEnvironment` filtering locally. If the allocation is worth removing too, returning `Box<dyn Iterator<Item = (OsString, OsString)> + '_>` keeps the trait object-safe while letting implementors stream.

<div>&hairsp;</div>

### DoctorRunner::add requires a pre-boxed check and DoctorCheck demands an unused Send {#doctor-check-registration-forces-caller-boxing}

**advisory** · `src/diagnostics.rs:96-110` · effort: trivial · <img src="assets/sparkline-doctor-check-registration-forces-caller-boxing.svg" height="14" alt="commit activity" />

Storing `Vec<Box<dyn DoctorCheck>>` is correct — the whole point is a heterogeneous check list. Exposing the box in `add`'s signature is not: the caller writes `runner.add(Box::new(ConfigCheck))` in the module's own documentation example (src/diagnostics.rs:22-23), when `add(&mut self, check: impl DoctorCheck + 'static)` would box internally and leave the storage decision an implementation detail the crate could later change. Separately, `DoctorCheck` requires `Send` (src/diagnostics.rs:36) that nothing uses: `run_all` (src/diagnostics.rs:118-134) iterates the checks sequentially on the calling thread, and no check is ever sent across a thread boundary. The bound rules out an implementor holding a `Rc` or a non-`Send` handle for no benefit the crate currently collects.

```rust src/diagnostics.rs:96-110
/// Collects and runs doctor checks.
pub struct DoctorRunner {
    checks: Vec<Box<dyn DoctorCheck>>,
}

impl DoctorRunner {
    /// Create a new empty runner.
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Register a check.
    pub fn add(&mut self, check: Box<dyn DoctorCheck>) {
        self.checks.push(check);
    }
```

Also at `src/diagnostics.rs:36-36`, `src/diagnostics.rs:118-134`.

Related: [debug-bundle-builder-cannot-be-chained](#debug-bundle-builder-cannot-be-chained).

**Remediation:** Change `add` to take `impl DoctorCheck + 'static` and box inside, keeping the field type unchanged. Drop the `Send` supertrait unless a concurrent `run_all` is planned — if it is, keep it and say so in the trait's doc comment so implementors know the bound is load-bearing rather than incidental.

<div>&hairsp;</div>

### CLI Spec document types can be written but not read back {#schema-wire-types-are-serialize-only}

**advisory** · `src/cli/schema.rs:133-145` · effort: small · <img src="assets/sparkline-schema-wire-types-are-serialize-only.svg" height="14" alt="commit activity" />

Every type in the CLI Spec document tree — `SchemaDocument`, `CommandSchema`, `ArgumentSchema`, `ArgumentGroupSchema`, `OutputField`, `CommandExample`, `ErrorMetadata`, `OutcomeMetadata`, `OutputBehavior`, `Stability` — derives `Serialize` and nothing else beyond `Debug` and `Clone`. These types exist to produce a machine-readable contract for external tooling (src/cli/parse.rs:154-169 writes the document to stdout), and the obvious things that tooling wants to do with a committed `clispec.json` are read it back and diff it against the current one to detect contract drift in CI. Neither is possible with librebar's own types: a consumer must hand-roll a parallel set of structs, which then drifts from librebar's as CLI Spec moves from 0.2 to 0.3. The missing `PartialEq`/`Eq` blocks the diff even for a consumer holding two freshly generated documents.

```rust src/cli/schema.rs:133-145
/// Stability declared for an application command contract.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    /// Stable public contract.
    Stable,
    /// Public beta contract.
    Beta,
    /// Experimental contract.
    Experimental,
    /// Deprecated contract retained for compatibility.
    Deprecated,
}
```

Also at `src/cli/schema.rs:14-15`, `src/cli/schema.rs:50-51`, `src/cli/schema.rs:84-85`.

Related: [growable-public-structs-lack-non-exhaustive](#growable-public-structs-lack-non-exhaustive).

**Remediation:** Add `Deserialize` and `PartialEq`/`Eq` to the schema document types alongside the existing `Serialize`. `Stability` and the string/`Vec`/`Option` fields all deserialize cleanly; the `&'static str` fields (`clispec`, `tty`, `piped`) need to become `String` or `Cow<'static, str>` first, which is a good moment to do it together with the `#[non_exhaustive]` change these same types need.

<div>&hairsp;</div>

*Verdict: The stated design principle for this crate is to wrap hyper rather than expose it. The wrapping is real and mostly holds, but it has gaps: dependency types appear in public signatures without matching re-exports, and public error variants carry third-party error payloads a caller cannot construct, match on, or convert without adding the dependency themselves. Each leak converts a semver bump in a dependency into a breaking change for every downstream consumer. The second theme is pluggability — `UpdateChecker` builds its own HTTP client and its own cache and hardcodes GitHub, which is precisely the 'define the trait first, implement the backend second' rule this project sets for itself.*

<div>&nbsp;</div>

---

## The Error Architecture Surface

*The error type is well-built and consistently used; what it loses is context — eight variants wrap a cause they never expose, and several call sites report the wrong one.*

### Eight Error variants wrap an error without exposing it through source() {#error-variants-drop-the-source-chain}

**moderate** · `src/error.rs:73-106` · effort: small · <img src="assets/sparkline-error-variants-drop-the-source-chain.svg" height="14" alt="commit activity" />

thiserror only treats a field as the error source when it carries `#[source]` or
`#[from]`, or is literally named `source` (thiserror-impl 2.0.19,
`src/prop.rs:97-110`). Eight variants interpolate a wrapped error into their
`Display` with `{0}` but carry no such attribute, so `Error::source()` returns
`None` for them: `ConfigDeserialize` (line 26), `OtelInit` (66), `TracingInit` (71),
`ShutdownInit` (76), `NoRuntime` (81), `Lock` (86), `Dispatch` (101), and
`Diagnostic` (106).

The message text survives, the machine-readable chain does not. A caller holding
`Error::Lock` cannot downcast to `std::io::Error` to distinguish lock contention
(`ErrorKind::WouldBlock`, retry later) from `PermissionDenied` or `NotFound` (give
up and report), even though `src/lockfile.rs:109` deliberately preserved the kind
when constructing it. The same applies to `Dispatch` (was the plugin missing or
non-executable?) and `Diagnostic`. `anyhow`'s `{:#}` chain rendering and any
`err.source()` walk stop at librebar.

The crate is inconsistent about this: `Http`, `Cache`, and `Io` do use `#[from]` and
chain correctly; `HttpError::Request`/`Body` use `#[source]`; `Error::ConfigParse`
and `HttpError::CookieJar` use a field named `source`. There is also no
error-kind accessor anywhere in the crate — no `Error::kind()`, no `is_*()`
predicates — so with the chain broken, classification for these eight variants is
only possible by matching on `#[non_exhaustive]`, feature-gated variants (a match
whose available arms depend on the feature union across the whole dependency graph)
or by string-matching `Display`.

```rust src/error.rs:73-106
/// Shutdown signal handler registration failed.
#[cfg(feature = "shutdown")]
#[error("failed to register shutdown handler: {0}")]
ShutdownInit(std::io::Error),

/// No Tokio runtime available for async initialization.
#[cfg(feature = "shutdown")]
#[error("no active Tokio runtime: {0}")]
NoRuntime(tokio::runtime::TryCurrentError),

/// Lockfile acquisition failed.
#[cfg(feature = "lockfile")]
#[error("failed to acquire lock: {0}")]
Lock(std::io::Error),

/// HTTP client error.
#[cfg(feature = "http")]
#[error("HTTP error: {0}")]
Http(#[from] HttpError),

/// Cache I/O error.
#[cfg(feature = "cache")]
#[error("cache error: {0}")]
Cache(#[from] CacheError),

/// External command dispatch error.
#[cfg(feature = "dispatch")]
#[error("dispatch error: {0}")]
Dispatch(std::io::Error),

/// Diagnostic error.
#[cfg(feature = "diagnostics")]
#[error("diagnostic error: {0}")]
Diagnostic(std::io::Error),
```

Also at `src/error.rs:23-26`.

Enables [lock-error-message-misreports-the-cause](#lock-error-message-misreports-the-cause).

Related: [error-display-duplicates-its-source](#error-display-duplicates-its-source).

**Remediation:** Add `#[source]` to each of the eight tuple fields. `#[from]` is not available for
the `std::io::Error` ones because `Error::Io` already owns that `From` impl, but
`#[source]` alone is exactly what is needed and does not add a conversion. Consider
pairing this with a small classification accessor (`Error::io_kind() ->
Option<ErrorKind>`, or `is_transient()`) so callers can branch without matching
feature-gated variants. Per the README's own versioning rules this is a semantics
change to a stable API, so land it in a minor bump.

<div>&hairsp;</div>

### Every try_lock failure is reported as contention, including real I/O errors {#lock-error-message-misreports-the-cause}

**moderate** · `src/lockfile.rs:107-112` · effort: trivial · <img src="assets/sparkline-lock-error-message-misreports-the-cause.svg" height="14" alt="commit activity" />

`File::try_lock` (stable since 1.89.0) returns `TryLockError`, which std documents
as two distinct outcomes: `WouldBlock` — the lock is held by another handle — and
`Error(io::Error)` — a genuine I/O failure, explicitly documented as never carrying
`WouldBlock`. librebar collapses both into one message that asserts the first.

A filesystem that does not support advisory locks, an `ENOLCK` under fd pressure, or
an `EBADF` all surface to the user as "failed to acquire lock: another instance holds
the lock: /tmp/myapp/myapp.lock" — a false diagnosis that sends an operator hunting
for a process that does not exist. The original error's own message is discarded
entirely, and because `Error::Lock` has no `source()` (see
`error-variants-drop-the-source-chain`) it cannot be recovered downstream either.
The `kind()` is preserved on line 109, so the information exists at construction
time and is thrown away one line later.

```rust src/lockfile.rs:107-112
file.try_lock().map_err(|e| {
    Error::Lock(std::io::Error::new(
        std::io::Error::from(e).kind(),
        format!("another instance holds the lock: {}", self.path.display()),
    ))
})?;
```

Enabled by [error-variants-drop-the-source-chain](#error-variants-drop-the-source-chain).

**Remediation:** Match on `TryLockError` and produce different messages for the two arms — keep
"another instance holds the lock: {path}" for `WouldBlock`, and for
`TryLockError::Error(io)` carry the original error's message and attach it as the
variant's `source()`. If the two cases warrant separate handling by callers, a
distinct `Error::LockContended` variant is additive under `#[non_exhaustive]`.

<div>&hairsp;</div>

### Display interpolates the source it also returns from source(), producing repeated messages {#error-display-duplicates-its-source}

**advisory** · `src/error.rs:88-96` · effort: small · <img src="assets/sparkline-error-display-duplicates-its-source.svg" height="14" alt="commit activity" />

`#[from]` implies `#[source]`, so these variants both return the inner error from
`source()` and print it inside their own `Display`. Any consumer that renders the
full chain — `anyhow`'s `{:#}`, `eyre`, or a hand-rolled `while let Some(e) =
e.source()` loop — prints each level's message once per level it appears in.

A JSON parse failure inside an HTTP response renders as
"HTTP error: JSON: expected value at line 1 column 1: JSON: expected value at line 1
column 1: expected value at line 1 column 1" because the same text is nested three
deep: `Error::Http` prints `HttpError`, `HttpError::Json` prints `serde_json::Error`,
and the chain walk visits all three. The pattern repeats across `CacheError`
(lines 176-189), `ConfigParseError` (192-208), and every `#[from]` variant of
`HttpError` (119-137). `Error::Io` at line 109 already does this correctly with
`#[error(transparent)]`.

```rust src/error.rs:88-96
/// HTTP client error.
#[cfg(feature = "http")]
#[error("HTTP error: {0}")]
Http(#[from] HttpError),

/// Cache I/O error.
#[cfg(feature = "cache")]
#[error("cache error: {0}")]
Cache(#[from] CacheError),
```

Also at `src/error.rs:119-137`, `src/error.rs:176-189`, `src/error.rs:192-208`.

Related: [error-variants-drop-the-source-chain](#error-variants-drop-the-source-chain).

**Remediation:** Where a variant exists only to widen a nested error, use `#[error(transparent)]` as
`Error::Io` already does. Where the prefix carries real information, keep the prefix
but drop the interpolation — `#[error("HTTP error")]` with `#[from]` still shows the
full detail through the chain. Note this changes `Display` output, which the
README's versioning section classifies as a stable-API semantics change.

<div>&hairsp;</div>

### Update check discards three failure paths that its docs promise are logged {#update-check-drops-errors-it-documents-as-logged}

**advisory** · `src/update.rs:106-117` · effort: trivial · <img src="assets/sparkline-update-check-drops-errors-it-documents-as-logged.svg" height="14" alt="commit activity" />

`UpdateChecker::check` documents its contract at `src/update.rs:88-89`: "Network
errors, GitHub rate limits, and parse failures are logged at debug level and return
`None`." Three of its failure paths return `None` without logging anything.

`src/update.rs:109` — `HttpClient::from_app(...).ok()?` discards `HttpError`, which
covers rustls provider initialization failure. That is a permanent, configuration-
level fault, and it produces no diagnostic at any level.

`src/update.rs:99` — `let Ok(Some(cached)) = cache.get(CACHE_KEY)` in an `if let`
chain swallows `CacheError::Json` and `CacheError::Decode` from a corrupt cache
entry. The code falls through to the network, which is the right recovery, but
`src/http/cache.rs:384` handles the identical condition with an explicit
`tracing::warn!("discarding corrupt HTTP cache entry")`.

`src/update.rs:131-133` — `json.get("tag_name")?.as_str()?` turns a shape change in
the GitHub API response into a silent `None`, the case an operator is most likely to
need explained.

The immediately adjacent code on lines 111-117 and 124-130 does log at debug, so the
omissions read as oversights rather than intent.

```rust src/update.rs:106-117
// Fetch from GitHub
let url = format!("https://api.github.com/repos/{}/releases/latest", self.repo);
let client =
    crate::http::HttpClient::from_app(&self.app_name, &self.current_version).ok()?;

let resp = match client.get(&url).await {
    Ok(r) => r,
    Err(e) => {
        tracing::debug!(error = %e, "update check failed");
        return None;
    }
};
```

Also at `src/update.rs:85-91`, `src/update.rs:97-104`, `src/update.rs:131-133`.

Related: [http-cache-eviction-results-discarded](#http-cache-eviction-results-discarded).

**Remediation:** Add `tracing::debug!` (matching the surrounding style) to all three paths: log the
`HttpError` before returning on client construction, log the `CacheError` when the
cached entry is unreadable, and replace the bare `?` on `tag_name`/`html_url` with a
logged early return naming the missing field.

<div>&hairsp;</div>

### Lockfile promises unconditional cross-process exclusion and reports every lock error as contention {#lockfile-exclusion-guarantee-unqualified}

**advisory** · `src/lockfile.rs:95-120` · effort: trivial · <img src="assets/sparkline-lockfile-exclusion-guarantee-unqualified.svg" height="14" alt="commit activity" />

`File::try_lock` is `flock(LOCK_EX|LOCK_NB)` on Unix and `LockFileEx` on Windows — advisory locking whose behaviour is filesystem-dependent. On NFS without a working lock daemon, and on several FUSE and overlay filesystems, `flock` either fails outright or is a silent no-op in which both processes believe they hold the lock. The module doc names the lock as advisory at src/lockfile.rs:3, but attaches no platform caveat to the exclusion guarantee it states on :4-5 — "Two instances of the same application cannot hold the lock simultaneously" — and `Lockfile::new(app_name, dir)` accepts an arbitrary caller-supplied directory, so nothing keeps a caller from pointing it at a network share and relying on a guarantee that does not hold there. Separately, the error mapping collapses the whole `TryLockError` space into one contention message, so `EOPNOTSUPP` from a filesystem that cannot lock is indistinguishable from a genuine second instance — the same defect as `lock-error-message-misreports-the-cause`, on the same six lines (src/lockfile.rs:107-112). See that finding for the mapping itself; the claim unique to this one is the unqualified guarantee. The stale-lock question this pattern usually raises does not apply — because the lock lives on the file descriptor, the kernel releases it when the process dies, so there is no wedged-lock recovery problem.

```rust src/lockfile.rs:95-120
    pub fn try_acquire(&self) -> Result<LockGuard> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)?;

        file.try_lock().map_err(|e| {
            Error::Lock(std::io::Error::new(
                std::io::Error::from(e).kind(),
                format!("another instance holds the lock: {}", self.path.display()),
            ))
        })?;

        tracing::debug!(path = %self.path.display(), "lock acquired");

        Ok(LockGuard {
            _file: file,
            path: self.path.clone(),
        })
    }
```

Also at `src/lockfile.rs:1-5`.

Related: [lock-error-message-misreports-the-cause](#lock-error-message-misreports-the-cause).

**Remediation:** Add a "Platform behaviour" section to the module docs stating that the lock is advisory, that it is only reliable on local filesystems, and that pointing `Lockfile::new` at a network filesystem may silently defeat exclusion. The `TryLockError` mapping on the same lines is `lock-error-message-misreports-the-cause`'s remediation — match on the error and map `WouldBlock` to contention while propagating everything else with its cause intact — and fixing it there fixes it here.

<div>&hairsp;</div>

### Lock directory falls back to /tmp, letting a local user pre-create the path and hold the lock {#lockfile-falls-back-to-shared-tmp-on-linux}

**advisory** · `src/lockfile.rs:30-36` · effort: small · <img src="assets/sparkline-lockfile-falls-back-to-shared-tmp-on-linux.svg" height="14" alt="commit activity" />

When XDG_RUNTIME_DIR is unset — a service account, a container, a cron job, an sshd session
without pam_systemd — default_lock_dir returns /tmp/{app_name}. try_acquire then runs
create_dir_all(parent) followed by File::options().read(true).write(true).create(true)
.truncate(false).open(&path). Neither call is symlink-safe or ownership-checked. An
unprivileged local user who knows the app name pre-creates /tmp/{app} before the victim
first runs the tool. Two outcomes, both reachable: (a) the attacker creates the directory
mode 0755 owned by themselves, so the victim's create_dir_all succeeds on the existing
directory but the open() for write fails with EACCES — a permanent denial of the exclusive
section for every other account on the host; (b) the attacker creates /tmp/{app} and plants
{app}.lock inside it, then holds a flock on it indefinitely, so every victim's try_acquire
returns Error::Lock("another instance holds the lock") forever, even with no other instance
running. A symlinked /tmp/{app} likewise redirects the lock to an attacker-chosen path;
the file is never written to, so this is a lock-integrity and availability problem, not an
arbitrary-write one. The impact is bounded by what the consuming app guards with the lock —
for the update checkers and background daemons the module docs name, that is a persistent
DoS of that functionality. macOS is unaffected in practice because TMPDIR is per-user.
lockfile is a non-default feature.

```rust src/lockfile.rs:30-36
#[cfg(target_os = "linux")]
{
    let base = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    base.join(app_name)
}
```

Also at `src/lockfile.rs:96-105`.

> A predictable path in a world-writable directory is a reservation I can make before you do. I create the file first and hold the lock, and your process concludes it is contending with a legitimate peer. It waits politely for something that is never going to let go.

**Remediation:** Prefer a per-user directory: on Linux, fall back to $XDG_STATE_HOME (or ~/.local/state)
before /tmp, and only reach for /tmp when no home directory exists. When /tmp is
unavoidable, include the uid in the directory name (/tmp/{app}-{uid}), create it with
DirBuilderExt::mode(0o700), and after opening verify with fstat that the file is a regular
file owned by the current uid with no group/other write bits — bail out rather than
proceeding if it is not. Obtaining that uid is its own step: std exposes no getuid(), and
std::os::unix::fs::MetadataExt::uid() reports a *file's* owner, not the process's. Since
Cargo.toml:168 sets unsafe_code = "deny" and the tree carries no libc, rustix or nix
dependency, either add one of those, or — on the Linux target this finding is specifically
about — read it without unsafe via std::fs::metadata("/proc/self") plus MetadataExt::uid().

<div>&hairsp;</div>

*Verdict: There is no unwrap-on-external-input problem here and no panic reachable from a parser, which is the outcome that matters most for a library and is worth stating plainly. The defects are diagnostic rather than structural. Eight variants wrap an underlying error without returning it from `source()`, so the chain terminates early and the real cause is unreachable programmatically even though it appears in the message. Alongside that, `Display` interpolates the source it also returns, producing doubled text in any consumer that walks the chain — the classic thiserror double-print. The lock-error attribution is the one with operational consequence: every `try_lock` failure is reported as contention, including real I/O errors, which will send someone debugging the wrong problem.*

<div>&nbsp;</div>

---

## The Supply Chain Surface

*The dependency policy is codified, enforced, and unusually well-maintained; the exposure that remains is in what the policy does not cover.*

### serde-saphyr sits at 0.0.29, where Cargo's compatibility range is a single version, on a default-feature path {#serde-saphyr-exact-pin-on-default-path}

**significant** · `Cargo.toml:38-40` · effort: small · <img src="assets/sparkline-serde-saphyr-exact-pin-on-default-path.svg" height="14" alt="commit activity" />

Cargo's caret semantics treat the leading non-zero component as the compatibility unit. For a `0.0.z` version there is no non-zero component before `z`, so every `0.0.z` release is mutually incompatible and the requirement `"0.0.29"` resolves to exactly one version: 0.0.29, confirmed at `Cargo.lock:1927`. This is not a loose floor that happens to have resolved tightly — it is a hard pin that cannot move. A hypothetical 0.0.30 carrying a security fix would not be picked up by `cargo update`, by `just update` (`.justfile:61`), or by any consumer's lockfile refresh; only a hand edit to `Cargo.toml` releases it. Two factors sharpen this. First, `serde-saphyr` sits on the `config` feature (`Cargo.toml:147`), and `config` is in `default` (`Cargo.toml:145`), so this is the YAML parser every downstream user gets unless they opt out. Second, the repo has deliberately disabled Dependabot version PRs — `open-pull-requests-limit: 0` at `.github/dependabot.yml:18`, with only security *alerts* routed to issues by the weekly workflow. That is a defensible choice for a crate that batches updates by hand, but it removes the automation that would otherwise surface the staleness, so the exact pin and the disabled updater compound. Upstream has since moved: serde-saphyr published 1.0.0 on 2026-07-31, one day before this commit, following 1.0.0-rc.1 and 1.0.0-rc.2, and it is not a fringe dependency — 3.55M total downloads, 2.76M in the last 90 days. The 1.0.0 release is a semver-major jump and is one day old, so adopting it immediately is not the recommendation; the finding is that the requirement as written cannot express "take patches," only "never move."

```toml Cargo.toml:38-40
# Feature: config
toml = { version = "1.1", optional = true }
serde-saphyr = { version = "0.0.29", optional = true }
```

Also at `Cargo.toml:145-147`, `.github/dependabot.yml:17-18`, `Cargo.lock:1926-1929`.

Related: [ci-builds-only-all-features](#ci-builds-only-all-features).

**Remediation:** Decide explicitly between the two coherent postures rather than inheriting the current one by accident. If serde-saphyr 1.0.0 is to be adopted, change the requirement to `"1.0"` so ordinary patch and minor fixes flow, and treat the migration as a normal semver-major bump under the existing `just upgrade-breaking` recipe once 1.0.0 has had a few weeks of soak. If staying on the 0.0.x line for now, say so in a comment beside the pin recording that `0.0.z` is exact-by-construction and that the crate is consciously frozen pending 1.0 evaluation, and open a tracking issue — that converts an invisible constraint into a visible decision. Either way, audit the other pre-1.0 dependencies on default paths for the same property: `0.x.y` requirements such as `tracing-subscriber = "0.3"` (Cargo.toml:46) and `hyper-rustls = "0.27"` (Cargo.toml:65) behave normally for their line — minor bumps breaking, patches flowing — but any other `0.0.z` requirement is a silent hard pin.

<div>&hairsp;</div>

### The crate's zero-unsafe guarantee stops at the crypto boundary: ring contributes 17 C files and 90 assembly files behind a build script {#ring-is-the-sole-c-asm-island}

**note** · `Cargo.toml:64-66` · effort: trivial · <img src="assets/sparkline-ring-is-the-sole-c-asm-island.svg" height="14" alt="commit activity" />

First-party FFI exposure is zero and verifiably so: `src/` contains no `extern "C"`, no `extern "system"`, no `unsafe` token at all, no bindgen or cc, and `libc` is not a direct dependency; `Cargo.toml:168` sets `unsafe_code = "deny"` and the crate ships no `build.rs`. Surface 8 therefore applies only transitively, and it reduces to one crate. Under `--all-features` the graph contains `ring` 0.17.14, which ships 17 `.c` files, 28 `.h` files, 90 `.S`/`.asm` files, a `build.rs` that drives a C compiler over them, and roughly 230 occurrences of `unsafe` in its Rust sources — by a wide margin the largest non-Rust surface in the tree, and code over which the crate's own `deny(unsafe_code)` lint has no authority. It arrives solely through the TLS stack: `ring` <- `rustls` 0.23.42 <- {`hyper-rustls`, librebar directly, `tokio-rustls`}, so it is present only when the `http` feature is on and absent from a `config`-only build. Worth recording explicitly: the selection is deliberate and is the *more* conservative of the two realistic options. `hyper-rustls` 0.27 defaults to `aws-lc-rs`, and `Cargo.toml:65` sets `default-features = false` and names `ring` instead, choosing the smaller and longer-audited C surface over AWS-LC. Neither is pure Rust, and the pure-Rust rustls providers (rustls-graviola, rustls-rustcrypto) are not production-equivalent drop-ins for 0.23, so no all-Rust option was foregone. This is a documented residual, not a lapse.

```toml Cargo.toml:64-66
hyper = { version = "1.9", features = ["client", "http1", "http2"], optional = true }
hyper-rustls = { version = "0.27", features = ["http1", "http2", "ring", "webpki-tokio"], default-features = false, optional = true }
rustls = { version = "0.23.40", default-features = false, optional = true }
```

Related: [base64-simd-unsafe-optout-holds](#base64-simd-unsafe-optout-holds).

**Remediation:** No change recommended; record the decision rather than revisit it. Add a comment beside `Cargo.toml:65` in the same style as the base64 note at lines 80-82, stating that `ring` is chosen over the `aws-lc-rs` default as the smaller audited C surface, that both are C, and that pure-Rust providers were evaluated and rejected as not production-ready for rustls 0.23. That turns the one place where `deny(unsafe_code)` does not reach into an explicit, reviewable choice and gives a future reader the trigger for revisiting it — a pure-Rust provider reaching parity. Re-evaluate if FIPS validation ever becomes a requirement, which would force `aws-lc-rs` and change the calculus.

<div>&hairsp;</div>

### The duplicate-version check warns rather than fails, and CI is the only place the warnings appear {#bans-multiple-versions-warn-only}

**advisory** · `.config/deny.toml:57-62` · effort: small · <img src="assets/sparkline-bans-multiple-versions-warn-only.svg" height="14" alt="commit activity" />

`cargo tree -d --all-features` prints nine headings, of which four are crates genuinely resolved at more than one version — five extra copies in total: `syn` at 2.0.119 and 3.0.3, `getrandom` at 0.2.17, 0.3.4 and 0.4.3, `base64` at 0.22.1 and 0.23.0, and `supports-color` at 2.1.0 and 3.0.2. The other five headings (derive_more, memchr, serde, serde_core, serde_json) are same-version entries split by feature or proc-macro resolution, not duplicates. `Cargo.lock` holds 306 package entries against 299 unique crate names, so seven entries are duplicate versions — the two not in the list above are `r-efi` and `windows-sys`, both target-gated and therefore invisible to `cargo tree -d` on this host. The `syn` split is the expensive one — it is the single largest compile-time cost in most Rust builds and both majors are compiled, 2.0.119 for tokio-macros / tracing-attributes / wasm-bindgen and 3.0.3 for clap_derive / serde_derive / thiserror-impl. `supports-color` is the one librebar cannot fix at all: both versions arrive through the same crate. owo-colors 4.3.0 defines `supports-colors = ["dep:supports-color-2", "supports-color"]` (owo-colors-4.3.0/Cargo.toml:58-61) and declares `supports-color` at 3.0.0 alongside `supports-color-2`, a rename of the 2.0 line (lines 67-74); librebar enables exactly that feature at `Cargo.toml:36`. The duplicate is upstream's deliberate choice, so no resolution or bump on librebar's side unifies it. The delivery problem compounds the policy one. With `multiple-versions = "warn"` cargo-deny prints these and exits 0, and the only place they are printed is the cargo-deny CI job, whose green checkmark is what anyone actually looks at. Warnings that appear exclusively in the log of a passing job are not read; the `.crustoleum/deny.txt` capture is itself evidence of this, since the duplicate warnings were present there and had to be surfaced by an audit rather than by the pipeline. The setting is a defensible starting posture — a foundation library does not control its transitive graph and cannot unilaterally resolve a `syn` major split — but as written it provides no ratchet, so nothing distinguishes "duplicates we accepted" from "duplicates we just acquired." `[sources] unknown-registry = "warn"` has the same shape: every one of the 306 lock entries currently resolves to crates.io, so denying costs nothing today.

```toml .config/deny.toml:57-62
[bans]
multiple-versions = "warn"
wildcards = "allow"
highlight = "all"
deny = []
skip = []
```

Also at `.config/deny.toml:68-74`.

Related: [base64-simd-unsafe-optout-holds](#base64-simd-unsafe-optout-holds), [license-allowlist-stale-entries](#license-allowlist-stale-entries).

**Remediation:** Convert the check into a ratchet: set `multiple-versions = "deny"` and enumerate the currently-accepted duplicates in `skip`, each with a comment naming the crate that forces it and the condition for removing the entry. `syn` 2.0.119 is held by darling_core, derive_more-impl, divan-macros, futures-macro, gungraun-macros, pin-project-internal, prost-derive, rmcp-macros, schemars_derive, serde_derive_internals, synstructure, tokio-macros, tracing-attributes and the zerovec/yoke/zerofrom derive family — clap_derive, serde_derive and thiserror-impl are already on 3.0.3, so the condition is those fourteen moving, not the ones that have. `getrandom` 0.2.17 is held by `ring` and 0.4.3 by the `tempfile` dev-dependency. `supports-color` belongs in `skip` too, with the note that owo-colors depends on both majors deliberately and the only levers are dropping the `supports-colors` feature at `Cargo.toml:36` or an upstream owo-colors change. New duplicates then fail CI and must be either fixed or consciously accepted, while the existing ones stay green. `cargo deny check bans` prints the exact `skip` stanzas to paste. This also gives the duplicates a written rationale, matching the standard the rest of this file already sets. Flip `[sources] unknown-registry` to `"deny"` at the same time — it is free today and turns a silent warning into a real gate on the first non-crates.io dependency.

<div>&hairsp;</div>

### Three allowlisted licenses match nothing in the graph, and the setting that would flag them is disabled {#license-allowlist-stale-entries}

**note** · `.config/deny.toml:14-24` · effort: trivial · <img src="assets/sparkline-license-allowlist-stale-entries.svg" height="14" alt="commit activity" />

Enumerating every `license` field across the full `--all-features` graph and matching it against the 15-entry allowlist shows that 12 entries are load- bearing and 3 match nothing: BSL-1.0, MIT-0 and CC0-1.0. Because `unused-allowed-license = "allow"` (line 36) suppresses cargo-deny's stale-entry reporting, this will never surface on its own. The practical effect is small — all three are permissive, and none is a license anyone would be alarmed to find — but they function as standing pre-authorizations: a future dependency arriving under CC0-1.0 or BSL-1.0 is admitted with no review event, which is precisely what an allowlist exists to prevent. Every other entry is genuinely needed, including the ones that look exotic: Unicode-3.0 covers 18 crates, CDLA-Permissive-2.0 covers exactly the one crate its comment names, and MPL-2.0 covers one. Two details are worth recording. First, `confidence-threshold = 0.8` (line 35) is currently inert: it governs only text-similarity scoring for crates that ship a `license-file` instead of a `license` field, and zero of the graph's crates do that, so the value has no effect on this dependency set. It is also cargo-deny's default, so it is not a loosening — it is simply dormant until some future dependency omits a machine-readable license. Second, one crate's license expression is malformed: `bincode-next` 2.1.0 declares `MIT or Apache-2.0` with a lowercase operator, which is not valid SPDX. cargo-deny's lax parse mode accepts it silently and resolves it correctly to MIT OR Apache-2.0, so there is no compliance exposure, but the tolerance is the tool's rather than the policy's.

```toml .config/deny.toml:14-24
"BSL-1.0",      # Boost Software License
"ISC",
"MIT",
"MIT-0",        # MIT No Attribution
"MPL-2.0",      # Mozilla Public License (weak copyleft, permissive for linking)
"Zlib",

# Public domain dedications
"0BSD",         # Zero-Clause BSD
"CC0-1.0",      # Creative Commons Zero
"Unlicense",
```

Also at `.config/deny.toml:35-36`.

Related: [advisory-suppressions-removed-after-cause-cleared](#advisory-suppressions-removed-after-cause-cleared), [bans-multiple-versions-warn-only](#bans-multiple-versions-warn-only).

**Remediation:** Drop BSL-1.0, MIT-0 and CC0-1.0 unless they are deliberate forward-looking allowances, in which case add a one-line comment saying so — the file already does exactly this for CDLA-Permissive-2.0, and that comment is the model to follow. Then set `unused-allowed-license = "warn"` so the list stays honest as the graph moves; with the stale entries removed first, that flip is quiet rather than noisy. Leave `confidence-threshold` at 0.8: it is the tool default and appropriate, and the fact that it is currently unreachable is a property of the graph, not a misconfiguration. The `bincode-next` SPDX typo is worth a one-line upstream PR, since it costs nothing and every downstream cargo-deny user is relying on lax parsing for it.

<div>&hairsp;</div>

### The base64 simd-unsafe opt-out is verified sound under --all-features, but a second base64 major is in the tree regardless {#base64-simd-unsafe-optout-holds}

**note** · `Cargo.toml:79-83` · effort: trivial · <img src="assets/sparkline-base64-simd-unsafe-optout-holds.svg" height="14" alt="commit activity" />

Cargo unifies features additively across a dependency graph, so a `default-features = false` opt-out is only as good as the rest of the tree — if any other crate depended on base64 0.23 with defaults on, `simd-unsafe` would be re-enabled for everyone and the stated intent silently defeated. That failure mode does not occur here. `cargo tree --all-features -e features -i base64@0.23.0` resolves exactly two features, `std` and its implied `alloc`, and `cargo tree -i` confirms librebar is the sole reverse dependency of 0.23.0. The opt-out is genuinely effective under the broadest possible feature set, which is the interesting result and the one worth recording, since it is not the default outcome and cannot be assumed. The same prefer-safe-backend discipline holds everywhere else it is selectable: `flate2` resolves through `rust_backend` -> `miniz_oxide` with no zlib-sys or C anywhere in the graph, and `tar` 0.4.46 contains zero C files. The residual note is that `base64` 0.22.1 is nonetheless present alongside 0.23.0, pulled by `rmcp`, `serde-saphyr` and `tonic`. That copy carries no safety implication — `simd-unsafe` did not exist before 0.23 — but it is a second full copy of the crate compiled and linked, and it is one of the five duplicates that the current `multiple-versions = "warn"` setting will never surface as a failure.

```toml Cargo.toml:79-83
# Feature: cache
# default-features = false to opt out of `simd-unsafe`, which base64 0.23
# added to its default feature set. `std` re-enables `alloc`; the cache module
# only needs STANDARD encode/decode, which the safe implementation provides.
base64 = { version = "0.23", default-features = false, features = ["std"], optional = true }
```

Related: [bans-multiple-versions-warn-only](#bans-multiple-versions-warn-only), [ring-is-the-sole-c-asm-island](#ring-is-the-sole-c-asm-island).

**Remediation:** Nothing to fix in the opt-out itself; it is correct and now verified. Two things preserve that state. First, the invariant rests on librebar being the only consumer of base64 0.23, which a future dependency could break with no visible change to this line — extend the existing comment to note that the opt-out depends on no other crate enabling base64's defaults, so a reader knows what to re-check after a dependency bump. Second, fold the 0.22/0.23 split into the duplicate ratchet described in the bans finding, so the second copy is either resolved when rmcp and serde-saphyr move to 0.23 or recorded as consciously accepted.

<div>&hairsp;</div>

### The advisory ignore list is empty and its history is documented — the suppression was removed once its cause cleared {#advisory-suppressions-removed-after-cause-cleared}

**note** · `.config/deny.toml:42-51` · effort: trivial · <img src="assets/sparkline-advisory-suppressions-removed-after-cause-cleared.svg" height="14" alt="commit activity" />

Advisory ignore lists are the standard place where supply-chain policy quietly rots: a suppression is added under deadline pressure, the upstream cause is fixed months later, and the entry survives indefinitely because nothing forces a re-check — so a future advisory against the same crate is silently swallowed. This repo does the rare, correct thing, and the claim in the comment is verifiable rather than aspirational. Confirming it against the current tree: `Cargo.lock` contains `bincode-next` 2.1.0 and no `bincode` at any version; `gungraun` 0.19.4's own manifest declares `dep:bincode-next` rather than `bincode`; and `gungraun` is the only path by which it enters, reachable solely through the non-default `bench-gungraun` feature. RUSTSEC-2025-0141 is therefore genuinely inapplicable, `ignore = []` is correct rather than optimistic, and `cargo audit` passes clean against all 306 lock entries. The comment also preserves the reasoning for why the entry once existed, which is what makes the removal auditable at all — a bare `ignore = []` would be indistinguishable from a policy that had never been exercised. Recording this as a finding because it is the behaviour the surrounding criteria assume and almost never observe, and because it sets the standard the two policy nits elsewhere in this file should be held to.

```toml .config/deny.toml:42-51
[advisories]
db-path = "~/.cargo/advisory-db"
db-urls = ["https://github.com/RustSec/advisory-db"]

# Accepted advisories.
#
# Empty. RUSTSEC-2025-0141 (bincode 1.3.3 unmaintained) was ignored here while
# gungraun pinned `bincode = "1"`. gungraun 0.19.4 migrated to `bincode-next`
# 2.x, so bincode 1.x is no longer in the graph and the suppression was removed.
ignore = []
```

Also at `Cargo.lock:187-191`.

Related: [license-allowlist-stale-entries](#license-allowlist-stale-entries).

**Remediation:** No action. Preserve the convention: when a suppression is added, record the upstream condition that would allow its removal in the same comment, and re-check the list whenever that dependency is bumped. The one durable improvement is to make the re-check automatic rather than remembered — with `ignore = []` there is nothing to expire today, but if a future entry is added, cargo-deny's per-entry `expiration` field turns "we should revisit this" into a date that fails the build on its own.

<div>&hairsp;</div>

*Verdict: This surface is in better shape than most published crates and should be read that way. `.config/deny.toml` carries an explicit license allowlist that documents its own non-obvious entries, and an advisory ignore list that is empty because the one suppression it held was removed once its cause cleared — a discipline almost nobody sustains. The gaps are narrow and specific: `serde-saphyr` is pinned to an exact `0.0.x` on a default-feature path, which is the one dependency in the tree that no `cargo update` can ever reach, and `ring` is the sole C and assembly island in an otherwise all-Rust graph, which is the honest boundary of this crate's zero-unsafe posture.*

<div>&nbsp;</div>

---

## The Configuration Discovery Surface

*Config discovery walks upward from the working directory and, in its default configuration, does not stop where it says it stops.*

### Config discovery escapes the .git boundary in the default case, walking to $HOME and / {#git-boundary-marker-inert-when-search-root-is-repo-root}

**moderate** · `src/config.rs:466-472` · effort: trivial · <img src="assets/sparkline-git-boundary-marker-inert-when-search-root-is-repo-root.svg" height="14" alt="commit activity" />

config.rs:34-35 documents the containment guarantee: "Search stops at a `.git` boundary by
default." ConfiguredBuilder::start uses std::env::current_dir() as the search root
(lib.rs:681), and users run CLIs from the repository root. On the first loop iteration
dir == start, so the `dir != start` conjunct is false and the break never fires even though
.git is right there. The walk then continues into every ancestor: the workspace parent, the
home directory, and /. In each one it probes three filenames per extension —
.config/{app}.{ext}, .{app}.{ext}, {app}.{ext} across toml, yaml, yml, json — and loads
the first hit into the merge chain above C::default(). The existing test
(tests/config_test.rs:552-574) only exercises the case where .git sits one level above the
search root, which is why the gap survived. Attacker: anyone who can write a file into a
directory that is an ancestor of a victim's working directory. Concretely — a shared CI
runner or build host where jobs check out under a common parent (an attacker's earlier job
leaves /builds/{app}.toml, and every later job at /builds/job-N picks it up); a container
image with a planted /{app}.toml, since the walk reaches / from a /app or /workspace cwd;
/Users/Shared or a world-writable /home on a multi-user box. Impact depends on what the
consuming app puts in its config type, and the README's documented pattern makes that
concrete: logging priority #3 is "log_dir from config" (README.md:435) via
LoggingConfig::with_log_dir, and ensure_writable (logging.rs:310-321) does create_dir_all
on that value and then opens {dir}/{service}.jsonl for append. So an attacker-supplied
log_dir yields attacker-chosen directory creation plus an append-mode file write as the
victim, and redirects the app's structured logs — including anything covered by
request-uri-with-credentials-recorded-in-log-spans — into a location the attacker reads.

```rust src/config.rs:466-472
// Check boundary after checking config (so same-dir config is found)
if let Some(ref marker) = self.boundary_marker
    && dir.join(marker).exists()
    && dir != start
{
    break;
}
```

Also at `src/config.rs:34-35`, `src/lib.rs:681-681`.

> The walk is supposed to stop at the repository. In the default configuration it does not, so it keeps climbing — through whatever shared parent directory your projects happen to sit in, toward `$HOME`, toward `/`. I do not need write access inside your repo. I need one config file one level above it.

Enables [request-uri-with-credentials-recorded-in-log-spans](#request-uri-with-credentials-recorded-in-log-spans).

**Remediation:** Drop the `dir != start` conjunct and reorder instead: check the three config filenames in
`dir`, then break if the boundary marker exists in `dir`, unconditionally. That preserves
the stated intent ("so same-dir config is found") because the config check already ran for
that directory, while making the boundary effective at the repo root. Add a regression test
with the search root equal to the directory containing .git and a config file planted in
its parent. Separately, consider treating discovered project config as lower-trust than
user config: log which ancestor supplied it (ConfigSources.project_file already carries
the path) and document that consuming apps should validate path-valued config fields such
as log_dir before handing them to the logging builder.

<div>&hairsp;</div>

### Project config discovery issues 12 stat probes and 24 heap allocations per directory level, and does not stop at the repository root it started from {#config-discovery-stat-fanout}

**advisory** · `src/config.rs:442-478` · effort: small · <img src="assets/sparkline-config-discovery-stat-fanout.svg" height="14" alt="commit activity" />

`CONFIG_EXTENSIONS` has four entries (src/config.rs:47) and each iteration probes three filename layouts, so a miss costs 12 `is_file()` calls — each a `stat(2)` — plus 12 `format!` Strings and 12 `Utf8PathBuf` joins, per directory level. One more stat and two more allocations go to the boundary check and the parent walk. The boundary marker does not bound the common case: the `dir != start` clause at line 469 means a `.git` in the *starting* directory is ignored, so a CLI invoked at its own project root walks past the root and keeps probing every ancestor up to `/`. From `/Users/clay/source/claylo/librebar` that is six levels, ~78 stats, ending with 24 probes for `librebar.toml` and friends in `/Users` and `/`. Measured end to end with a warm page cache, the whole `ConfigLoader::load` miss costs 63 µs from the repo root and 39 µs from `src/http`, against a full `librebar::init(...).start()` of 2.1 ms — so on a local SSD this is ~3% of startup and not the bottleneck. It is filed as advisory because the scaling, not the current value, is the problem: the cost is linear in cwd depth with no ceiling, and on a network filesystem where a stat is ~1 ms the same walk is ~78 ms of startup.

```rust src/config.rs:442-478
    fn find_project_config(&self, start: &Utf8Path) -> Option<Utf8PathBuf> {
        let mut current = Some(start.to_path_buf());

        while let Some(dir) = current {
            for ext in CONFIG_EXTENSIONS {
                // .config/app.ext
                let dotconfig = dir.join(format!(".config/{}.{ext}", self.app_name));
                if dotconfig.is_file() {
                    return Some(dotconfig);
                }

                // .app.ext
                let dotfile = dir.join(format!(".{}.{ext}", self.app_name));
                if dotfile.is_file() {
                    return Some(dotfile);
                }

                // app.ext
                let regular = dir.join(format!("{}.{ext}", self.app_name));
                if regular.is_file() {
                    return Some(regular);
                }
            }

            // Check boundary after checking config (so same-dir config is found)
            if let Some(ref marker) = self.boundary_marker
                && dir.join(marker).exists()
                && dir != start
            {
                break;
            }

            current = dir.parent().map(Utf8Path::to_path_buf);
        }

        None
    }
```

**Remediation:** Move the boundary check so a `.git` in the starting directory also terminates the walk — the stated intent at line 466 (find same-directory config first) is already satisfied by checking the boundary after the config probes, so the `dir != start` guard is not needed for that. Separately, hoist the filename construction out of the loop: build the three candidate names once per app_name rather than 12 times per directory level, using a single reused `String` with `clear()` between candidates. If the stat count itself needs to come down, a single `read_dir` per level replaces 12 stats with one directory read.

<div>&hairsp;</div>

*Verdict: The `.git` boundary marker is inert in the default case, so discovery continues past the repository root toward `$HOME` and `/`. That is a correctness problem before it is a security one — a config file in a parent directory of the repo silently participates in resolution — but it is also the path by which a file outside the project can influence a tool running inside it. The cost side is smaller and bounded: twelve stat probes and twenty-four allocations per directory level, paid once at startup, which matters for a CLI only because it is multiplied by however deep the user happens to be standing.*

<div>&nbsp;</div>

---

## The Dispatch and Self-Update Surface

*Plugin dispatch trusts PATH more than it should, and the update checker builds a URL from a string it never validates.*

### Plugin dispatch executes a binary from the current directory when PATH contains an empty entry {#dispatch-resolves-binary-from-current-directory}

**significant** · `src/dispatch.rs:36-39` · effort: small · <img src="assets/sparkline-dispatch-resolves-binary-from-current-directory.svg" height="14" alt="commit activity" />

which 8.0.5 splits PATH with std::env::split_paths, which yields an empty PathBuf for
adjacent or trailing separators. The filter that drops empty entries is gated behind
#[cfg(target_os = "windows")] (which-8.0.5/src/finder.rs:173), so on macOS and Linux an
empty entry becomes PathBuf::from("").join("myapp-deploy") == "myapp-deploy" — a bare
relative name resolved against the process working directory. librebar returns that
unvalidated to run(), which hands it to Command::new(); std then re-resolves the
separator-free name through execvp, hitting the same empty PATH entry and executing the
file in the cwd. I confirmed this empirically against the vendored crate: with
PATH="/usr/bin:/bin:" and cwd containing an executable named myapp-deploy,
which::which("myapp-deploy") returned "myapp-deploy" (is_absolute = false) and
Command::new(&resolved).status() ran the cwd script. Attacker: anyone who can place a
file into a directory the victim will cd into — a git repo, an extracted archive, an npm
or pip package directory, a shared downloads folder. They gain arbitrary code execution
as the invoking user with the full inherited environment. The PATH precondition (a
trailing colon, an adjacent "::", or a literal ".") is a common shell misconfiguration
and is itself reachable from repo-supplied .envrc, Makefile, or devcontainer files that
do PATH="$PATH:$MAYBE_EMPTY". Nothing else in the chain is validated: the resolved path
is not checked for absoluteness, ownership, or mode.

```rust src/dispatch.rs:36-39
pub fn resolve(app_name: &str, subcommand: &str) -> Option<PathBuf> {
    let binary = subcommand_binary(app_name, subcommand);
    which::which(&binary).ok()
}
```

Also at `src/dispatch.rs:62-64`.

> I do not need to write to any directory on your PATH. I need one empty entry in it — a trailing colon, a leading colon, a `::` in the middle — which is the kind of thing a hand-edited shell profile accumulates and nobody ever looks at again. POSIX says an empty entry means the current directory. So I put my binary in a repo you will clone, name it after a subcommand your tool advertises, and wait for you to `cd` in and type it.

**Remediation:** Reject non-absolute resolutions in resolve(): after which::which returns, drop any path
where !path.is_absolute() (or canonicalize and re-verify it lives under a PATH entry that
is itself absolute). Prefer filtering PATH before the lookup — build the candidate list
from std::env::split_paths, discard empty components and any relative component, and pass
the cleaned list to which::which_in. Optionally add a defense-in-depth check on the
resolved file (regular file, not group/world writable) before Command::new, mirroring what
git does for its own dashed-subcommand lookup.

<div>&hairsp;</div>

### Update checker interpolates an unvalidated version string into the release URL it shows the user {#cached-version-string-interpolated-into-release-url}

**advisory** · `src/update.rs:143-146` · effort: trivial · <img src="assets/sparkline-cached-version-string-interpolated-into-release-url.svg" height="14" alt="commit activity" />

First, the reassuring part: src/update.rs is a notifier, not a self-updater. It queries the
GitHub releases API over https, compares versions, and returns a message. It never
downloads an artifact, never writes a binary, and never executes anything — so the
unauthenticated-self-update class (missing signature, checksum from the same origin,
non-atomic replace, verify/install TOCTOU) does not apply to this crate. What does apply is
input validation on the version string. `latest` is read either from the GitHub response
or from the on-disk cache (update.rs:98-103, decoded with String::from_utf8 and used
directly), is validated by nothing, and is then interpolated into a github.com URL that
UpdateInfo::message() prints to the user's terminal as the place to get the update.
is_newer parses with `s.parse().unwrap_or(0)`, so any trailing garbage silently becomes 0
rather than rejecting the string. Concrete path: any process running as the same user that
is not the app itself — a malicious npm/pip postinstall, a compromised dev tool, a
dependency's build script — writes the cache entry at
~/Library/Caches/{app}/librebar/v1-bGF0ZXN0LXZlcnNpb24.json with an attacker-chosen value.
is_newer parses the leading digits and returns true, and the app then prints that value
twice: once as the version number and once as the tail of a
https://github.com/{repo}/releases/tag/v… URL the user is told to visit. The request
target is not at risk — a URI's authority ends at the first "/", and `latest` is
interpolated several slashes into the path, so the host stays github.com for every
possible value. What the attacker controls is the rendered text: a misleading tail on a
URL whose prefix looks official, and, since nothing filters the string, control characters
and escape sequences reaching the terminal through UpdateInfo::message()
(src/update.rs:37-42). The exposure is deception of the person reading the output, not
redirection of the fetch, and it costs one regex to close.

```rust src/update.rs:143-146
fn compare_versions(&self, latest: &str) -> Option<UpdateInfo> {
    let url = format!("https://github.com/{}/releases/tag/v{}", self.repo, latest);
    self.compare_versions_with_url(latest, &url)
}
```

Also at `src/update.rs:98-103`, `src/update.rs:166-168`.

> Self-update is the one code path whose entire purpose is to install something. Anything I can get interpolated into the URL you display is a string you are about to read and trust, at the exact moment you have decided to run whatever comes back.

**Remediation:** Validate before use. Reject any `latest` that does not match ^[0-9]+(\.[0-9]+){0,2}
(optionally with a -prerelease/+build suffix) in both compare_versions and the cache-read
path, returning None instead of building a URL. Make is_newer's parse failure meaningful —
return None from the comparison rather than coercing to 0 — so a malformed segment cannot
be treated as a valid version. Also validate the html_url taken from the API response
(update.rs:133) by parsing it and confirming the host is exactly github.com before it is
shown to the user.

<div>&hairsp;</div>

*Verdict: The dispatch finding is the classic one and it is real: an empty entry in PATH is interpreted as the current directory, so `librebar`-based CLIs will execute a plugin binary from whatever directory the user happens to be standing in. The exposure is bounded by the fact that `dispatch` is not a default feature and the attacker needs to influence both PATH and the working directory, but the fix is a two-line filter and there is no reason to carry it. The update-checker finding is smaller — an unvalidated version string interpolated into a release URL shown to the user — and matters mainly because self-update is the one code path where a mistake installs something.*

<div>&nbsp;</div>

---

## The Verification Apparatus Surface

*The crate carries scaffolding for measurement and for an unsafe escape hatch, and neither is connected to anything.*

### Two Cargo features, two optional dependencies, and two test files exist for benchmarking, and none of them benchmarks librebar {#bench-apparatus-measures-nothing}

**moderate** · `tests/bench_test.rs:1-8` · effort: medium · <img src="assets/sparkline-bench-apparatus-measures-nothing.svg" height="14" alt="commit activity" />

There is no `benches/` directory in the repository and no `bench-reports/` directory (Cargo.toml:20 excludes a `/bench-reports/` path that does not exist). `src/bench.rs` is a re-export shim: `pub use divan`, `pub use gungraun`, and a `BenchConfig` struct whose two fields are never read by any code in the crate. `tests/bench_test.rs` and `tests/bench_gungraun_test.rs` are eight lines each and assert only that the module compiles. The net effect is that the crate advertises wall-clock and instruction-count benchmarking as first-class features while having zero measurements of its own behavior. That is not a cosmetic gap: the two costs I measured for this audit — a 1 MiB HTTP cache hit at 10.3 ms of CPU, and a 5 ms fsync on every cache write — are exactly the kind of regression a `bench` feature exists to catch, and both shipped unnoticed. It also means the crate has no baseline against which the fixes in `http-cache-entry-body-amplification` and `cache-set-fsync-per-write` can be shown to work.

```rust tests/bench_test.rs:1-8
#![allow(missing_docs)]
#![cfg(feature = "bench")]

#[test]
fn bench_module_compiles() {
    // Verify the module is accessible
    let _ = std::any::type_name::<librebar::bench::BenchConfig>();
}
```

Also at `src/bench.rs:46-70`, `Cargo.toml:162-163`.

Enables [cache-set-fsync-per-write](#cache-set-fsync-per-write), [http-cache-entry-body-amplification](#http-cache-entry-body-amplification).

Related: [cargo-profiles-do-not-reach-consumers](#cargo-profiles-do-not-reach-consumers).

**Remediation:** Either add a `benches/` directory that exercises the paths that actually dominate this crate's cost — `Cache::set`/`Cache::get` round-trip at several body sizes, `ConfigLoader::load` from a representative directory depth, `JsonLogLayer::on_event` inside a nested span, and full `librebar::init(...).start()` — or drop `BenchConfig` and document `bench`/`bench-gungraun` honestly as dependency re-exports for downstream crates, which is all they currently are. The first option is the one that pays for itself; a divan bench over `Cache` alone would have surfaced both findings above.

<div>&hairsp;</div>

### Crate-wide unsafe_code escape hatch is justified by benchmark harnesses that do not exist {#unsafe-escape-hatch-rationale-does-not-match-use}

**advisory** · `Cargo.toml:165-168` · effort: trivial · <img src="assets/sparkline-unsafe-escape-hatch-rationale-does-not-match-use.svg" height="14" alt="commit activity" />

The crate deliberately chooses `deny` over `forbid` for `unsafe_code`, and the comment records the reason as benchmark instrumentation. That justification does not hold: the repository contains no `benches/` directory, no `[[bench]]` target in Cargo.toml, and `src/bench.rs` (the only file behind the `bench` and `bench-gungraun` features) contains no `unsafe` and no `#[allow(unsafe_code)]`. The escape hatch is real and is exercised, but by two integration test files for an unrelated reason — `tests/otel_test.rs:1` and `tests/update_test.rs:1` both carry `#![allow(missing_docs, unsafe_code)]` so they can call `std::env::set_var`/`remove_var`, which became `unsafe` fns in Rust 2024. Those two uses are themselves sound: each file serializes env mutation behind a file-local `ENV_LOCK: Mutex<()>`, each `unsafe` block carries an accurate `// SAFETY:` comment, the blocks are minimally scoped, and the project's own runner (`cargo nextest run`, per `.justfile:30`) gives each test its own process. The defect is documentary, not behavioural: an auditor reading Cargo.toml is pointed at benchmark code to explain why the crate's headline memory-safety guarantee is weakened, finds nothing there, and has no pointer to the two files that actually hold the override. A stale rationale on a lint that gates the crate's strongest safety claim erodes the value of that claim.

```toml Cargo.toml:165-168
[lints.rust]
# Use "deny" instead of "forbid" to allow benchmark harnesses to override
# when their instrumentation machinery requires it
unsafe_code = "deny"
```

**Remediation:** Update the comment to name the real consumer — env-var mutation in integration tests under Rust 2024 — and reference `tests/otel_test.rs` and `tests/update_test.rs` explicitly. Better still, close the hatch for library code — but not through `[lints]`. That table is package-wide and applies to every target including integration tests, and Cargo has no per-target lints table to scope around it, so setting `unsafe_code = "forbid"` there would make the `#![allow(missing_docs, unsafe_code)]` at tests/otel_test.rs:1 and tests/update_test.rs:1 a hard error and break both tests — precisely the situation `deny` was chosen to avoid. The implementable form of the same intent is to drop `unsafe_code` from `[lints.rust]` entirely and change src/lib.rs:142 from `#![deny(unsafe_code)]` to `#![forbid(unsafe_code)]`, which binds the library target only and leaves the two test targets free. If the benchmark rationale is meant to be forward-looking, say so, or drop it until a bench target actually needs it.

<div>&hairsp;</div>

### The tuned Cargo profiles affect only local development, and two of the settings restate Cargo's defaults {#cargo-profiles-do-not-reach-consumers}

**note** · `Cargo.toml:188-217` · effort: trivial · <img src="assets/sparkline-cargo-profiles-do-not-reach-consumers.svg" height="14" alt="commit activity" />

Cargo honours `[profile.*]` only from the manifest of the package being built as the workspace root. librebar is published to crates.io as a library, so for every consumer these thirty lines are inert — a downstream binary gets the stock release profile regardless of what `[profile.release]` says here. That is worth stating plainly because it reframes the block: it is local development-experience tuning, not shipped performance, and questions like "should release get thin LTO, panic=abort, or strip" are the consumer's call and cannot be answered from this manifest. Two settings are also no-ops on their own terms: `codegen-units = 256` is already Cargo's default for dev and test, so line 193 and line 198 change nothing. `[profile.dev.package."*"] opt-level = 1` (Cargo.toml:178-180) is partly redundant with `[profile.dev] opt-level = 1`, which already applies to dependencies; only its `debug = 0` is doing work. And `[profile.bench]` tunes a profile the repository never invokes, since there is no `benches/` directory.

```toml Cargo.toml:188-217
[profile.dev]
debug = "line-tables-only"
opt-level = 1
# On macOS, keep debug info from exploding into huge scattered dSYM trees
split-debuginfo = "packed"
codegen-units = 256

# Test profile with full debug symbols for profiling
[profile.test]
inherits = "dev"
codegen-units = 256
debug = true

# Release profile with reduced debug info
[profile.release]
debug = "line-tables-only"

# Debug-friendly profile for investigating mysteries (hangs, crashes, etc.)
# Usage: cargo build --profile mystery
[profile.mystery]
inherits = "release"
opt-level = 1
debug = true
strip = false

# Optimized profile for benchmarks
[profile.bench]
inherits = "release"
lto = "thin"
debug = "line-tables-only"
```

Related: [bench-apparatus-measures-nothing](#bench-apparatus-measures-nothing).

**Remediation:** Delete the two `codegen-units = 256` lines and the redundant `opt-level` in the `package."*"` override, keeping `debug = 0` there. Add a one-line comment at the head of the block recording that these profiles apply only when building librebar itself, so a future reader does not try to tune consumer performance from here. Keep `[profile.bench]` only if the benches from `bench-apparatus-measures-nothing` get written; otherwise drop it with the rest of that apparatus. Guidance for consumers on release tuning belongs in the README, not in a profile table they will never see.

<div>&hairsp;</div>

*Verdict: Two agents arrived at this from opposite ends without seeing each other's work. The safety auditor asked why `unsafe_code` is set to `deny` rather than `forbid` and found the manifest's answer — that benchmark harnesses need to override it — describes harnesses that do not exist. The performance agent, independently, found that two cargo features, two optional dependencies, and two test files exist for benchmarking and none of them benchmarks librebar. Same root cause from both directions: the apparatus was built before the thing it was meant to measure. Tightening the lint to `forbid` is free today and stops being free the moment a real harness appears, which is an argument for doing it now and revisiting deliberately.*

<div>&nbsp;</div>

---

## The Remediation Ledger

One row per finding, grouped by narrative in report order — not by severity.

### The Release Boundary Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Every CI job builds --all-features; no job builds the default set or any individual feature](#ci-builds-only-all-features) | significant | `.github/workflows/ci.yml:68-72` | small | [ci-reimplements-justfile-recipes](#ci-reimplements-justfile-recipes), [serde-saphyr-exact-pin-on-default-path](#serde-saphyr-exact-pin-on-default-path) |
| [No `[package.metadata.docs.rs]` and no `doc_auto_cfg`: 12 of 18 named features are invisible on docs.rs](#docs-rs-publishes-only-default-features) | significant | `Cargo.toml:144-163` | trivial | — |
| [crates.io publishing is fully manual — no automated workflow, no Trusted Publishing, no attestation](#no-attested-publish-path) | moderate | `.config/scrat.toml:1-8` | medium | — |
| [The doc-rot gate stops at `src/`: README.md's 16 Rust blocks are compiled by nothing](#readme-code-blocks-outside-the-doc-test-gate) | advisory | `.justfile:35-40` | medium | — |
| [CI never invokes just; it reimplements each recipe as a cargo command, and the drift hazard is self-documented](#ci-reimplements-justfile-recipes) | note | `.github/workflows/ci.yml:82-92` | small | [ci-builds-only-all-features](#ci-builds-only-all-features) |
| [Five unresolved intra-doc links to `Error::Cache` / `Error::Http` in public `# Errors` sections](#unresolved-intra-doc-error-links) | note | `src/cache.rs:66-71` | trivial | [docs-rs-publishes-only-default-features](#docs-rs-publishes-only-default-features) |
| [No CONTRIBUTING.md and no status badges on a published crate with full issue/PR template scaffolding](#missing-contributing-and-status-badges) | note | `README.md:1-3` | small | — |

### The Diagnostics and Disclosure Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Debug bundle performs no redaction and writes a 0644 archive intended for public bug reports](#debug-bundle-ships-unredacted-content-world-readable) | significant | `src/diagnostics.rs:211-215` | medium | [crash-dump-world-readable-and-unbounded](#crash-dump-world-readable-and-unbounded), [request-uri-with-credentials-recorded-in-log-spans](#request-uri-with-credentials-recorded-in-log-spans) |
| [Full request URI, including userinfo and query string, is recorded as a tracing span field](#request-uri-with-credentials-recorded-in-log-spans) | significant | `src/http.rs:639-643` | small | [crash-dump-world-readable-and-unbounded](#crash-dump-world-readable-and-unbounded), [response-debug-impl-exposes-body-and-set-cookie](#response-debug-impl-exposes-body-and-set-cookie) |
| [Crash dumps are written with default permissions and never pruned](#crash-dump-world-readable-and-unbounded) | moderate | `src/crash.rs:120-133` | small | [crash-dumps-documented-as-json-are-free-text](#crash-dumps-documented-as-json-are-free-text), [crash-hook-print-turns-panics-into-aborts](#crash-hook-print-turns-panics-into-aborts) |
| [DebugBundle holds every file in RAM and copies each one twice, with no streaming API](#debug-bundle-buffers-entire-archive-in-memory) | moderate | `src/diagnostics.rs:192-221` | medium | — |
| [Response derives Debug over the full body and header map, including Set-Cookie](#response-debug-impl-exposes-body-and-set-cookie) | advisory | `src/http/response.rs:57-63` | small | [request-uri-with-credentials-recorded-in-log-spans](#request-uri-with-credentials-recorded-in-log-spans) |
| [Crash dumps are documented as structured JSON but are unparseable free text](#crash-dumps-documented-as-json-are-free-text) | advisory | `src/crash.rs:50-61` | medium | [crash-dump-world-readable-and-unbounded](#crash-dump-world-readable-and-unbounded) |
| [DebugBundle mixes &mut Self chaining with a consuming finish()](#debug-bundle-builder-cannot-be-chained) | moderate | `src/diagnostics.rs:210-231` | trivial | [doctor-check-registration-forces-caller-boxing](#doctor-check-registration-forces-caller-boxing) |

### The HTTP Cache Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Cached HTTP bodies are written through two serialization layers, inflating them 4.6x and costing 10 ms of CPU per 1 MiB cache hit](#http-cache-entry-body-amplification) | significant | `src/http/cache.rs:561-584` | medium | [cache-set-fsync-per-write](#cache-set-fsync-per-write) |
| [Async HTTP cache and update-check paths perform blocking filesystem I/O, including two fsyncs per write](#blocking-fsync-on-async-cache-paths) | significant | `src/http/cache.rs:421-444` | small | [signal-task-exits-after-first-signal](#signal-task-exits-after-first-signal), [cache-expiry-unlink-races-concurrent-write](#cache-expiry-unlink-races-concurrent-write) |
| [HTTP cache fingerprints three credential headers and writes every other request header to disk verbatim](#http-cache-persists-unrecognized-credential-headers) | moderate | `src/http/cache.rs:17-18` | small | [request-uri-with-credentials-recorded-in-log-spans](#request-uri-with-credentials-recorded-in-log-spans) |
| [On-disk cache never prunes: expired entries are removed only when the same key is read again](#cache-has-no-eviction-outside-per-key-reads) | moderate | `src/cache.rs:99-126` | medium | [http-cache-entry-body-amplification](#http-cache-entry-body-amplification) |
| [Every cache write pays a full-drive fsync (~5 ms measured) for data the code already treats as disposable](#cache-set-fsync-per-write) | moderate | `src/cache.rs:178-190` | small | [http-cache-entry-body-amplification](#http-cache-entry-body-amplification) |
| [Six cache eviction results discarded with let _ =, while the same call is logged elsewhere](#http-cache-eviction-results-discarded) | advisory | `src/http/cache.rs:337-347` | trivial | [update-check-drops-errors-it-documents-as-logged](#update-check-drops-errors-it-documents-as-logged) |
| [Expiry cleanup unlinks the cache path unconditionally, discarding a concurrent writer's fresh entry](#cache-expiry-unlink-races-concurrent-write) | note | `src/cache.rs:109-119` | trivial | [blocking-fsync-on-async-cache-paths](#blocking-fsync-on-async-cache-paths) |

### The Transport and Cookie Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Cookie jar compiles the public-suffix feature but never installs the list, accepting supercookies](#cookie-jar-never-installs-public-suffix-list) | significant | `src/http/cookies.rs:20-23` | medium | [cross-origin-redirect-forwards-non-blocklisted-credentials](#cross-origin-redirect-forwards-non-blocklisted-credentials) |
| [Redirect follower strips only three header names and permits HTTPS-to-HTTP downgrade](#cross-origin-redirect-forwards-non-blocklisted-credentials) | significant | `src/http.rs:365-371` | small | [cookie-jar-never-installs-public-suffix-list](#cookie-jar-never-installs-public-suffix-list), [webpki-root-store-is-compiled-in](#webpki-root-store-is-compiled-in) |
| [Cookie jar read/write lock failures silently drop cookies from requests and responses](#cookie-jar-failures-are-silent) | moderate | `src/http/cookies.rs:77-91` | small | — |
| [Cookie jar enforces no per-domain or total cookie limit on origin-supplied Set-Cookie headers](#cookie-jar-accepts-unbounded-cookie-count) | advisory | `src/http/cookies.rs:104-115` | small | — |
| [TLS trust anchors are compiled into the binary and only refresh on a dependency bump](#webpki-root-store-is-compiled-in) | note | `Cargo.toml:65-65` | trivial | [cross-origin-redirect-forwards-non-blocklisted-credentials](#cross-origin-redirect-forwards-non-blocklisted-credentials) |

### The Process Lifecycle Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Panic hook prints with eprintln!, converting any panic into SIGABRT when stderr is broken](#crash-hook-print-turns-panics-into-aborts) | significant | `src/crash.rs:101-113` | trivial | [crash-dump-world-readable-and-unbounded](#crash-dump-world-readable-and-unbounded), [print-macros-panic-where-errors-cannot-propagate](#print-macros-panic-where-errors-cannot-propagate) |
| [Signal task handles exactly one signal, leaving the process permanently un-interruptible](#signal-task-exits-after-first-signal) | significant | `src/shutdown.rs:69-96` | small | [blocking-fsync-on-async-cache-paths](#blocking-fsync-on-async-cache-paths), [ctrl-c-registration-error-triggers-shutdown](#ctrl-c-registration-error-triggers-shutdown) |
| [A failed ctrl_c handler registration is discarded and read as a received signal](#ctrl-c-registration-error-triggers-shutdown) | moderate | `src/shutdown.rs:79-93` | trivial | [signal-task-exits-after-first-signal](#signal-task-exits-after-first-signal) |
| [eprintln!/println! used inside Drop, a tracing Layer callback, and a fallible startup path](#print-macros-panic-where-errors-cannot-propagate) | moderate | `src/otel.rs:100-106` | trivial | [crash-hook-print-turns-panics-into-aborts](#crash-hook-print-turns-panics-into-aborts) |
| [wait_to_retry decrements an unsigned counter without checking it, invariant undocumented](#retry-counter-decrement-relies-on-caller-invariant) | note | `src/http.rs:896-902` | trivial | — |

### The Telemetry Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [OTLP span export is wired to a batch processor that cannot drive the selected hyper HTTP client](#otel-batch-processor-cannot-drive-hyper-exporter) | significant | `src/otel.rs:139-153` | medium | — |
| [`OTEL_EXPORTER_OTLP_PROTOCOL=http/json` is documented but the `http-json` exporter feature is never enabled](#otel-http-json-protocol-not-buildable) | moderate | `src/otel.rs:156-175` | small | [otel-config-env-var-name-fields-unread](#otel-config-env-var-name-fields-unread), [otel-grpc-feature-has-no-test](#otel-grpc-feature-has-no-test) |
| [Every log event clones the complete field map of every enclosing span, then immediately destroys the clone](#log-event-clones-span-field-map) | moderate | `src/logging.rs:399-409` | trivial | — |
| [`OtelConfig::env_var_protocol` and `env_var_endpoint` are public, documented as configuration, and read by no code path](#otel-config-env-var-name-fields-unread) | advisory | `src/otel.rs:45-51` | trivial | [otel-http-json-protocol-not-buildable](#otel-http-json-protocol-not-buildable) |
| [`otel-grpc` is the only one of the 19 features with no integration test exercising it](#otel-grpc-feature-has-no-test) | note | `src/otel.rs:160-166` | trivial | [otel-http-json-protocol-not-buildable](#otel-http-json-protocol-not-buildable) |
| [The log-directory writability probe creates a file the appender never writes to, leaving a zero-byte file behind on every run](#log-writability-probe-creates-unused-file) | note | `src/logging.rs:309-322` | trivial | — |

### The Public API Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Dependency types appear in public signatures without a matching re-export](#unreachable-dependency-types-in-public-api) | significant | `src/lib.rs:198-206` | medium | [dependency-error-payloads-are-unwrappable](#dependency-error-payloads-are-unwrappable) |
| [Public error variants carry third-party error types the caller cannot name](#dependency-error-payloads-are-unwrappable) | significant | `src/error.rs:191-208` | medium | [growable-public-structs-lack-non-exhaustive](#growable-public-structs-lack-non-exhaustive), [unreachable-dependency-types-in-public-api](#unreachable-dependency-types-in-public-api) |
| [Config-shaped public structs are exhaustive, and one has a cfg-gated field](#growable-public-structs-lack-non-exhaustive) | significant | `src/http.rs:130-148` | small | [dependency-error-payloads-are-unwrappable](#dependency-error-payloads-are-unwrappable), [schema-wire-types-are-serialize-only](#schema-wire-types-are-serialize-only) |
| [UpdateChecker builds its own HTTP client and cache and hardcodes GitHub](#update-checker-hardcodes-github-and-its-collaborators) | significant | `src/update.rs:91-109` | medium | [environment-source-trait-over-constrains-implementors](#environment-source-trait-over-constrains-implementors) |
| [EnvironmentSource requires Debug and forces implementors to materialize everything](#environment-source-trait-over-constrains-implementors) | moderate | `src/config/environment.rs:9-13` | small | [update-checker-hardcodes-github-and-its-collaborators](#update-checker-hardcodes-github-and-its-collaborators) |
| [DoctorRunner::add requires a pre-boxed check and DoctorCheck demands an unused Send](#doctor-check-registration-forces-caller-boxing) | advisory | `src/diagnostics.rs:96-110` | trivial | [debug-bundle-builder-cannot-be-chained](#debug-bundle-builder-cannot-be-chained) |
| [CLI Spec document types can be written but not read back](#schema-wire-types-are-serialize-only) | advisory | `src/cli/schema.rs:133-145` | small | [growable-public-structs-lack-non-exhaustive](#growable-public-structs-lack-non-exhaustive) |

### The Error Architecture Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Eight Error variants wrap an error without exposing it through source()](#error-variants-drop-the-source-chain) | moderate | `src/error.rs:73-106` | small | [lock-error-message-misreports-the-cause](#lock-error-message-misreports-the-cause), [error-display-duplicates-its-source](#error-display-duplicates-its-source) |
| [Every try_lock failure is reported as contention, including real I/O errors](#lock-error-message-misreports-the-cause) | moderate | `src/lockfile.rs:107-112` | trivial | [error-variants-drop-the-source-chain](#error-variants-drop-the-source-chain) |
| [Display interpolates the source it also returns from source(), producing repeated messages](#error-display-duplicates-its-source) | advisory | `src/error.rs:88-96` | small | [error-variants-drop-the-source-chain](#error-variants-drop-the-source-chain) |
| [Update check discards three failure paths that its docs promise are logged](#update-check-drops-errors-it-documents-as-logged) | advisory | `src/update.rs:106-117` | trivial | [http-cache-eviction-results-discarded](#http-cache-eviction-results-discarded) |
| [Lockfile promises unconditional cross-process exclusion and reports every lock error as contention](#lockfile-exclusion-guarantee-unqualified) | advisory | `src/lockfile.rs:95-120` | trivial | [lock-error-message-misreports-the-cause](#lock-error-message-misreports-the-cause) |
| [Lock directory falls back to /tmp, letting a local user pre-create the path and hold the lock](#lockfile-falls-back-to-shared-tmp-on-linux) | advisory | `src/lockfile.rs:30-36` | small | — |

### The Supply Chain Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [serde-saphyr sits at 0.0.29, where Cargo's compatibility range is a single version, on a default-feature path](#serde-saphyr-exact-pin-on-default-path) | significant | `Cargo.toml:38-40` | small | [ci-builds-only-all-features](#ci-builds-only-all-features) |
| [The crate's zero-unsafe guarantee stops at the crypto boundary: ring contributes 17 C files and 90 assembly files behind a build script](#ring-is-the-sole-c-asm-island) | note | `Cargo.toml:64-66` | trivial | [base64-simd-unsafe-optout-holds](#base64-simd-unsafe-optout-holds) |
| [The duplicate-version check warns rather than fails, and CI is the only place the warnings appear](#bans-multiple-versions-warn-only) | advisory | `.config/deny.toml:57-62` | small | [base64-simd-unsafe-optout-holds](#base64-simd-unsafe-optout-holds), [license-allowlist-stale-entries](#license-allowlist-stale-entries) |
| [Three allowlisted licenses match nothing in the graph, and the setting that would flag them is disabled](#license-allowlist-stale-entries) | note | `.config/deny.toml:14-24` | trivial | [advisory-suppressions-removed-after-cause-cleared](#advisory-suppressions-removed-after-cause-cleared), [bans-multiple-versions-warn-only](#bans-multiple-versions-warn-only) |
| [The base64 simd-unsafe opt-out is verified sound under --all-features, but a second base64 major is in the tree regardless](#base64-simd-unsafe-optout-holds) | note | `Cargo.toml:79-83` | trivial | [bans-multiple-versions-warn-only](#bans-multiple-versions-warn-only), [ring-is-the-sole-c-asm-island](#ring-is-the-sole-c-asm-island) |
| [The advisory ignore list is empty and its history is documented — the suppression was removed once its cause cleared](#advisory-suppressions-removed-after-cause-cleared) | note | `.config/deny.toml:42-51` | trivial | [license-allowlist-stale-entries](#license-allowlist-stale-entries) |

### The Configuration Discovery Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Config discovery escapes the .git boundary in the default case, walking to $HOME and /](#git-boundary-marker-inert-when-search-root-is-repo-root) | moderate | `src/config.rs:466-472` | trivial | [request-uri-with-credentials-recorded-in-log-spans](#request-uri-with-credentials-recorded-in-log-spans) |
| [Project config discovery issues 12 stat probes and 24 heap allocations per directory level, and does not stop at the repository root it started from](#config-discovery-stat-fanout) | advisory | `src/config.rs:442-478` | small | — |

### The Dispatch and Self-Update Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Plugin dispatch executes a binary from the current directory when PATH contains an empty entry](#dispatch-resolves-binary-from-current-directory) | significant | `src/dispatch.rs:36-39` | small | — |
| [Update checker interpolates an unvalidated version string into the release URL it shows the user](#cached-version-string-interpolated-into-release-url) | advisory | `src/update.rs:143-146` | trivial | — |

### The Verification Apparatus Surface

| Finding | Concern | Location | Effort | Chains |
|---|---|---|---|---|
| [Two Cargo features, two optional dependencies, and two test files exist for benchmarking, and none of them benchmarks librebar](#bench-apparatus-measures-nothing) | moderate | `tests/bench_test.rs:1-8` | medium | [cache-set-fsync-per-write](#cache-set-fsync-per-write), [http-cache-entry-body-amplification](#http-cache-entry-body-amplification), [cargo-profiles-do-not-reach-consumers](#cargo-profiles-do-not-reach-consumers) |
| [Crate-wide unsafe_code escape hatch is justified by benchmark harnesses that do not exist](#unsafe-escape-hatch-rationale-does-not-match-use) | advisory | `Cargo.toml:165-168` | trivial | — |
| [The tuned Cargo profiles affect only local development, and two of the settings restate Cargo's defaults](#cargo-profiles-do-not-reach-consumers) | note | `Cargo.toml:188-217` | trivial | [bench-apparatus-measures-nothing](#bench-apparatus-measures-nothing) |

