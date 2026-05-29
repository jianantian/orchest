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
| v0.6.1 | 功能 | Image AIGC Gateway（`agent-runtime-aigc-providers`、4 provider adapter、资产持久化、公共输出 contract） |

## 迭代编号约定

- **主线迭代**（v0.7、v0.8、v0.9）：runtime 核心能力演进，有依赖链
- **卫星迭代**（v0.6.1、v0.8.1 ...）：与主线并行或从已完成主线切出的独立模块（易用性工具、扩展 crate 等）。独立 crate，不阻塞主线，按就绪时间合入

## 规划中

### [v0.7 — 扩展性地基 + Actor PoC](./v0_7/spec.md)

**Phase 1（gate）**：Ractor PoC——用 Ractor 实现最小 WorkerAgent + WatcherAgent，验证 actor 消息流与 run_loop 集成、Kill 优先级、typed API 封装。PoC 结论决定后续所有架构走向。

**Phase 2**：Hook 框架 + Handoff 重构（Agent-as-Tool / Handoff 两层语义）+ LLM Retry + Loop Detection。根据 PoC 结果决定 AgentRun 用 Ractor actor 还是保持 channel 原语。

**依赖**：hotfix 2026-05-26 完成

**研究输入**：[Actor Model 评估](../research/actor-model-evaluation.md)、[Sub-agent Handoff vs Agent-as-Tool](../research/sub-agent-handoff-vs-agent-as-tool.md)

### [v0.8 — 持久化 + 安全 + Supervised Delegation 基础](./v0_8/spec.md)

Session 持久化 + Guardrails + 权限模型扩展 + Supervised Delegation 基础通信层。

基于 v0.7 的 Hook 框架 + AgentRun 重构，补齐状态管理、安全层，并建立 Supervised Delegation 所需的双向通信和多方事件订阅基础。

**依赖**：v0.7 Hook 框架 + Handoff 重构

### [v0.9 — 产品成熟度 + Supervised Delegation](./v0_9/spec.md)

Mid-run Steering（基于 v0.8 双向通信基础暴露 API）+ Supervised Delegation 完整实现（watcher LLM 中途干预、崩溃恢复）+ Provider 扩展 + 文档 + 发布准备。

**依赖**：v0.8 Session 持久化 + Supervised Delegation 基础

### 依赖图

```
v0.7 Phase 1: Ractor PoC ──gate──> v0.7 Phase 2: Hook + Handoff
                                          │
                                          ▼
                                   v0.8: Session + Guardrail
                                        + SD 基础通信层
                                          │
                                          ▼
                                   v0.9: Steering + SD 完整
                                        + Provider + 发布
```

## 能力缺口全景

下表汇总研究文档（[vs OpenAI/Claude SDK](../research/orchest-vs-openai-claude-sdk.md)、[vs DeerFlow/Craft](../research/orchest-vs-craft-deerflow-gap-analysis.md)、[vs pi-agent](../research/orchest-vs-pi-agent-gap-analysis.md)、[Actor Model 评估](../research/actor-model-evaluation.md)、[Sub-agent Handoff vs Agent-as-Tool](../research/sub-agent-handoff-vs-agent-as-tool.md)）识别的缺口与规划版本的映射：

| 能力 | 当前状态 | 规划版本 |
|------|---------|---------|
| Actor Model（Ractor）PoC | proto-actor（channel 手写） | **v0.7 Phase 1** |
| Hook / 中间件框架 | 零扩展点 | **v0.7 Phase 2** |
| Sub-agent 语义统一（Agent-as-Tool + Handoff） | 双路径并行 | **v0.7 Phase 2** |
| LLM Retry / 容错 | 失败直接 return | **v0.7 Phase 2** |
| Loop Detection | 无 | **v0.7 Phase 2** |
| Session 持久化 | 无 | **v0.8** |
| Guardrails | 无 | **v0.8** |
| 权限模型扩展 | `requires_approval: bool` | **v0.8** |
| 双向通信 + 多方事件订阅 | RunHandle 只有 wait + approval | **v0.8** |
| Supervised Delegation 基础 | 无 | **v0.8** |
| Mid-run Steering | 只有 approval | **v0.9** |
| Supervised Delegation 完整 | 无 | **v0.9** |
| Provider 扩展 | 2 个 | **v0.9** |
| Image AIGC Gateway | ~~无~~ → **v0.6.1 已完成** | ✅ |
