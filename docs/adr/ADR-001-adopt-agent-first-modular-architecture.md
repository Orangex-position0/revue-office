---
编号: ADR-001
标题: 采用 Agent-first 模块化架构
状态: Accepted
日期: 2026-09-30
作者: Revue Office team
最后修改: 2026-10-03
---

# ADR-001：采用 Agent-first 模块化架构

## 上下文

Revue Office 是基于 Rust 2024 Edition、Tauri 2、Axum、Tokio 和 SQLx 构建的对话驱动 AI Office 应用。当前后端是单 Crate，运行时在 Tauri 桌面进程内启动 Axum HTTP 服务，并同时支持 SQLite 和 MySQL。

现有代码正在渐进迁移：Chat、Session、Presentation 和 Artifact 已具备 Application、Port 与 Infrastructure 边界；Auth、Project、File、Notification、Settings 和 Dashboard 仍存在 Route 直接访问 `db` 与全局状态的路径。仓库还存在以下结构问题：

- `routes` 与 `transport`、`db` 与 `infrastructure/persistence`、`models` 与 `contracts` 职责重叠；
- `agent` 同时包含通用 Agent Runtime、Office Tool 和产品专属数据类型；
- 全局数据库、Service、配置和 Tool Registry 隐藏了依赖；
- 仅靠目录名称无法稳定表达依赖方向；
- `mod.rs` 与同名模块文件混用；
- Provider 差异、Agent 执行、Office 能力和产品用例尚未形成清晰边界。

当前未记录固定团队规模、性能指标或交付期限。本决策优先解决可维护性、可替换性、测试隔离和渐进迁移问题，同时要求保持现有 HTTP/SSE 契约以及 SQLite/MySQL 数据兼容。

## 决策

我们决定采用 **Agent-first 模块化架构**，将后端组织为职责明确的 `providers`、`agent_core`、`agent`、`capabilities`、`application`、`infrastructure`、`transport` 和 `bootstrap` 子系统。

具体来说：

- `providers` 提供统一的 AI Provider 请求、响应、流事件和错误，并在内部区分统一契约、API 线协议与 Provider 身份；
- `agent_core` 拥有通用 Agent Runtime、Loop、Tool 协议、取消、超时、终态和通用输出，只依赖 Provider 公共契约；
- `agent` 将 Agent Core、Provider、Prompt 和 Tool 组装为 Office Agent，并对 Application 暴露稳定 `AgentRunner`；
- `capabilities` 按 Presentation、Document、Spreadsheet、Diagram、Media 等 Office 能力组织，Capability 可被多个 Tool 使用且不依赖 Agent；
- `application` 按 Identity、Conversations、Assets、Artifacts、Projects、Preferences 等产品用例组织；
- `transport` 仅负责 HTTP、SSE 和未来 Tauri Command 的协议转换；
- `infrastructure` 实现持久化、文件系统、导出、提取、身份、凭据、搜索和 OCR 等 Port；
- `bootstrap` 是唯一选择具体 Adapter 并构造对象图的位置；
- Port 由提出需求的模块拥有，不再保留全局 `ports` 目录；
- 全局 `models.rs` 与 `contracts` 最终拆回语义所有者；
- Tool 是 Capability 的 Agent Adapter，Capability 不由 Tool 组成；
- Provider Event、Agent Event、Application Event 和 HTTP Stream Event 在边界逐层转换；
- 使用实例级 Tool Registry、显式 Axum State 和构造函数注入，删除 Service Locator 与全局数据库状态；
- 近期保持单 Crate，通过 Rust 可见性和 AST 架构测试强制边界；边界稳定并出现独立发布需求后再考虑 Workspace；
- 每迁移一个子系统时同步转换为同名模块文件布局，最终删除全部 `mod.rs`；
- 跨 Crate 公开的错误枚举必须添加 `#[non_exhaustive]`；
- 重构期间保持现有 HTTP、JSON、SSE 与数据库数据兼容。

本决策不包含：详细实施排期、立即拆分 Workspace Crate、具体系统凭据库选型、API 协议重设计，以及新增 Provider/Capability 的产品范围。

## 后果

正面：

- 通用 Agent Runtime 与 Office 专属能力分离，`agent_core` 可独立测试并具备未来提取为 Crate 的条件；
- Provider 差异被统一契约与 API 协议 Adapter 吸收，Agent 和 Capability 不依赖具体厂商；
- Application、Transport 和 Infrastructure 的依赖方向清晰，SQLite/MySQL、文件存储和凭据实现可替换；
- Capability 可以脱离 Agent 从 HTTP、批处理、测试或其他入口复用；
- 显式依赖注入与实例级 Registry 减少全局状态和测试互相污染；
- AST 架构测试和递减的遗留 allowlist 可以防止迁移过程重新引入反向依赖。

负面/中性：

- 同一业务数据在 Conversation、Agent、Provider 和 HTTP 边界需要显式转换，代码量会增加；
- 单 Crate 内的模块边界仍弱于 Cargo Crate 边界，需要持续维护可见性和架构测试；
- 渐进迁移期间新旧结构将暂时共存，开发者需要理解兼容区和目标区；
- Provider、Agent Core、Agent、Capability 与 Application 的职责比传统三层架构更细，需要文档和评审保持术语一致；
- 移除全局状态和万能模型类型会触及大量调用路径，迁移必须分阶段完成。

## 替代方案

- **方案 A：继续采用纯水平分层**
  - 优点：目录数量较少，Transport、Application、Domain、Infrastructure 的概念普遍易懂。
  - 缺点：Agent Core、Provider、Office Capability 和产品用例之间仍容易堆积在同一层，无法准确表达本产品的核心执行模型。
  - 不选原因：Revue Office 的主链路是 Agent 驱动而非普通 CRUD，纯水平分层不足以分离通用 Runtime 与 Office 能力。

- **方案 B：采用纯 Feature-first 结构**
  - 优点：Identity、Chat、Presentation、Files 等业务代码可以就近组织，单个 Feature 容易导航。
  - 缺点：Agent Runtime、Provider 协议、Artifact 发布等跨 Feature 机制会重复或形成新的共享杂物模块。
  - 不选原因：产品的核心是一个由多种 Office Capability 支撑的统一 Agent，而不是彼此独立的 HTTP Feature 集合。

- **方案 C：立即拆分为 Cargo Workspace 多 Crate**
  - 优点：Cargo 依赖关系可以提供最强的编译期边界，并支持独立版本和发布。
  - 缺点：当前模块边界仍在调整，过早拆分会固化错误接口，并显著增加构建、测试和迁移成本。
  - 不选原因：先在单 Crate 中通过门面、可见性和架构测试验证边界，稳定后再拆分风险更低。

- **方案 D：只统一文件夹和删除 `mod.rs`**
  - 优点：改动简单，对运行行为影响较小。
  - 缺点：不会解决全局状态、反向依赖、类型所有权和重复抽象问题。
  - 不选原因：文件布局不是当前架构问题的根因，单纯改名无法形成可验证边界。

## 撤销条件

当以下任一可观测条件出现时，应重新评估本决策：

- 连续两个发布周期中，超过 50% 的核心用户工作流不再以 Agent/Conversation 为入口，产品定位已转为 Project-first 或文档编辑器优先；
- `agent_core` 或 `providers` 被第二个独立仓库正式依赖，或需要独立版本和发布，此时应评估将其提取为 Workspace/独立 Crate；
- 连续两个发布周期内，架构测试需要保留 3 个或以上经过批准的跨子系统反向依赖例外，表明当前边界划分与真实协作关系不符；
- 后端被拆分为独立部署服务，且至少两个子系统需要独立扩缩容、独立数据所有权或独立发布，此时应重新设计进程与服务边界；
- Provider 子系统新增三种以上非 Chat 操作，且统一契约持续出现无关字段或运行时分支，此时应重新评估按操作拆分 Provider Crate/API 的方案。

## 变更历史

| 日期 | 变更类型 | 原因 | 操作人 |
| --- | --- | --- | --- |
| 2026-09-30 | 创建 | 记录后端 Agent-first 模块化架构决策 | Revue Office team |
| 2026-10-03 | 状态变更 | S-001 至 S-007 已完成实施与最终验证，决策状态改为 Accepted | Revue Office team |
