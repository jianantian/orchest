# Orchest 迭代路线图

## 迭代节奏

功能迭代和重构迭代不固定交替。原则：**功能堆叠产生足够结构熵时，停下来重构降熵，然后继续**。

判断"该重构了"的信号：
- 新功能需要"绕过"已有结构才能实现
- 文件/函数超长、职责混杂、重复代码积累
- lint 违规需要 suppress 才能通过
- 测试变脆弱，改一处破多处

## 已完成

| 版本 | 类型 | 主题 |
|------|------|------|
| v0.1 | 功能 | 最小可用 Runtime（核心 loop、Tool trait、Anthropic adapter、双语言 SDK） |
| v0.2 | 功能 | MCP 集成 + 生产增强（MCP stdio/HTTP、context compaction、并行 tool call） |
| v0.3 | 功能 | 生产级完整度（sub-agent、skill 系统、code execution、budget guard） |
| hotfix 05-23 | 重构 | Runtime Contract Repair（权限 enforce、ToolMetadata enforce、SDK-skill 接通、MCP HTTP 重试安全） |
| v0.4 | 功能 | 验收 + 扩展骨架（e2e 验证、文档） |
| v0.5 | 功能 | Provider 独立 Crate（`agent-runtime-providers` 拆分、OpenAI/DeepSeek/OpenRouter adapter） |
| v0.6 | 重构 | 架构健壮性改造（run.rs 拆分、BudgetGuard 定价解耦、ApprovalBus、AgentConfig builder、MCP 并发化） |
| hotfix 05-26 | 重构 | Review 问题清偿（bug 修复、Error 治理、依赖反转、ProviderFactory、CancellationToken、CI 防线） |

## 迭代编号约定

- **主线迭代**（v0.7、v0.8、v0.9）：runtime 核心能力演进，有依赖链
- **卫星迭代**（v0.6.1、v0.8.1 ...）：与主线并行或从已完成主线切出的独立模块（易用性工具、扩展 crate 等）。独立 crate，不阻塞主线，按就绪时间合入

## 规划中

### [v0.6.1 — Image AIGC Gateway](./v0_6_1/spec.md)

统一图像生成/编辑 gateway（`agent-runtime-aigc-providers`）、4 个 provider adapter、资产持久化与公共输出 contract。

这是从 v0.6 后切出的卫星迭代，独立 crate，不依赖 v0.7 Hook 框架，也不阻塞 v0.7 主线。

### [v0.7 — 扩展性地基](./v0_7/spec.md)

Hook 框架 + Handoff 重构 + LLM Retry + Loop Detection。

让 runtime 从"硬编码循环"变为"可扩展的 hook 链"，同时清理 sub-agent 双路径问题。Hook 框架是后续所有能力（guardrail、session、steering）的前置条件。

**依赖**：hotfix 2026-05-26 完成

### [v0.8 — 持久化与安全](./v0_8/spec.md)

Session 持久化 + Guardrails + 权限模型扩展。

基于 v0.7 的 hook 框架，补齐产品级 agent 应用需要的状态管理和安全层。Session 通过 hook 回调实现，Guardrail 作为 Hook trait 的具体实现提供。

**依赖**：v0.7 Hook 框架

### [v0.9 — 产品成熟度](./v0_9/spec.md)

Mid-run Steering + Provider 扩展 + Examples + 发布准备。

扩展 RunHandle 的运行时交互能力，扩大 provider 覆盖，补齐文档和示例，为首次公开发布做准备。

**依赖**：v0.8 Session 持久化（steering 需要稳定的状态管理）

## 能力缺口全景

下表汇总研究文档（[vs OpenAI/Claude SDK](../research/orchest-vs-openai-claude-sdk.md)、[vs DeerFlow/Craft](../research/orchest-vs-craft-deerflow-gap-analysis.md)、[vs pi-agent](../research/orchest-vs-pi-agent-gap-analysis.md)）识别的缺口与规划版本的映射：

| 能力 | 当前状态 | 规划版本 |
|------|---------|---------|
| Hook / 中间件框架 | 零扩展点 | **v0.7** |
| Sub-agent 语义统一（Handoff） | 双路径并行 | **v0.7** |
| LLM Retry / 容错 | 失败直接 return | **v0.7** |
| Loop Detection | 无 | **v0.7** |
| Session 持久化 | 无 | **v0.8** |
| Guardrails | 无 | **v0.8** |
| 权限模型扩展 | `requires_approval: bool` | **v0.8** |
| Mid-run Steering | 只有 approval | **v0.9** |
| Provider 扩展 | 2 个 | **v0.9** |
| Image AIGC Gateway | 无 | **v0.6.1**（卫星） |
