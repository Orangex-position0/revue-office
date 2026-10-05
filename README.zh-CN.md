# revue-office

[English](README.md) | **简体中文**

通过对话创建文档、表格和演示文稿的 AI 辅助桌面办公应用。

适合希望将想法或参考材料整理为可编辑办公产物的用户：描述任务，查看 Agent 的执行进度，预览结果，再导出文件。

> **处于早期开发阶段。** 项目配置中的版本号为 `0.1.0`，不代表该版本已经发布。界面主要使用简体中文。CI 和发布打包面向 Windows，macOS 和 Linux 支持尚未验证。

## 功能

- **对话式创作：** 使用兼容 OpenAI API 的聊天模型生成文档、Markdown、表格和演示文稿。
- **预览与导出：** 查看生成的产物，将文档导出为 DOCX、表格导出为 XLSX、演示文稿导出为 PPTX。
- **参考附件：** 上传文本、图片或 Office 文件，为任务提供上下文。
- **持久化工作区：** 重新查看本地保存的对话、生成产物和演示文稿项目。
- **模型设置：** 配置服务端点、模型列表和 API Key；桌面模式集成系统原生凭据存储。
- **其他工具：** 创建图表和流程图、搜索网页，以及在配置相应服务后生成图片和视频。

桌面容器使用 Tauri 2，界面使用 React + TypeScript，Rust 后端在应用进程内运行 Axum HTTP/SSE 服务。默认数据库为 SQLite；模型请求仍会发送到配置的服务商，因此本项目不是纯离线应用。

## 快速开始

### 环境要求

- Node.js、pnpm 和 Rust。[CI](.github/workflows/ci.yml) 固定使用 Node.js **24.14.0**、pnpm **11.8.0** 和 Rust **1.95.0**。这些是参考版本，不代表已验证的最低版本要求。
- [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)。Windows 需要安装 Microsoft C++ Build Tools，并选择 **Desktop development with C++（使用 C++ 的桌面开发）**，以及 Microsoft Edge WebView2 运行时。
- 可访问的系统原生凭据存储。桌面应用启动时会验证凭据读写能力；验证失败时不会回退到明文存储。
- 可访问的 OpenAI-compatible 聊天端点、支持工具调用的模型，以及服务商要求的凭据。

### 1. 获取源码

```sh
git clone https://github.com/Orangex-position0/revue-office.git
cd revue-office
pnpm install --frozen-lockfile
```

### 2. 配置应用

将 [`.env.example`](.env.example) 复制为仓库根目录下的 `.env`。不要覆盖已有的本地配置。

```sh
# Git Bash / POSIX shell
cp .env.example .env
```

使用 PowerShell 时，改为执行 `Copy-Item .env.example .env`。

根据服务商提供的信息修改文本模型配置：

```dotenv
LLM_TEXT_BASE_URL=https://your-provider.example/v1
LLM_TEXT_API_KEY=your-api-key
LLM_TEXT_MODELS=your-tool-capable-model
```

以上值均为占位示例，不是可直接使用的公共服务。配置示例中的 `http://127.0.0.1:8777/v1` 同样需要单独运行模型服务，revue-office 不会启动该服务。

首次运行时：

- 保持默认的本地桌面模式和 `AIPPT_HOST=127.0.0.1`。默认的 local-guest 身份策略不要求注册账号。
- 注释掉复制后的 `.env` 中的 `AIPPT_CORS_ORIGINS`，使用桌面模式内置的 Vite 和 Tauri 来源配置。示例中的显式覆盖值没有包含所有 Tauri 来源。
- 保留 `LLM_IMAGE_*` 和 `LLM_VIDEO_*` 中的端点及模型配置：即使只使用文本功能，当前启动流程也要求这些配置组存在。使用相应远程生成工具前，需要配置有效凭据。
- 不设置 `DATABASE_URL` 即可使用 SQLite。通过 Tauri 启动时，Rust 进程的工作目录为 `src-tauri/`，请保留示例中的 `../data` 和 `../outputs` 路径。
- 使用 MySQL 时，请配置服务端 TLS，并按照 `.env.example` 在 `DATABASE_URL` 中设置 `ssl-mode=verify_identity`。服务端证书必须受到系统信任（也可以通过 `ssl-ca` 指定），且与连接主机名匹配。本项目明确禁用了非 TLS 的 RSA 密码认证。
- 不要提交 `.env`。桌面模式会将环境变量提供的密钥导入系统原生凭据存储，但原始 `.env` 文件仍然包含这些密钥。

### 3. 启动桌面应用

```sh
pnpm tauri dev
```

该命令同时启动 Vite 前端和 Rust 桌面应用。默认前端地址为 `http://localhost:1420`，内嵌 API 监听 `127.0.0.1:8000`。

单独执行 `pnpm dev` 只会启动前端，不会启动后端。也可以在应用的 **Settings（设置）** 页面管理模型服务配置。

**验证范围：** 启动命令已根据本地安装的 Tauri CLI 和仓库配置核对。本次 README 更新未验证从全新克隆启动桌面应用或实际调用模型生成内容；这些操作依赖系统环境、凭据存储访问权限和模型服务。

## 第一个任务

新建对话，尝试一个小任务，例如：

> 创建一页项目简报，包含摘要、目标、风险和下一步计划。

查看实时执行进度，在产物面板中预览生成的文档，再导出为 DOCX。如果需要表格或演示文稿，请在请求中明确指定，并使用对应的导出控件。

生成质量和工具可用性取决于所选模型及外部服务。使用前请检查结果，不要默认计算、引用来源或幻灯片布局一定正确。

## 开发

在仓库根目录执行以下命令：

| 命令                                                              | 用途                               |
| ----------------------------------------------------------------- | ---------------------------------- |
| `pnpm typecheck`                                                  | 检查前端 TypeScript                |
| `pnpm lint`                                                       | 运行 ESLint                        |
| `pnpm format:check`                                               | 检查 Prettier 格式                 |
| `pnpm build`                                                      | 构建前端，不生成桌面安装包         |
| `cargo nextest run --manifest-path src-tauri/Cargo.toml --locked` | 运行 Rust 测试，需要 cargo-nextest |

仓库使用 **prek** 管理 Git hooks。请按照 [hook 配置指南](docs/development/git-hooks.md) 安装工具并注册 hooks。Pre-push 检查包括前端构建、严格 Rust Clippy 检查和 Rust 测试。目前没有前端应用测试套件。

Windows 打包通过 tag 触发工作流，在 CI 通过后生成**未签名安装包并上传到草稿 Release**。版本检查、质量门禁和手动发布流程详见 [CI 与发布](docs/development/ci-and-releases.md)。

## 文档

- [后端架构](docs/architecture/backend-architecture.md) — 当前模块、依赖边界、存储和模型服务集成。
- [架构决策](docs/adr/ADR-001-adopt-agent-first-modular-architecture.md) — 采用 Agent-first 模块化架构的原因。
- [Git hooks](docs/development/git-hooks.md) — 本地检查与贡献工具配置。
- [CI 与发布](docs/development/ci-and-releases.md) — 验证流程与 Windows 打包。

架构文档目前使用简体中文，开发指南使用英文。

## 参与贡献

欢迎提交 Bug 报告和范围明确的 Pull Request。较大改动请先通过 Issue 讨论范围。

Bug 报告请包含复现步骤、平台与工具版本，以及脱敏后的日志。提交 Pull Request 前，请按照 hook 配置指南准备环境，运行相关检查，并说明改动的验证方式。Commit 标题使用指南中规定的英文 Conventional Commit 格式。

## 安全与隐私

请将本项目视为本地开发应用，而不是经过安全加固的公共服务。保持内嵌 API 仅监听回环地址，不要使用示例配置将其暴露到网络。

提示词和附件内容可能会发送给配置的模型或搜索服务。对话和生成文件保存在本地；桌面模式下，通过设置页面提供的 API Key 使用系统原生凭据存储。不要在 Issue、日志、截图或 Pull Request 中包含密钥或敏感文档。

目前没有单独的 `SECURITY.md`。报告漏洞时，**如果仓库已启用** GitHub 私密漏洞报告，请使用仓库 [Security 页面](https://github.com/Orangex-position0/revue-office/security) 中的私密报告入口。否则，请先请求私下报告的联系方式，不要公开利用细节或敏感数据。

## 许可证

本项目采用 [Apache License, Version 2.0](LICENSE)。第三方依赖仍受各自许可证约束。
