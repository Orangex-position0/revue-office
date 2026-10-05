# CI and releases

## Continuous integration

`.github/workflows/ci.yml` runs on pull requests, pushes to `main` or `master`,
manual dispatch, and calls from the release workflow.

- **Frontend:** frozen pnpm install, Prettier, ESLint, TypeScript, commit-validator
  tests, production build and production dependency audit (high severity or above).
- **Rust:** rustfmt, strict Clippy and nextest on Windows.
- **Rust dependency audit:** RustSec checks against `src-tauri/Cargo.lock`.
- **Secret scan:** Gitleaks scans checked-out Git history with redacted output.

Node.js, pnpm and Rust versions are specified in the workflows. Keep both workflow
files aligned when upgrading them. Rust compilation uses the committed lockfile.
The frontend has no application test suite yet; commit-validator tests are not a
replacement for UI tests.

Enable branch protection or a ruleset in GitHub and require the CI jobs before
merging. Local hooks and workflow files alone do not enforce branch protection.
Pre-existing formatting, Clippy or dependency findings must be addressed before
these gates can pass. No advisories are silently ignored.

### Database dependency security

SQLx 0.9 makes MySQL's non-TLS RSA password authentication optional. This project
keeps `mysql-rsa` disabled, removing the vulnerable `rsa` dependency associated
with `RUSTSEC-2023-0071`, and enables rustls with native certificate roots.
MySQL users should configure server TLS and set `ssl-mode=verify_identity` in
`DATABASE_URL`; the certificate must match the hostname and be trusted by the
system, or supplied with `ssl-ca`. Connections needing RSA authentication without
TLS now fail instead of enabling the vulnerable dependency. SQLite is unchanged.

`cargo audit --file src-tauri/Cargo.lock` remains the regression gate. Unmaintained
or unsound transitive dependency warnings are still reported; an audit exit code
of zero is not a claim that every dependency is maintained or risk-free.

## Windows releases

`.github/workflows/release.yml` runs when a `v*` tag is pushed. It first runs the
entire CI workflow on the tagged commit. Only after all checks pass does it build
Windows x64 NSIS (`.exe`) and MSI installers and upload them to a **draft** GitHub
Release. It also retains workflow build artifacts.

Before tagging:

1. Set the same version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json`; refresh lockfile metadata if necessary.
2. Update `CHANGELOG.md`, review the changes and ensure CI passes.
3. Commit and push the release commit, then create and push its matching tag
   (for example, `v0.1.0`). A tag/version mismatch fails the release build.
4. Review the generated notes and installers in the draft, test installation,
   and publish it manually. Tags containing `-` create prerelease drafts.

The workflow uses GitHub's built-in `GITHUB_TOKEN`; no personal access token is
required. Only the packaging job receives `contents: write`. No code signing,
macOS/Linux packages, updater metadata or automatic publication is configured.
Windows installers are unsigned and may trigger SmartScreen warnings.

Do not include local `.env`, database files, user data or credentials in release
resources. `LICENSE` is included by the existing Tauri bundle configuration.
Adding workflows does not run them locally or publish a release; GitHub Actions
must be enabled on the hosting repository.
