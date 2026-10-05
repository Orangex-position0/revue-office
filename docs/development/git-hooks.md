# Git hooks

The repository uses one root `prek.toml` for the React/TypeScript frontend and
Tauri Rust backend. Do not register Lefthook alongside prek.

## Setup

Install frontend dependencies with `pnpm install --frozen-lockfile`. The hooks
also require Node.js, pnpm, Rust with rustfmt and Clippy, prek, Gitleaks,
cargo-todo and cargo-nextest on PATH.

Install missing tools yourself:

```sh
cargo install prek --locked
cargo install cargo-todo --locked
cargo install cargo-nextest --locked
rustup component add rustfmt clippy
```

Install Gitleaks using its [official installation instructions](https://github.com/gitleaks/gitleaks#installing).
Prettier is a project development dependency, installed by `pnpm install`.
Formatting rules live in `.prettierrc.json`; `.prettierignore` excludes the Rust
backend (formatted with rustfmt), generated files and dependency lockfiles.
Use `pnpm format:check` to check formatting or `pnpm format` to format eligible
files throughout the repository. Review the diff before committing.

For an existing Lefthook checkout, migrate before registering prek:

```sh
lefthook uninstall
prek validate-config prek.toml
prek install
```

A fresh clone only needs the last two commands. `prek install` registers
pre-commit, pre-push and commit-msg using the config's defaults.

## Checks

| Stage      | Frontend                     | Backend / shared                                                           |
| ---------- | ---------------------------- | -------------------------------------------------------------------------- |
| pre-commit | Prettier, ESLint, TypeScript | Rust formatting, locked compilation check, TODO review, staged secret scan |
| pre-push   | Production build             | Clippy with warnings denied, locked nextest tests                          |
| commit-msg | Shared header validator      | Shared header validator                                                    |

Pre-commit checks select files from the change; ESLint and TypeScript run against
the whole frontend once selected. Pre-push always checks both sides. No frontend
test hook is configured because this repository has no frontend test suite or
Vitest dependency. Add a test hook when a suite exists rather than fetching
Vitest implicitly during a push.

`cargo todo` reports TODOs for review; it does not enforce a zero-TODO policy.
The secret scanner inspects the staged index, even when prek is run with
`--all-files`; that command is not a full-history security scan.

Run checks manually:

```sh
prek run --all-files --stage pre-commit
prek run --all-files --stage pre-push
node --test scripts/check-commit-message.test.mjs
```

Commit headers use `type(scope)!: description`. Scope and `!` are optional;
scopes use kebab-case. Supported types are feat, fix, docs, style, refactor,
perf, test, build, ci, chore and revert. Use an English imperative description.
The dependency-free validator checks header structure, not prose quality or
breaking-change footers. Git-generated merge and revert headers are accepted.

Hooks may fail on pre-existing formatting or Clippy findings; fix those rather
than weakening the checks. Hooks can be bypassed with `--no-verify`, so they are
not a security boundary. [CI](ci-and-releases.md) enforces the quality gates before public merges when
configured as required checks in GitHub branch protection.
