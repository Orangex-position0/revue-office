# 后端架构重构 Spec

> 状态：已确认的架构方向
>
> 关联文档：[`docs/roadmap.md`](../roadmap.md)、[`docs/research/ai-office-architecture-reference.md`](../research/ai-office-architecture-reference.md)

## 1. 背景

`revue-office` 当前使用 Tauri 2、React、Rust、Axum、SQLx 和 LLM 流式调用，后端已经包含 Agent Loop、工具注册、文件处理、文档生成、渲染、数据库和路由等能力。

当前主要问题不是功能缺失，而是职责边界交叉：

- Agent Loop 同时处理 Prompt、工具调用、上下文、事件和业务规则。
- Tool 直接持有会话、用户、项目、附件、SSE 回调和临时状态。
- Route、Tauri Command、数据库和具体能力之间缺少统一的 Application 用例层。
- 数据库实现容易泄漏到上层。
- `ToolResult` 和事件模型不足以支持统一预览、校验、版本和回滚。

本 Spec 定义后端的目标依赖方向和模块边界，不规定 Agent Runtime 的最终内部算法。Agent Runtime 的具体构建方案另行设计。

## 2. 已确认的架构决策

### 2.1 总体架构

采用：

> **模块化单体 + Clean Architecture 的依赖方向**

不采用当前阶段的微服务，也不强制采用完整 DDD。

目标依赖方向：

```text
Transport
   ↓
Application
   ↓
Agent Runtime / Capabilities
   ↓
Ports / Contracts
   ↓
Infrastructure
```

依赖只能从上层指向抽象，具体实现位于 Infrastructure。

### 2.2 不使用 Domain 层

本项目当前明确不建立独立的 `domain/` 层，也不强制使用 Entity、Aggregate、Domain Event 等 DDD 模式。

原因：

- 当前主要问题是 Agent 编排、工具能力和基础设施隔离；
- 业务不变量和复杂生命周期尚未稳定到需要完整 DDD 的程度；
- 过早引入 Domain 层可能增加映射和抽象成本。

稳定的跨模块数据结构放入 `contracts/`，依赖抽象放入 `ports/`。如果未来某个对象出现稳定的不变量和复杂生命周期，再局部引入更强的领域模型，不改变本 Spec 的总体边界。

### 2.3 单 Crate 优先

本次先保持单 Rust Crate，通过目录和依赖规则建立模块边界。

暂不模仿 pi 的 TypeScript Workspace 拆分为多个 Rust Crate。只有在 Agent Runtime、Office Engine 或能力模块需要独立复用、发布、测试或显著降低编译成本时，再考虑 Rust Workspace。

## 3. 目标目录

```text
src-tauri/src/
├── app/
│   ├── bootstrap.rs             # 依赖组装和应用启动
│   └── state.rs                 # 应用级状态
├── transport/
│   ├── routes/                  # Axum 路由
│   ├── commands.rs              # Tauri Command
│   ├── dto.rs                   # HTTP/Tauri DTO
│   ├── error.rs                 # 传输错误转换
│   └── events/                  # SSE/Tauri 事件适配
├── application/
│   ├── chat_service.rs
│   ├── artifact_service.rs
│   ├── document_service.rs
│   ├── export_service.rs
│   ├── file_service.rs
│   └── settings_service.rs
├── agent/
│   ├── runtime.rs               # Runtime 对外边界，内部方案待定
│   ├── tool_registry.rs
│   ├── skill_registry.rs        # 后续扩展接口
│   ├── event.rs
│   └── context.rs
├── capabilities/
│   ├── presentation/
│   ├── document/
│   ├── spreadsheet/
│   ├── diagram/
│   ├── chart/
│   ├── file/
│   ├── search/
│   └── render/
├── contracts/
│   ├── conversation.rs
│   ├── agent_run.rs
│   ├── tool.rs
│   ├── artifact.rs
│   ├── document.rs
│   ├── preview.rs
│   ├── job.rs
│   └── events.rs
├── ports/
│   ├── repositories/
│   │   ├── session.rs
│   │   ├── project.rs
│   │   ├── artifact.rs
│   │   └── agent_run.rs
│   ├── llm.rs
│   ├── renderer.rs
│   ├── file_storage.rs
│   ├── event_sink.rs
│   └── transaction.rs
├── infrastructure/
│   ├── persistence/
│   │   ├── factory.rs
│   │   ├── mysql/
│   │   ├── postgres/
│   │   └── migrations/
│   ├── llm/
│   ├── filesystem/
│   ├── renderers/
│   ├── search/
│   └── config/
└── lib.rs
```

目录可以渐进式建立，不要求第一阶段一次性搬迁所有文件。

## 4. 模块职责与边界

### 4.1 Transport

负责：

- HTTP、SSE、Tauri Command 的入口；
- DTO 解析与校验；
- 鉴权信息提取；
- 调用 Application Service；
- 将 Application Error 转换成 HTTP、Tauri 或 SSE 表达；
- 将内部 Agent Event 转换为传输事件。

禁止：

- 直接访问数据库 Repository；
- 直接调用 `LlmClient`；
- 直接实例化具体 Tool、Renderer 或数据库连接；
- 承载业务编排。

### 4.2 Application

负责面向用户用例进行编排，例如：

- 创建和恢复会话；
- 启动一次 Agent Run；
- 保存 Artifact；
- 执行导出；
- 查询文件和项目；
- 管理设置。

Application Service 可以组合 Agent Runtime、Capability、Repository 和事务 Port，但不实现文档格式细节。

### 4.3 Agent Runtime

本次只锁定边界，不锁定内部 Loop 实现。

Runtime 负责：

- 组装上下文和 Prompt；
- 调用模型；
- 选择和执行 Tool；
- 管理 Turn、取消、超时和错误；
- 维护 Agent Run 状态；
- 发布统一 Agent Event；
- 调用 Context Compact、Retry 和 Tool Registry。

Runtime 不负责：

- PPT、Word、Excel 文件细节；
- 数据库查询；
- Axum Response 或 Tauri AppHandle；
- 前端 SSE 事件；
- 具体业务用例编排。

### 4.4 Capabilities

Capability 是应用内部可复用的稳定能力，例如：

```text
PresentationCapability
DocumentCapability
SpreadsheetCapability
DiagramCapability
ChartCapability
FileCapability
SearchCapability
RenderCapability
```

同一个 Capability 可以被以下入口调用：

- Agent Tool；
- Application Service；
- Tauri Command；
- 自动化测试；
- 未来 CLI 或 MCP Adapter。

Capability 不依赖 Axum、Tauri、React 或具体前端事件。

### 4.5 Contracts

`contracts/` 只存放跨模块共享的数据结构和协议类型，不表示 DDD Domain 层。

建议至少定义：

```text
Conversation
AgentRun
ToolCall
ToolExecutionResult
Artifact
ArtifactVersion
DocumentState
Preview
ValidationResult
Job
AgentEvent
```

Contracts 不得依赖：

- SQLx；
- MySQL/PostgreSQL 类型；
- Axum；
- Tauri；
- 具体 Infrastructure 实现。

### 4.6 Ports

`ports/` 由上层定义依赖抽象，Infrastructure 负责实现。

典型 Port：

```text
SessionRepository
ProjectRepository
ArtifactRepository
AgentRunRepository
LlmProvider
Renderer
FileStorage
EventSink
TransactionManager
```

Ports 不应返回 SQLx Row、HTTP Response 或具体数据库错误。

## 5. Tool、Skill、MCP 与 Capability

四者关系如下：

```text
Skill / MCP
    ↓
Tool
    ↓
Capability
    ↓
Ports
    ↓
Infrastructure
```

### Tool

Tool 是 Agent 一次可以调用的结构化操作，包含：

- 名称和描述；
- JSON Schema 参数；
- 只读/写入属性；
- 权限要求；
- 超时和取消策略；
- 结构化执行结果。

Tool 不应直接操作 SQL 或拼接文档底层 XML。

### Skill

Skill 是领域知识、Prompt、流程规则和 Tool 使用指导的组合。Skill 不直接访问数据库或文件系统，而是通过 Tool 使用 Capability。

### MCP

MCP 是外部 Tool、Resource、Prompt 的接入协议。MCP Adapter 属于未来扩展，不阻塞本次重构。

## 6. 统一 Tool Result 与 Agent Event

当前 `ToolResult` 需要逐步演进为结构化结果：

```rust
pub struct ToolExecutionResult {
    pub status: ToolStatus,
    pub observation: String,
    pub changed_entities: Vec<EntityChange>,
    pub artifacts: Vec<ArtifactRef>,
    pub previews: Vec<PreviewRef>,
    pub validation: Option<ValidationResult>,
    pub undo_token: Option<String>,
    pub next_action: Option<NextAction>,
    pub error: Option<ErrorInfo>,
}
```

建议统一事件：

```text
AgentRunStarted
ToolStarted
ToolFinished
ArtifactCreated
ArtifactUpdated
PreviewReady
ValidationFailed
RunCompleted
RunFailed
```

Agent Runtime 只发布内部 Agent Event。SSE、Tauri Event、测试收集器和未来 WebSocket 分别实现 Event Adapter。

当前 `ToolContext.emit` 可以作为兼容实现，后续替换为 `EventSink` Port。

## 7. 数据库可替换设计

### 7.1 目标

用户可以通过配置选择 MySQL 或 PostgreSQL。Application、Agent Runtime 和 Capability 不得直接依赖具体数据库或 SQLx。

硬性约束：

> 新增数据库实现时，不应修改上层 Application、Agent Runtime、Capability 和 Port 接口。

### 7.2 依赖结构

```text
Application / Capability
          ↓
Repository Port
          ↑
MySqlRepository / PostgresRepository
          ↑
Persistence Factory
          ↑
DatabaseConfig
```

示例：

```rust
#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Session>, RepositoryError>;
    async fn save(&self, session: &Session) -> Result<(), RepositoryError>;
}

pub struct AppRepositories {
    pub sessions: Arc<dyn SessionRepository>,
}
```

### 7.3 运行时选择

采用：

- MySQL 和 PostgreSQL 驱动在编译期启用；
- 应用启动时根据 `DatabaseConfig` 选择实现；
- Factory 返回由 Port 组成的 `AppRepositories`；
- Application 只持有 Port 的 trait object。

```rust
pub enum DatabaseBackend {
    MySql,
    Postgres,
}

pub struct DatabaseConfig {
    pub backend: DatabaseBackend,
    pub url: String,
    pub max_connections: u32,
}

pub async fn build_repositories(
    config: &DatabaseConfig,
) -> Result<AppRepositories, InfrastructureError>;
```

### 7.4 SQLx 使用原则

- SQLx Row、查询、迁移和数据库连接只存在于 Infrastructure；
- SQLx DTO 不跨越 Persistence 边界；
- 可移植 SQL 优先；
- 对 JSON、UUID、时间、分页、锁和批量操作等差异允许按数据库分别实现；
- 不以 `sqlx::Any` 作为核心抽象，除非后续验证其类型映射和迁移能力满足需求；
- MySQL/PostgreSQL 迁移脚本分开维护；
- 先隔离 Repository，再执行 MySQL → PostgreSQL 迁移。

### 7.5 事务边界

- Application Service 定义业务事务边界；
- Repository/Transaction Port 提供事务能力；
- Infrastructure 实现具体事务；
- Agent Runtime 不直接开启数据库事务；
- 长任务不持有数据库事务，采用阶段性提交和状态更新。

## 8. Job 预留

本次不实现完整 Job Runtime，但预留 `Job` Contract 和相关 Port。

建议状态：

```text
queued → running → succeeded
                 ↘ failed
                 ↘ cancelled
```

PPT 生成、视频生成、大文件解析和导出等长任务未来可以由：

```text
AgentRun → Job → Task Steps → Tool / Capability
```

完整 Job Runtime、取消、重试、恢复、进度和后台持久化列入 `v0.2.0`，见 [`docs/roadmap.md`](../roadmap.md)。

## 9. Multi-tenant 预留

当前不实现 Multi-tenant，但持久化对象应保留明确归属关系，避免设计成全局单例：

```text
User → Workspace → Project → Conversation / Artifact / AgentRun / Job
```

Multi-tenant 的 Tenant/Organization 模型、权限隔离和数据库部署模式列入 `v0.2.0` 架构评估，不在本次重构中实现。

## 10. 错误模型

错误按边界转换：

```text
InfrastructureError
        ↓
CapabilityError
        ↓
ApplicationError
        ↓
TransportError / AgentEvent::Error
```

约束：

- Infrastructure 不返回 HTTP 状态码；
- Capability 不构造 SSE 错误；
- Application 不暴露 SQLx 错误；
- Transport 负责 HTTP/Tauri/SSE 错误表达；
- Agent Runtime 将模型、工具和运行错误转换成 Agent Event。

第一阶段可以使用统一的 `AppError` 枚举，随后按边界逐步拆分。

## 11. 入口交互模式

统一调用链：

```text
Axum Route / Tauri Command
          ↓
DTO 转换、鉴权和输入校验
          ↓
Application Service
          ↓
Agent Runtime / Capability
          ↓
Ports
          ↓
Infrastructure Adapter
```

Agent 流式链路：

```text
Agent Runtime
      ↓
AgentEvent
      ├── SSE Adapter
      ├── Tauri Event Adapter
      └── Test Collector
```

Transport 不应将 HTTP/SSE 类型传入 Agent Runtime。

## 12. 迁移计划

### 阶段一：契约和依赖边界

- 创建 `contracts/` 和 `ports/`；
- 定义 AgentRun、ToolCall、Artifact、Preview 和 AgentEvent；
- 定义数据库 Repository Port；
- 定义 Application Service 接口；
- 保留现有实现，建立兼容适配器。

### 阶段二：Agent Runtime 边界

- 将当前 `agent_loop.rs` 的外部入口收敛到 Runtime；
- 把 SSE 推送改为 EventSink 适配；
- 将 Tool Registry 与具体 Capability 解耦；
- 不在本阶段决定 Context Compact、死循环控制和 Prompt 外置的最终方案。

### 阶段三：Capability 抽离

优先顺序：

1. Presentation；
2. Artifact、Preview 和 Render；
3. Document；
4. Spreadsheet；
5. Diagram、Chart、File、Search。

### 阶段四：Persistence Adapter

- 将现有 SQL 查询和 SQLx Row 收拢到 Infrastructure；
- 修复会话历史的 TEXT/BLOB 类型错误；
- 建立 MySQL/PostgreSQL Repository 实现；
- 通过 Factory 运行时选择数据库；
- 增加真实数据库集成测试。

### 阶段五：前置验证后再考虑 Workspace

当 Agent Runtime 或 Office Capability 出现独立复用、发布、测试或编译需求时，再拆分 Rust Workspace。当前不以 Workspace 作为重构前置条件。

## 13. 验收标准

### 13.1 结构验收

- `transport` 不直接依赖 Infrastructure 具体实现；
- `agent` 不依赖 Axum、Tauri；
- `capabilities` 不依赖 HTTP Response、React 或 Tauri；
- `contracts` 不依赖 SQLx；
- `ports` 不依赖 MySQL/PostgreSQL 具体类型；
- Route 和 Tauri Command 不直接访问 Repository。

### 13.2 行为验收

- Agent Event 可以被 SSE 和 Tauri Event 分别消费；
- MySQL/PostgreSQL 可以通过配置切换；
- Application Service 不感知具体数据库；
- Tool 可以在不启动 UI 的情况下独立测试；
- Capability 可以被 Agent Tool 和 Application Service 复用。

### 13.3 数据验收

- Repository 返回应用类型，而不是 SQLx Row；
- Session、Artifact、AgentRun 可以正确持久化；
- 数据库类型错误不会泄漏到 Transport；
- MySQL/PostgreSQL 迁移脚本可以分别执行；
- 长任务不依赖未提交的长事务。

### 13.4 测试验收

- Port 使用 fake/in-memory adapter 测试；
- Capability 使用 mock Port 测试；
- Application 使用 fake Repository 测试；
- Infrastructure 使用真实数据库集成测试；
- Transport 测试 DTO、鉴权和事件转换；
- 增加依赖方向或架构规则检查。

## 14. 非目标

本次重构不包含：

- 完整 DDD Domain/Entity/Aggregate 建模；
- 完整 Job Runtime；
- Multi-tenant 实现；
- MCP Adapter 实现；
- 用户自定义 Skill 平台；
- Agent Runtime 内部 Loop 的最终设计；
- Context Compact 的最终策略；
- PostgreSQL 迁移的具体发布日期；
- Rust Workspace 拆分；
- 微服务拆分。

## 15. 未决问题

以下问题保留为后续专题，不阻塞本次模块化重构：

1. Agent Runtime 的具体 Loop 状态机；
2. Context Compact 策略；
3. Prompt 是否全部外置为文档；
4. Skill 是否支持用户自定义；
5. MCP Adapter 的生命周期；
6. PostgreSQL 迁移的具体时间；
7. 是否需要独立 Rust Workspace；
8. Capability 是否最终拆分为多个 Crate；
9. Multi-tenant 的租户模型；
10. Job Runtime 是否需要持久化队列；
11. Agent Loop 死循环检测和恢复策略；
12. Tool 权限、审批和副作用确认机制。

## 16. 关键架构规则摘要

```text
1. Transport 只负责入口和协议适配。
2. Application 负责用例编排和事务边界。
3. Agent Runtime 负责 Agent 执行，不负责业务能力细节。
4. Capability 负责可复用的办公能力。
5. Tool 是 Agent 可调用的结构化操作。
6. Skill 是知识、Prompt 和流程规则，不直接访问基础设施。
7. MCP 是未来的外部能力接入协议。
8. Contracts 不依赖具体实现。
9. Ports 由上层定义，Infrastructure 负责实现。
10. Application、Agent、Capability 不依赖具体数据库。
11. 数据库通过 Factory 在启动时选择 MySQL 或 PostgreSQL。
12. Event 与 SSE/Tauri 传输分离。
13. 长任务不持有长事务。
14. 先单 Crate，只有出现独立复用需求时再拆 Workspace。
```
