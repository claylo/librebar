# CLI Generated Artifacts Implementation Plan

**Status:** Implemented

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Do not create a worktree.

**Goal:** Generate stable shell completions and manpages from the exact same
augmented Clap command tree used for parsing and CLI Spec introspection.

**Architecture:** The parser adds a librebar-owned `completions` terminal
subcommand and renders it through `clap_complete`. A focused artifact module
wraps `clap_mangen` for build/release-time manpage generation. Both receive the
already augmented command tree so help, schema, completions, and manpages cannot
drift.

**Tech Stack:** Rust 2024, Clap 4.6, `clap_complete`, `clap_mangen`.

**Design:**
`record/superpowers/specs/2026-07-31-agent-ready-cli-foundation.md`

---

### Task 1: Add stable shell completions as a terminal command

**Files:** `Cargo.toml`, `src/cli/parse.rs`, `tests/cli_test.rs`, `README.md`

- [x] Add failing tests proving help exposes `completions`, Bash and Zsh output
  are nonempty and name the consumer binary, and consumer command collisions
  are rejected.
- [x] Add `clap_complete` as an optional dependency enabled by `cli`; do not
  enable its unstable dynamic-completion feature.
- [x] Add `ParseOutcome::Completions(Vec<u8>)`, the synthetic
  `completions <SHELL>` command, and rendering through `clap_complete::generate`.
- [x] Run the targeted tests and full `cli_test`; expect PASS.

### Task 2: Add release-time manpage generation

**Files:** `Cargo.toml`, `src/cli/artifacts.rs`, `src/cli.rs`, `tests/cli_test.rs`, `README.md`

- [x] Add failing tests that render the root page and write collision-free
  pages for nested subcommands into a temporary directory.
- [x] Add `clap_mangen` as an optional dependency enabled by `cli` and implement
  writer/directory APIs that recurse over the augmented command tree.
- [x] Ensure generated filenames use the full command path and include the
  librebar-owned schema/completions commands.
- [x] Run targeted tests, all CLI tests, examples, and doc-tests; expect PASS.

### Task 3: Verify and record the artifact slice

**Files:** `scratch/TODO.txt`, this plan

- [x] Update the TODO ledger and plan checkboxes.
- [x] Run `just check`, `git --no-pager diff --check`, and inspect the final
  diff/stat.
- [x] Leave the worktree uncommitted for Clay's `commit.txt`/`gtxt` workflow.
