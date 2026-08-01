# Contributing to Librebar

Keep changes focused, test what they change, and make the pull request tell
the truth about both.

## Before you start

- Use [GitHub Discussions](https://github.com/claylo/librebar/discussions) for
  design questions or open a feature request for proposed behavior.
- Report vulnerabilities privately through the process in
  [SECURITY.md](SECURITY.md), not through a public issue.
- Let `rustup` select the toolchain pinned in `rust-toolchain.toml`.
- Install `just`, `cargo-nextest`, and `cargo-deny`. Changes to Cargo features
  also need `cargo-hack`.

## Check your change

Run the same baseline gate used by CI:

```sh
just check
```

That checks formatting, Clippy, dependency policy, tests, doctests, and API
documentation. If you change features or optional dependencies, also run:

```sh
just feature-matrix
```

Add or update tests when behavior changes. Update the README or API docs when
users will see the difference. Don't manually change the package version;
releases handle that separately.

## Open the pull request

Pick the template that matches the change:

| Change | Template |
|--------|----------|
| General maintenance | [Default](.github/PULL_REQUEST_TEMPLATE.md) |
| Bug fix | [Bugfix](.github/PULL_REQUEST_TEMPLATE/bugfix.md) |
| Documentation only | [Documentation](.github/PULL_REQUEST_TEMPLATE/docs.md) |
| New behavior | [Feature](.github/PULL_REQUEST_TEMPLATE/feature.md) |

If GitHub loads the default template, replace it with the specific one before
filling it out.

The PR title must use Conventional Commit format because CI validates it:

```text
type(scope): concise summary
```

Examples: `fix(cache): preserve stale entries` and
`docs(http): clarify redirect behavior`. Complete the template with the checks
you actually ran and explain anything you skipped.
