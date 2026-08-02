## [0.4.0] - 2026-08-02

### 🚀 Features

- *(core)* Complete config and HTTP foundation
- *(cli)* Add agent-ready command contracts
- *(core)* Enable the application foundation by default
- *(http)* Add production defaults and cookie jars
- *(http)* Add conditional requests and RFC-aware caching (#25)
- *(update)* Add pluggable release sources
- *(config)* Preserve merged value provenance
- *(diagnostics)* Accept unboxed doctor checks
- *(cli)* Make schema documents readable

### 🐛 Bug Fixes

- *(cli)* Stop CommonArgs rustdoc becoming the consumer's help text
- *(cli)* Make --version-only global like every other common flag
- *(release)* Cover feature matrix and docs.rs surface
- *(diagnostics)* Redact and privatize debug bundles
- *(logging)* Keep request credentials out of logs
- *(crash)* Privatize and bound crash dumps
- *(diagnostics)* Stream sanitized files into bundles
- *(http)* Redact response debug output
- *(crash)* Write structured JSON crash dumps
- *(diagnostics)* Make debug bundles chainable
- *(cache)* Store HTTP bodies as raw bytes
- *(cache)* Keep filesystem I/O off async workers
- *(http)* Fingerprint cached request headers
- *(cache)* Prune expired entries during active writes
- *(cache)* Avoid durable syncs for disposable entries
- *(cache)* Log HTTP cache eviction failures
- *(cache)* Avoid unlinking expired entries on read
- *(http)* Reject public-suffix domain cookies
- *(http)* Harden redirect trust boundaries
- *(http)* Recover poisoned cookie jars
- *(http)* Bound cookie jar growth
- *(crash)* Make panic-hook notices fallible
- *(shutdown)* Escalate repeated process signals
- *(output)* Make fallback writes fallible
- *(http)* Saturate retry budget consumption
- *(otel)* Drive Hyper exports on a private runtime
- *(otel)* Honor HTTP JSON protocol selection
- *(otel)* Remove inert environment name fields
- *(otel)* Drive gRPC export on a private runtime
- *(logging)* Probe the daily log path
- *(api)* Define public dependency boundaries
- *(error)* Preserve dependency source chains
- *(api)* Stabilize growable public structs
- *(lockfile)* Distinguish contention from failures
- *(error)* Avoid duplicate source messages
- *(lockfile)* Avoid shared Linux lock paths
- *(config)* Honor boundary at project search root
- *(dispatch)* Ignore unsafe PATH entries
- *(update)* Validate release metadata
- *(safety)* Forbid unsafe in library target
- *(safety)* Audit remediation across all modules

### 💼 Other

- *(deps)* Adopt serde-saphyr 1.0
- *(deps)* Update and ratchet dependency graph
- *(policy)* Tighten supply-chain exceptions
- *(cargo)* Simplify local profile settings

### 📚 Documentation

- *(api)* Enforce valid intra-doc links
- *(project)* Add contributor guide and status badges
- *(cache)* Define cache write durability policy
- *(cache)* Define eviction observability policy
- *(http)* Document compiled TLS trust anchors
- *(lockfile)* Qualify advisory locking guarantees

### ⚡ Performance

- *(logging)* Avoid cloning span field maps
- *(config)* Reduce discovery filesystem probes

### 🧪 Testing

- *(docs)* Compile README examples as doctests
- *(bench)* Measure cache reads and writes

### ⚙️ Miscellaneous Tasks

- Tune bito config
- *(release)* Add attested crates.io publishing
- *(checks)* Run shared Just recipes
- Exclude record dir from packaging, consolidate superpowers, add audits
## [0.3.0] - 2026-07-26

### 🚀 Features

- *(cli)* Add CommonArgs::apply and re-export camino (#24)

### 📚 Documentation

- Add latest handoff

### ⚙️ Miscellaneous Tasks

- Release 0.3.0
## [0.2.0] - 2026-07-25

### 🚀 Features

- *(mcp)* [**breaking**] Migrate to rmcp 2.2, update all dependencies (#22)

### ⚙️ Miscellaneous Tasks

- Release 0.2.0
- Fix cargo-deny invocation, bump all action pins (#23)
## [0.1.0] - 2026-05-26

### 🚀 Features

- Initial rebar crate with feature-gated module stubs
- *(cli)* Add CommonArgs, ColorChoice, and HelpShort helper
- *(config)* Add config merge, file parsing, and discovery
- *(logging)* Add JSONL log layer, log target resolution, and env_filter
- Add builder and App orchestration layer
- Add Phase 2 feature flags and module stubs
- *(crash)* Add panic hook with structured crash dumps
- Add otel and mcp modules
- *(phase3)* Add Phase 3 dependencies and module stubs
- *(builder)* Add .with_version() to Builder and ConfiguredBuilder
- *(lockfile)* Add exclusive operation locking via fs4
- *(http)* Add HTTP client with h2/h1 negotiation and tracing
- *(cache)* Add XDG cache storage with TTL support
- *(update)* Add GitHub release version checking with cache
- *(dispatch)* Add git-style external command dispatch
- *(diagnostics)* Add doctor framework and debug bundle builder
- *(bench)* Add divan and gungraun benchmark harness helpers
- Phase 3 — cache, update, dispatch, diagnostics, bench modules
- *(http)* Add TLS support via rustls with Mozilla CA roots
- *(examples)* Add minimal example exercising cli, config, logging (#3)
- *(examples)* Add service example exercising shutdown, crash, and otel (#4)
- *(examples)* Add updater example exercising http, cache, update (#6)
- *(examples)* Add plugin-cli example exercising external subcommand dispatch (#7)
- *(examples)* Add doctor-bundle example exercising diagnostics (#11)
- *(examples)* Add mcp-server example exposing a single tool over stdio (#12)
- *(error)* Mark Error and companion enums `#[non_exhaustive]` (#13)
- *(examples)* Add mcp-server `call` subcommand for self-contained round-trip (#15)

### 🐛 Bug Fixes

- Resolve six audit findings across config, startup, and shutdown surfaces
- Applied fixes to generated audit report
- Remediate cased audit findings (10 fixed, 1 accepted, 2 deferred)
- Remediate cased audit findings (12 fixed, 1 accepted)
- Add 2026-04-11 audit and remediate findings (3 fixed, 1 accepted) (#1)
- *(tests)* Make network tests opt-in explicit, serialize env-var tests, document run recipes (#8)
- *(docs)* Compile the four flagged rustdoc examples and verify them in CI (#10)
- *(docs)* Compile the remaining 11 rustdoc examples (#14)

### 🚜 Refactor

- *(logging)* Split build_json_layer from init for composable layers

### 📚 Documentation

- Add README with usage guide
- Add comprehensive inline documentation
- Design and progress docs
- Current handoff
- Add feature reference guide to lib.rs
- Latest handoff
- Add cased audit and handoff
- Update latest handoff
- Add latest handoff
- Add handoff because we build in the open (#5)
- Add semver policy to README (#9)
- Update README (#21)

### 🧪 Testing

- *(config)* Add discovery and boundary marker tests

### ⚙️ Miscellaneous Tasks

- Format update module and verify Phase 3 feature isolation
- Rename crate to librebar and port launch scaffolding (#2)
- Update dev channel, clean up readme (#17)
- Release 0.1.0
