# v0.1.0

## 前端修改

不要完全和 E:\test\References\walioffice 的前端一样，修改项有：

- [x] 不要增加一个扫码登录页面（原项目需要扫公众号获取验证码才能使用项目，本质是给公众号引流，但本项目不需要引流）
- [ ] 前端的部分样式是否需要修改？因为我不想和原项目一模一样，比如：
    - [ ] 应用是否有设置按钮？比如浅色深色模式、api key 的配置等？设置按钮要放到哪里？（画面左下角吗？）

## CICD

- [ ] 项目中将 lefthook 改为 prek。先提醒我可以直接使用相关 skill (`bootstrap`)

## Others

- [ ] 项目的 logo 需要重新设计
- [x] 项目改用 rust 2024 edition

## Bugs

- [ ] 我用 pnpm tauri dev 启动后，进行了一轮对话，然后关闭应用重启（都是 pnpm tauri dev），但恢复对话历史失败。错误信息：

```bash
2026-09-24T03:59:38.654877Z ERROR revue_office_lib::routes::session: [Session] get_session_detail error: Database(ColumnDecode { index: "1", source: "mismatched types; Rust type `alloc::vec::Vec<u8>` is not compatible with SQL type `TEXT`" })
```

## 后端架构重构

- 先讨论使用什么架构？Clean Architecture 吗？（搜索一下github上 star 数较多的5个同类产品用的什么架构和模块组织方式），再讨论模块组织方式，模块边界，各个模块的交互模式。我的初步想法：
    - 应用层(负责连接 Agent Runtime 和各种能力，类似编排层)、Agent Runtime 层（存放 Agent 核心）、扩展层（Skills / Tools / MCP）、基础设施层分开（参考 pi，Agent Runtime 单独用一层，各种扩展能力放在其它层）
    - 是否要引入 ddd domain 概念？ddd 对于业务类系统更合适，对于本项目不一定合适？
    - 要充分利用“依赖倒置”，上层只定义抽象，infra 层才定义实现

# v0.1.1

## 一些待讨论项

- [ ] 数据库 MySQL 改为 PostgreSQL
- [ ] System Prompt 是否还可以优化
- [ ] Context Compact 策略优化（可参考 pi 和 codex）
- [ ] Agent Loop 死循环问题（参考 pi 和 codex）
- [ ] 是否要扩展为“multi-tenant”，其适用于现在的业务背景吗？若扩展，有什么优缺点？
- [ ] 将所有 Tool 中和“场景推断”有关的 prompt 都拆分为文档？或是所有 prompt 都拆分为文档，然后在代码中引用，而不是硬编码？
- [ ] web_search Tool 中的中文 mcp 是否要修改？不使用百度，还能使用什么？
- [ ] ppt 的生成，不能只靠模型能力。你推荐加入 Skill 系统吗（模仿其他 coding agent），并内置部分 ppt skill 供用户选择是否使用？
- [ ] 设置 -> 基础设置 -> 显示主题 -> 内容主题，这个内容主题的作用是？代码是如何写的（内置提示词吗）？这个功能有必要吗？

---

# v0.2.0

## 后端架构扩展

- [ ] 引入 Job Runtime，支持长任务、取消、重试、恢复和进度
- [ ] 支持 AgentRun 与 Job 的关联
- [ ] 增加后台任务持久化和状态查询
- [ ] 根据实际使用情况扩展 Skill Registry
- [ ] 增加 MCP Adapter
- [ ] 支持多入口复用 Agent Runtime

## Multi-tenant 架构评估

- [ ] 评估 Tenant/Organization 模型
- [ ] 完善 User、Workspace、Project 的归属边界
- [ ] 评估权限隔离和数据访问策略
- [ ] 评估多租户数据库部署模式

> Multi-tenant 在 v0.2.0 先进行架构评估，不承诺本版本完成实现。
