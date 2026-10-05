# revue-office

**English** | [简体中文](README.zh-CN.md)

An AI-assisted desktop workspace for creating documents, spreadsheets, and presentations through conversation.

Built for people who want to turn an idea or source material into an editable office artifact: describe the task, follow the agent's progress, preview the result, and export it.

> **Early development.** Project metadata is currently at `0.1.0`; this is not a claim that a release has been published. The interface is primarily in Simplified Chinese. CI and release packaging target Windows; macOS and Linux support is not verified.

## Features

- **Conversational creation:** generate documents, Markdown, spreadsheets, and slide decks using an OpenAI-compatible chat model.
- **Preview and export:** inspect generated artifacts and export documents as DOCX, spreadsheets as XLSX, and presentations as PPTX.
- **Source attachments:** provide text, images, or office files as context for a task.
- **Persistent workspace:** revisit conversations, generated artifacts, and presentation projects stored locally.
- **Model settings:** configure provider endpoints, model lists, and API keys, with native credential-store integration in desktop mode.
- **Additional tools:** create charts and diagrams, search the web, and use image/video generation with the corresponding services configured.

The desktop shell is Tauri 2, the UI is React + TypeScript, and the Rust backend runs an embedded Axum HTTP/SSE service. SQLite is the default database; model requests still use your configured provider. This is not an offline-only application.

## Quickstart

### Prerequisites

- Node.js, pnpm, and Rust. The versions pinned in [CI](.github/workflows/ci.yml) are Node.js **24.14.0**, pnpm **11.8.0**, and Rust **1.95.0**; these are the reference versions, not a tested minimum-version matrix.
- [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/). On Windows, install Microsoft C++ Build Tools with **Desktop development with C++**, and the Microsoft Edge WebView2 runtime.
- An accessible native credential store. Desktop startup checks that it can write and read credentials; it does not fall back to plaintext storage if that check fails.
- A reachable OpenAI-compatible chat endpoint, a model that supports tool calls, and any credentials required by your provider.

### 1. Get the source

```sh
git clone https://github.com/Orangex-position0/revue-office.git
cd revue-office
pnpm install --frozen-lockfile
```

### 2. Configure the application

Copy [`.env.example`](.env.example) to `.env` in the repository root. Do not overwrite an existing local configuration.

```sh
# Git Bash / POSIX shell
cp .env.example .env
```

In PowerShell, use `Copy-Item .env.example .env` instead.

Edit the text-model settings to match your provider:

```dotenv
LLM_TEXT_BASE_URL=https://your-provider.example/v1
LLM_TEXT_API_KEY=your-api-key
LLM_TEXT_MODELS=your-tool-capable-model
```

These values are placeholders, not a working public service. The example's `http://127.0.0.1:8777/v1` endpoint also requires a separately running provider; revue-office does not start it.

For the first run:

- Keep the default local desktop profile and `AIPPT_HOST=127.0.0.1`. No account registration is required with the default local-guest identity policy.
- Comment out `AIPPT_CORS_ORIGINS` in your copied `.env` to use the desktop profile's built-in Vite and Tauri origins. The example's explicit override does not include all Tauri origins.
- Keep the `LLM_IMAGE_*` and `LLM_VIDEO_*` endpoint/model entries: startup currently requires these configuration groups even for text-only use. Configure valid credentials before using their remote generation tools.
- Leave `DATABASE_URL` unset to use SQLite. Keep the example's `../data` and `../outputs` paths when running through Tauri, which starts the Rust process from `src-tauri/`.
- For MySQL, configure server TLS and use `ssl-mode=verify_identity` in `DATABASE_URL`, as shown in `.env.example`. The server certificate must be trusted by the system (or supplied through `ssl-ca`) and match the connection hostname. Non-TLS RSA password authentication is intentionally disabled.
- Never commit `.env`. Environment-provided keys are imported into the native credential store in desktop mode, but the source `.env` file still contains those keys.

### 3. Launch the desktop app

```sh
pnpm tauri dev
```

This starts both the Vite frontend and the Rust desktop application. The default frontend URL is `http://localhost:1420`, and the embedded API binds to `127.0.0.1:8000`.

`pnpm dev` alone starts only the frontend, not the backend. Provider settings can also be managed from the application's **Settings (设置)** view.

**Verification scope:** the launch command was checked against the installed Tauri CLI and repository configuration. A fresh-clone desktop launch and live model generation have not been validated as part of this README update; they depend on system prerequisites, credential-store access, and your provider.

## First task

Start a conversation with a small request, for example:

> Create a one-page project brief with a summary, goals, risks, and next steps.

Follow the streamed progress, inspect the generated document in the artifact panel, and export it as DOCX. For a spreadsheet or slide deck, ask for that artifact explicitly and use its corresponding export control.

Generation quality and tool availability depend on the selected model and external services. Review generated content before using it; do not assume that calculations, citations, or slide layouts are correct.

## Development

Run commands from the repository root:

| Command                                                           | Purpose                                     |
| ----------------------------------------------------------------- | ------------------------------------------- |
| `pnpm typecheck`                                                  | Check frontend TypeScript                   |
| `pnpm lint`                                                       | Run ESLint                                  |
| `pnpm format:check`                                               | Check Prettier formatting                   |
| `pnpm build`                                                      | Build the frontend, not a desktop installer |
| `cargo nextest run --manifest-path src-tauri/Cargo.toml --locked` | Run Rust tests; requires cargo-nextest      |

The repository uses **prek** for Git hooks. Follow the [hook setup guide](docs/development/git-hooks.md) to install the required tools and register hooks. Pre-push checks run the frontend build, strict Rust Clippy, and Rust tests. There is no frontend application test suite yet.

Windows packaging is configured through a tag-triggered workflow that creates **unsigned installers in a draft release**, after CI succeeds. See [CI and releases](docs/development/ci-and-releases.md) for version checks, quality gates, and the manual publication process.

## Documentation

- [Backend architecture](docs/architecture/backend-architecture.md) — current modules, dependency boundaries, storage, and provider integration.
- [Architecture decision](docs/adr/ADR-001-adopt-agent-first-modular-architecture.md) — rationale for the agent-first modular architecture.
- [Git hooks](docs/development/git-hooks.md) — local checks and contribution tooling.
- [CI and releases](docs/development/ci-and-releases.md) — validation and Windows packaging.

The architecture documents are currently in Simplified Chinese; the development guides are in English.

## Contributing

Bug reports and focused pull requests are welcome. For substantial changes, open an issue first to discuss the scope.

Include reproduction steps, platform/tool versions, and redacted logs in bug reports. Before submitting a pull request, follow the hook setup guide, run the relevant checks, and describe how you verified the change. Use English Conventional Commit headers as documented in that guide.

## Security and privacy

Treat this as a local development application, not a hardened public service. Keep the embedded API on loopback; do not expose it to a network using the example configuration.

Prompts and attachment content may be sent to configured model or search services. Conversations and generated files are stored locally; API keys supplied through Settings use the native credential store in desktop mode. Keep secrets and sensitive documents out of issues, logs, screenshots, and pull requests.

There is no dedicated `SECURITY.md` yet. For vulnerabilities, use GitHub private vulnerability reporting **if enabled** on the repository's [Security page](https://github.com/Orangex-position0/revue-office/security). Otherwise, request a private reporting contact without posting exploit details or sensitive data publicly.

## License

Licensed under the [Apache License, Version 2.0](LICENSE). Third-party dependencies remain subject to their respective licenses.
