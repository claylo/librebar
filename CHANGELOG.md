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
