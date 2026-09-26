# AI Office 类项目架构与模块组织参考

> 面向 `revue-office` 重构的调研文档
>
> 调研范围：GitHub 上与 AI Office、AI 文档工作区、AI 演示文稿生成和本地桌面 Agent 相关的开源项目。
>
> Star 数为 GitHub API 在 2026-09-26 的调研快照，项目会持续变化，不能视为长期排名。

## 1. 结论摘要

`revue-office` 当前已经具备一个清晰的产品雏形：Tauri 桌面壳、React 前端、Rust 本地服务、SQLite 持久化、LLM 流式调用、Agent Loop、文件处理，以及 PPT、Word、Excel、Draw.io、ECharts 等工具能力。

当前主要问题不是能力不足，而是**应用层、Agent 层、文档能力层和基础设施层交叉在一起**。重构时不建议简单地把目录改成 `components/`、`services/`、`utils/` 三类，而应围绕四个稳定边界组织：

1. **产品工作区**：项目、会话、文件、预览、导出、设置。
2. **Agent 运行时**：上下文、意图识别、模型客户端、工具注册、循环控制、事件流。
3. **文档与产物能力**：PPT、Word、Excel、图表、流程图、渲染和文件提取。
4. **平台基础设施**：认证、数据库、配置、路由、错误、日志、Tauri 集成。

最值得借鉴的组合不是某一个项目的完整复制，而是：

- **GenOffice** 的“应用按文档类型拆分，能力下沉到共享 packages”。
- **Deckium** 的“Agent 工具操作领域状态，编辑器和导出共用同一份模型”。
- **OfficeCLI** 的“文档引擎独立于 UI，并提供 CLI、结构化输出和渲染反馈”。
- **AionUi** 的“Agent、Assistant、Skill、工具和桌面/Web 入口分层”。
- **Presenton** 的“生成、模板、编辑、导出、API 与桌面运行时分离”。

推荐 `revue-office` 最终采用**模块化单体 + 清晰领域包**，而不是立即拆成微服务。桌面应用的关键是边界稳定、数据模型统一、工具可测试，而不是进程数量多。

## 2. 本项目现状与重构约束

### 2.1 当前定位

从仓库结构和 `.spec/` 文档看，本项目定位接近：

- 面向知识工作者的 AI Office 工作区。
- 通过自然语言驱动文档、演示文稿、表格、图表和流程图等工具。
- 以 Tauri 提供跨平台桌面入口。
- React 负责工作区、聊天、文件、预览和编辑交互。
- Rust 负责 Agent、LLM、路由、数据库、文件处理、渲染和本地能力。

### 2.2 当前结构

```text
src/
├── api/
├── components/
│   ├── artifacts/
│   ├── chat/
│   ├── history/
│   ├── layout/
│   ├── preview/
│   ├── settings/
│   ├── slides/
│   └── toolbar/
├── config/
├── lib/
├── pages/
├── stores/
├── styles/
└── types/

src-tauri/src/
├── agent/
│   ├── agent_loop.rs
│   ├── context.rs
│   ├── intent.rs
│   ├── registry.rs
│   ├── tool.rs
│   └── tools.rs
├── db/
├── llm/
├── render/
├── routes/
├── auth.rs
├── commands.rs
├── config.rs
├── files.rs
├── models.rs
└── state.rs
```

这个结构已经比单一的 `main.rs` 或单一的 `App.tsx` 健康，但仍然存在两个潜在风险：

- Rust 侧以技术类型组织的模块，容易让一个业务流程跨越 `routes`、`agent`、`db`、`files`、`render` 多处实现。
- 前端的 `components` 与 `stores` 是横向目录，随着 Studio、聊天、文件、产物、预览继续增长，领域边界会变得模糊。

### 2.3 重构约束

- 保留 Tauri + React + Rust，不因为参考项目使用 Electron 或 Node 就整体换栈。
- 保留本地优先能力，避免将文件、会话和产物强行绑定云端服务。
- Agent 工具必须能够独立测试，不依赖 React 组件和具体页面。
- PPT、Word、Excel、图表和流程图都应视为可插拔的能力模块。
- 预览结果、工具结果和导出结果需要有统一的产物模型。
- 路由层不应直接承载业务规则。

## 3. 按 Star 排名的 5 个参考项目

### 排名说明

以下项目按调研时 GitHub API 返回的 Star 数排序。选择标准不是单纯追求 Star，而是同时满足以下至少一项：

- 面向 AI Office 或文档生产。
- 具备 PPT、Word、Excel 或 PDF 等真实文件工作流。
- 具备可参考的 Agent、工具调用、预览或桌面架构。

| 排名 | 项目 | Stars 快照 | 主要定位 | 与 revue-office 的参考价值 |
|---:|---|---:|---|---|
| 1 | [iOfficeAI/AionUi](https://github.com/iOfficeAI/AionUi) | 33,127 | 多模型、多 Agent 的桌面 Cowork 工作区，包含 PPT、Word、Excel 助手 | Agent 平台、Assistant/Skill、桌面与 Web 入口 |
| 2 | [iOfficeAI/OfficeCLI](https://github.com/iOfficeAI/OfficeCLI) | 31,237 | 面向 Agent 的 Word、Excel、PowerPoint 文件操作和渲染引擎 | 文档能力下沉、CLI/API、结构化操作、渲染反馈 |
| 3 | [presenton/presenton](https://github.com/presenton/presenton) | 10,769 | 本地/自托管 AI 演示文稿生成、编辑、导出和 API | 生成流水线、模板、导出、桌面和服务端边界 |
| 4 | [genspark-ai/genoffice](https://github.com/genspark-ai/genoffice) | 7,725 | 原生 AI Office，覆盖 Docs、Sheets、Slides、PDF、Markdown、HTML | 多应用 monorepo、共享引擎、按文档类型拆分 |
| 5 | [sleipner42/Deckium](https://github.com/sleipner42/Deckium) | 4 | 本地优先、可人工干预的 AI 演示文稿编辑器 | 工具化 Agent Loop、领域状态、校验和可逆编辑 |

> 说明：Star 数来自调研时的 GitHub API 快照，排序只用于筛选参考对象，不代表项目质量或架构优劣。

## 4. 项目一：AionUi

- 仓库：[github.com/iOfficeAI/AionUi](https://github.com/iOfficeAI/AionUi)
- Stars 快照：33,127
- 定位：跨平台 Cowork 应用，内置 Agent，也支持多个外部 Agent。

### 4.1 架构设计

AionUi 把产品看成一个 Agent 工作平台，而不是单一聊天窗口：

```text
桌面应用 / WebUI / 远程渠道
              │
              ▼
       会话与工作区层
              │
      ┌───────┴────────┐
      ▼                ▼
 内置 Agent        外部 Agent/CLI
      │                │
      └───────┬────────┘
              ▼
       Skills / Tools / MCP
              │
              ▼
       文件、Office、搜索、自动化
```

仓库中可以看到以下组织倾向：

- 应用入口与桌面/Web 交付相关代码独立管理。
- 内置助手通过 assistants catalog 管理。
- Office 能力通过 `pptx`、`docx`、`xlsx` skills 组织。
- Agent、远程接入、设置、工作区和 PRD 文档分别形成长期演进边界。
- AionUi 将 OfficeCLI 作为底层 Office 文件能力，而不是把每一种文件操作都塞到界面层。

### 4.2 模块组织方式

```text
应用壳
├── desktop / web
├── workspace
├── conversation
├── settings
└── remote channels

Agent 平台
├── built-in agent
├── external agent adapters
├── assistants catalog
├── skills
├── MCP/tools
└── scheduling/automation

业务能力
├── PPT assistant
├── Word assistant
├── Excel assistant
├── file operations
└── web/search/image tools
```

### 4.3 对 revue-office 的启发

适合借鉴：

- 将“助手配置”从 Agent Loop 中抽离，形成 Assistant/Skill/Tool 三层。
- 工具不只是一组 Rust 函数，还应有名称、输入 Schema、权限、超时、结果类型和可见性。
- 远期可以让聊天工作区、命令行和远程入口共用同一个 Agent Runtime。

不应直接照搬：

- AionUi 面向多 Agent、远程控制和自动化，复杂度远高于当前项目。
- 当前阶段不需要先引入完整的外部 CLI Agent 生态。

## 5. 项目二：OfficeCLI

- 仓库：[github.com/iOfficeAI/OfficeCLI](https://github.com/iOfficeAI/OfficeCLI)
- Stars 快照：31,237
- 定位：为 AI Agent 设计的 Word、Excel、PowerPoint 文件操作和渲染工具。

### 5.1 架构设计

OfficeCLI 的核心思想是：**UI 不应该拥有 Office 文件的业务语义，Agent 也不应该直接拼接底层 XML。**

```text
Agent / 人类 / AionUi
          │
          ▼
   CLI / JSON / SDK 接口
          │
          ▼
  文档对象操作层
  ├── Word
  ├── Excel
  └── PowerPoint
          │
          ├── 读写与结构化查询
          ├── 增删改与路径寻址
          ├── 模板与技能
          └── HTML/PNG 渲染
```

项目特别强调：

- 单二进制、跨平台、无 Office 依赖。
- 用结构化命令读写文档。
- 使用路径寻址定位文档元素。
- 提供 `view`、HTML、PNG 和实时预览，帮助 Agent 形成“生成、查看、修复”的反馈回路。
- 将 Office 技能放在独立的 `skills/` 目录中。
- 同时提供 CLI、Node SDK、Python SDK 等调用入口。

### 5.2 模块组织方式

```text
src/officecli/
├── Word domain
├── Excel domain
├── PowerPoint domain
├── command/query layer
├── render/view layer
└── shared document primitives

skills/
├── officecli
├── officecli-docx
├── officecli-xlsx
├── officecli-pptx
└── domain assistants

sdk/
├── node
└── python
```

### 5.3 对 revue-office 的启发

这是对本项目最直接的参考：

1. 把 `pptx`、`docx`、`xlsx` 能力从 `agent/tools` 中下沉到独立领域服务。
2. Tool 只负责把 Agent 请求转成领域命令，不负责文档细节。
3. 每个文档领域提供统一的 `inspect / mutate / render / export` 接口。
4. 工具返回结构化结果，而不是只返回字符串。
5. 预览和校验应该是 Agent Loop 的一等反馈，不是任务结束后的附加功能。

## 6. 项目三：Presenton

- 仓库：[github.com/presenton/presenton](https://github.com/presenton/presenton)
- Stars 快照：10,769
- 定位：本地或自托管的 AI 演示文稿生成器，支持模板、编辑、PPTX/PDF 导出和 API。

### 6.1 架构设计

Presenton 采用相对清晰的产品流水线：

```text
用户输入 / 上传文件 / API
             │
             ▼
       内容与上下文准备
             │
             ▼
       AI 生成与布局规划
             │
             ▼
       模板 / 主题 / Slide Model
             │
             ▼
       浏览器编辑器
             │
       ┌─────┴─────┐
       ▼           ▼
   PPTX 导出    PDF 导出
```

仓库结构体现了几条重要边界：

- `servers/fastapi` 负责服务端 API、认证、数据库、异步任务和生成流程。
- `electron` 负责桌面主进程、IPC、运行时资源和本地服务编排。
- 前端资源、模板转换和导出脚本保持相对独立。
- API 以 presentation、file、template 等资源对外暴露。
- Docker、自托管和桌面应用共享一部分服务端能力。

### 6.2 模块组织方式

```text
servers/fastapi/
├── API routes
├── auth
├── database/migrations
├── presentation generation
├── file upload/extraction
├── templates
└── async tasks

electron/
├── main process
├── IPC handlers
├── local runtime/server lifecycle
├── export helpers
└── packaged resources

frontend/shared assets
├── editor
├── templates
└── export/runtime resources
```

### 6.3 对 revue-office 的启发

- 将“生成任务”建模成有状态的 Job，而不是一次 HTTP 请求。
- 文件上传、内容抽取、生成、编辑和导出是不同阶段，应该拥有不同的数据结构和错误边界。
- 模板是领域资产，应独立于聊天页面和工具实现。
- 桌面壳只负责启动、IPC、资源和生命周期，业务逻辑放在可测试的应用服务中。

## 7. 项目四：GenOffice

- 仓库：[github.com/genspark-ai/genoffice](https://github.com/genspark-ai/genoffice)
- Stars 快照：7,725
- 定位：全功能 AI Office，覆盖 Docs、Sheets、Slides、PDF、Markdown 和 HTML。

### 7.1 架构设计

GenOffice 的最强参考点是**按应用拆分入口，按能力抽取共享包**：

```text
apps/
├── docs
├── sheets
├── slides
├── pdf
├── markdown
├── html
└── shell

packages/
├── agent-core
├── ai-provider
├── ai-search
├── docx-engine
├── pptx-engine
├── pptx-render
├── xlsx-gateway
├── file-parse
├── project-store
├── pipelines
├── ui
└── cli
```

每个文档应用有自己的 `main / preload / renderer / shared`，而跨文档的 Agent、AI Provider、解析、存储、渲染和 UI 则放在 packages 中。

### 7.2 模块组织方式

```text
应用层
├── Docs app
├── Sheets app
├── Slides app
├── PDF app
└── Shell app

共享领域层
├── docx engine
├── pptx engine
├── xlsx gateway
├── file parse
├── project store
└── rendering/conversion

Agent 基础层
├── agent-core
├── ai-provider
├── ai-search
├── pipelines
└── cli/skills
```

GenOffice 还强调：

- AI 面板嵌入每一个文档应用，而不是只存在于统一聊天页。
- 文档编辑需要真实格式、版本快照、Diff、回滚和局部修改。
- 文件搜索、解析、渲染和 Agent 操作应该共用文档模型。

### 7.3 对 revue-office 的启发

这是最适合作为目标结构参考的项目：

- 不把 PPT、Word、Excel 当成散落的工具函数，而是分别形成领域模块。
- 把共用的 Agent、LLM Provider、文件解析、项目存储和 UI 组件抽到 shared/core。
- `Studio` 可以保留为统一壳，但内部应按 `document type` 和 `workflow` 分派。
- 如果未来出现独立的 Docs、Sheets、Slides 页面，可以在不重写核心能力的情况下增加应用入口。

## 8. 项目五：Deckium

- 仓库：[github.com/sleipner42/Deckium](https://github.com/sleipner42/Deckium)
- Stars 快照：4
- 定位：本地优先、支持人工控制的 AI 演示文稿编辑器。

虽然 Star 数较低，但它的架构文档对本项目很有价值，因为它把 Agent Loop 和编辑器状态的关系说明得很清楚。

### 8.1 架构设计

```text
Renderer
├── presentation editor
├── selection/manual edits
└── chat panel

Main process
├── ai service
│   ├── provider adapters
│   ├── agent history
│   ├── tool factory
│   └── tool implementations
├── presentation state/operations
├── powerpoint import/export
├── pdf export
└── settings

Common
├── domain entities
├── AI types
├── linting types
└── shared configuration
```

它的 Agent Loop 是：

1. 向模型提供当前演示文稿、当前页和人工编辑 Diff。
2. 模型通过工具修改演示文稿。
3. 每次工具调用后返回 slide grid 和 lint 结果。
4. 模型根据渲染和校验反馈继续修正。
5. 会话历史和工具调用持久化，使多轮编辑保持连续。

### 8.2 模块组织方式

Deckium 明确区分：

- `main`：AI、文件、导入导出、设置和领域操作。
- `renderer`：编辑器界面。
- `common`：领域实体、配置和跨进程类型。
- `ai/tools`：每个工具一个文件，并通过 ToolFactory 注册。
- `presentation`：演示文稿状态和操作，不直接依赖 UI。

### 8.3 对 revue-office 的启发

- `ToolFactory`/Registry 可以映射到本项目的 Rust Tool Registry。
- 工具输入和输出应围绕领域命令设计，而非暴露数据库模型。
- Agent 每一步都应能收到结构化反馈：变更摘要、预览、校验错误、可回滚版本。
- “用户手工修改后的 Diff”是多轮 Agent 协作的重要上下文，不能只把完整文档重复塞进 Prompt。

## 9. 五个项目的横向比较

| 维度 | AionUi | OfficeCLI | Presenton | GenOffice | Deckium |
|---|---|---|---|---|---|
| 核心定位 | Agent 工作平台 | Office 文件引擎 | PPT 生成平台 | 多文档 AI Office | PPT Agent 编辑器 |
| 主要边界 | Assistant/Skill/Agent/渠道 | 文档域/CLI/渲染 | API/生成/模板/导出/桌面 | apps/packages | main/renderer/common |
| 文档模型 | 依赖底层能力和助手 | 结构化文档对象 | Presentation/Template | 各文档引擎 | Presentation domain |
| Agent 形态 | 内置 + 外部 Agent | 被 Agent 调用的工具 | 生成任务与 API | 内嵌 AI 面板 | Tool-based loop |
| 反馈机制 | 工具和产物 | view/HTML/PNG/watch | 编辑预览和导出 | Diff/快照/回滚/引用 | grid/lint/screenshot |
| 适合借鉴 | 平台化 | 能力下沉 | 流程拆分 | 目标 monorepo | Agent 编辑闭环 |

## 10. 对 revue-office 的推荐目标架构

### 10.1 推荐原则

1. **按领域组织，而不是按技术名词组织。**
2. **Agent 只依赖领域接口，不直接依赖数据库和 UI。**
3. **每个工具都生成结构化事件和结构化结果。**
4. **文档修改、预览、校验、导出共享同一份领域状态。**
5. **前端页面按工作流组织，公共 UI 只保留真正共享的部分。**
6. **先模块化单体，暂不拆分进程。**

### 10.2 推荐 Rust 目录

```text
src-tauri/src/
├── app/                         # 应用启动、生命周期、依赖组装
│   ├── mod.rs
│   ├── bootstrap.rs
│   └── state.rs
├── domain/                      # 纯领域模型和端口，不依赖 Axum/SQLx
│   ├── workspace/
│   ├── conversation/
│   ├── artifact/
│   ├── document/
│   ├── job/
│   └── user/
├── agent/                       # Agent Runtime
│   ├── runtime.rs
│   ├── context.rs
│   ├── intent.rs
│   ├── planner.rs
│   ├── tool_registry.rs
│   ├── tool_contract.rs
│   ├── event.rs
│   └── policies.rs
├── capabilities/                # 可被 Agent 和 UI 调用的领域能力
│   ├── office/
│   │   ├── docx.rs
│   │   ├── xlsx.rs
│   │   └── pptx.rs
│   ├── presentation/
│   ├── spreadsheet/
│   ├── document/
│   ├── diagram/
│   ├── chart/
│   ├── file/
│   ├── search/
│   └── render/
├── application/                 # 用例编排，不放底层细节
│   ├── chat_service.rs
│   ├── artifact_service.rs
│   ├── document_service.rs
│   ├── export_service.rs
│   ├── file_service.rs
│   └── settings_service.rs
├── infrastructure/              # 技术实现
│   ├── db/
│   ├── llm/
│   ├── filesystem/
│   ├── renderers/
│   ├── search/
│   └── auth/
├── transport/                   # Axum/Tauri 边界
│   ├── routes/
│   ├── commands.rs
│   ├── dto.rs
│   └── error.rs
└── main.rs / lib.rs
```

### 10.3 推荐前端目录

```text
src/
├── app/
│   ├── routes/
│   ├── providers/
│   └── bootstrap/
├── features/
│   ├── workspace/
│   ├── conversation/
│   ├── files/
│   ├── artifacts/
│   ├── studio/
│   ├── settings/
│   └── auth/
├── document-types/
│   ├── presentation/
│   ├── document/
│   ├── spreadsheet/
│   ├── diagram/
│   └── chart/
├── shared/
│   ├── ui/
│   ├── hooks/
│   ├── api/
│   ├── state/
│   ├── types/
│   └── utils/
└── styles/
```

每个 feature 内部建议保持：

```text
features/conversation/
├── components/
├── hooks/
├── api.ts
├── state.ts
├── types.ts
└── selectors.ts
```

不要把所有 Zustand store 集中到一个全局目录，也不要让页面直接修改另一个 feature 的内部状态。

## 11. 推荐的 Agent 与工具边界

### 11.1 Agent 层只负责

- 读取会话和工作区上下文。
- 选择模型和工具。
- 执行工具调用循环。
- 处理流式事件、重试、取消和超时。
- 持久化 Agent 运行记录。
- 向 UI 发布统一事件。

### 11.2 工具层负责

- 声明工具名、输入 Schema、权限和副作用。
- 将输入转换成领域命令。
- 调用领域服务。
- 返回结构化结果和可选预览。

建议的工具结果：

```json
{
  "ok": true,
  "operation": "presentation.add_slide",
  "artifact_id": "artifact_123",
  "changed": ["slide[3]"],
  "preview": {
    "kind": "slide_grid",
    "url": "..."
  },
  "validation": {
    "errors": [],
    "warnings": ["标题文本接近溢出"]
  },
  "undo_token": "undo_456"
}
```

### 11.3 能力层负责

例如 `presentation.add_slide` 应该能被以下入口复用：

- Agent Tool。
- React 编辑器按钮。
- Tauri Command。
- 自动化测试。
- 未来 CLI 或 MCP。

这要求能力层不能依赖聊天消息、React 状态或 Axum Request。

## 12. 推荐的数据模型边界

建议区分以下对象，不要用一个 `models.rs` 容纳所有模型：

```text
Workspace       用户可见的工作空间
Conversation    对话和消息历史
AgentRun        一次 Agent 执行及状态
ToolCall        一次工具调用及结构化结果
Artifact        AI 或用户产生的文件/文档产物
DocumentState   可编辑的领域状态
DocumentVersion 文档快照和回滚点
Preview         预览资源和渲染元数据
Job             长时间生成、导出或转换任务
```

数据库 Repository 只负责持久化，不能把 SQL 查询结果直接作为领域服务的公开 API。

## 13. 推荐的重构顺序

### 阶段一：先建立契约，不移动所有文件

- 定义 `Artifact`、`AgentRun`、`ToolCall`、`DocumentVersion`。
- 定义统一 Agent Event：`run_started`、`token_delta`、`tool_started`、`tool_finished`、`artifact_updated`、`preview_ready`、`run_failed`。
- 为工具定义输入/输出 Schema。
- 为错误建立稳定的错误码。

### 阶段二：抽离 Agent Runtime

- 将 `agent_loop.rs`、`context.rs`、`intent.rs`、`registry.rs` 重新整理为 Runtime、Context、Policy、Tool Registry。
- Tool Registry 不再直接保存所有业务细节。
- 工具通过 application service 调用 capabilities。

### 阶段三：抽离文档能力

按下面顺序迁移：

1. PPT：已有能力最多，先建立 `presentation` 领域。
2. 文件和产物：统一 preview/export/artifact。
3. Word 与 Excel：将生成和渲染接口统一起来。
4. Draw.io、ECharts：归入 diagram/chart capabilities。

### 阶段四：按 feature 重组前端

- `pages` 只保留路由入口。
- `components/chat` 迁移到 `features/conversation`。
- `components/artifacts`、`preview`、`slides` 迁移到 `features/artifacts` 和 `document-types`。
- `stores` 拆到各 feature 的 `state.ts`。
- API DTO 与领域 UI 类型分开。

### 阶段五：增加可验证闭环

每个文档能力至少提供：

```text
create → inspect → mutate → render → validate → export
```

Agent 每次修改后都能获得：

- 改动对象。
- 预览或截图。
- 校验结果。
- 版本/撤销标识。

## 14. 不建议采用的方案

### 14.1 不建议马上拆成微服务

本项目是桌面优先、本地运行、SQLite 存储。微服务会增加部署、进程生命周期和调试成本，但不会自动解决领域耦合。

### 14.2 不建议按文件格式复制三套 Agent

不要分别实现 `ppt_agent`、`docx_agent`、`xlsx_agent` 三套完全独立循环。应共享 Agent Runtime，把格式差异放在 capability、tool schema 和 document adapter 中。

### 14.3 不建议让路由直接调用 Repository

`route -> repository` 会让业务规则散落在 API 入口。建议保持：

```text
route/command -> application service -> domain/capability -> repository/adapter
```

### 14.4 不建议把所有公共代码都放进 utils

能表达业务含义的代码应进入具体领域。例如：

- `artifact_turns` 应属于 artifact/conversation workflow。
- `file_extract` 应属于 file capability。
- `pptx_render` 应属于 presentation/rendering。

### 14.5 不建议只保存最终文件

AI 编辑需要过程可追踪。至少保留 AgentRun、ToolCall、ArtifactVersion 和 Preview 元数据，否则出现格式损坏或 Agent 误操作时无法解释和回滚。

## 15. 推荐的最小目标版本

如果只做一次小规模重构，建议先达到下面结构：

```text
src-tauri/src/
├── domain/
│   ├── artifact.rs
│   ├── conversation.rs
│   └── document.rs
├── agent/
│   ├── runtime.rs
│   ├── context.rs
│   ├── tools.rs
│   └── events.rs
├── capabilities/
│   ├── presentation.rs
│   ├── office.rs
│   ├── file.rs
│   └── render.rs
├── application/
│   ├── chat.rs
│   ├── artifacts.rs
│   └── export.rs
├── infrastructure/
│   ├── db/
│   └── llm/
└── transport/
    ├── routes/
    └── commands.rs

src/
├── app/
├── features/
│   ├── conversation/
│   ├── studio/
│   ├── files/
│   └── artifacts/
├── document-types/
└── shared/
```

这个版本不会要求立刻把每个 Rust 文件拆成独立 crate，也不会改变现有产品流程，但能先建立正确的依赖方向。

## 16. 最终建议

建议将 `revue-office` 的架构目标定义为：

> 一个本地优先的模块化 AI Office Runtime。UI 是工作区入口，Agent 是编排层，文档能力是领域层，文件/LLM/数据库/渲染是基础设施层；所有入口都通过统一的领域命令和结构化事件协作。

优先级排序：

1. 先统一 Artifact、DocumentState、AgentRun、ToolCall 和 Preview 模型。
2. 再把 Agent Loop 与工具实现解耦。
3. 再按 PPT、Word、Excel、Diagram、Chart 拆能力域。
4. 最后再重组 React feature 和页面目录。

不要先做大规模文件搬迁。先建立接口和依赖规则，再按一条完整链路迁移，例如：

```text
用户请求
→ AgentRun
→ presentation tool
→ presentation capability
→ artifact version
→ preview/render
→ structured event
→ React workspace
```

这条链路跑通后，再复制到 Word、Excel 和其他产物类型，风险最低。

## 17. 参考来源

以下均为项目官方 GitHub 仓库或官方仓库内容：

1. [AionUi](https://github.com/iOfficeAI/AionUi)
2. [OfficeCLI](https://github.com/iOfficeAI/OfficeCLI)
3. [Presenton](https://github.com/presenton/presenton)
4. [GenOffice](https://github.com/genspark-ai/genoffice)
5. [Deckium](https://github.com/sleipner42/Deckium)

补充参考：

- [AionUi architecture skill references](https://github.com/iOfficeAI/AionUi/tree/main/.claude/skills/architecture)
- [OfficeCLI repository tree](https://github.com/iOfficeAI/OfficeCLI/tree/main)
- [Presenton repository tree](https://github.com/presenton/presenton/tree/main)
- [GenOffice repository tree](https://github.com/genspark-ai/genoffice/tree/main)
- [Deckium architecture section](https://github.com/sleipner42/Deckium#architecture)

## 18. 调研限制与不确定性

- Star 数是动态指标，本文只记录调研时快照。
- GitHub README 和目录结构能说明公开架构意图，但不能完全证明所有运行时依赖关系。
- AionUi、Presenton 和 GenOffice 的仓库规模较大，本次重点分析公开 README、目录结构和架构相关文档，没有逐文件审计全部实现。
- Deckium Star 数较低，但其 Agent Loop 和模块边界与本项目高度相关，因此保留为架构参考，而不是社区成熟度参考。
- “相似”按产品工作流和技术问题判断，不代表这些项目在商业定位、许可证或部署模式上完全一致。
