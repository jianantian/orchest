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
| v0.7 | 功能 | 扩展性地基 + Actor（Ractor PoC → 通过、Hook 框架、Agent-as-Tool + Handoff 两层语义、LLM Retry、Loop Detection、WorkerActor refactor、8 个使用示例） |
| v0.8 | 功能 | 持久化 + 安全 + Supervised Delegation 基础（SessionStore + SQLite、Guardrail 四层框架、ApprovalMode、Multi-subscriber Events + Watcher + InjectCmd） |
| v0.9 | 功能 | Supervised Delegation 端到端（Approval 枚举、结构化 ToolError、as_tool Builder、Steering API、LlmWatcher、supervisor 恢复、多 watcher FIFO、端到端示例） |
| v0.9.1 | 卫星 | ASR Provider Gateway（`agent-runtime-asr-providers`、Volcengine/Aliyun adapter、duplex streaming、observability） |
| v0.9.2 | 卫星 | 文档（rustdoc 清理、quickstart、Python/TS SDK 指南、basic_agent_run 示例、CI doc/example 防线） |
| hotfix 06-12 | 重构 | Code Review 问题清偿（ASR 安全、Core Runtime 正确性、Type Stub 对齐、Event Backpressure、MCP/Node 可靠性） |
| v0.9.3 | 卫星 | TTS Provider Gateway（`agent-runtime-tts-providers`、voice catalog、streaming synthesis、Volcengine/Aliyun adapter、observability） |

## 迭代编号约定

- **主线迭代**（v0.7、v0.8、v0.9）：runtime 核心能力演进，有依赖链
- **卫星迭代**（v0.6.1、v0.8.1 ...）：与主线并行或从已完成主线切出的独立模块（易用性工具、扩展 crate 等）。独立 crate，不阻塞主线，按就绪时间合入

## 规划中

### SDK 验证产品迭代（编号待定）

用一个真实的简单产品 dogfood SDK，验证其完备性和易用性。验证结论可能反过来影响 API 形状，因此须排在公开发布之前。SDK 语言（Python / TS / Rust）与产品形态待单独规划。

**依赖**：v0.9.2 文档（验证者参照文档上手）

### v1.0 — 首次公开发布（规划）

第一个公开发布到 crates.io 的版本。包含发布准备的全部内容：Cargo publish 元数据、license 定稿、release workflow、CHANGELOG、版本号策略文档。

**依赖**：SDK 验证产品迭代完成（API 经真实产品验证后才发布）

### 依赖图

```
✅ v0.7: Hook + Handoff + Actor
            │
            ▼
✅ v0.8: Session + Guardrail
        + SD 基础通信层
            │
            ▼
✅ v0.9.2: 文档
            │
            ▼
   SDK 验证产品迭代（dogfooding，编号待定）
            │
            ▼
   v1.0: 首次公开发布（crates.io + release workflow + license 定稿）

✅ v0.9.1: ASR Provider Gateway（卫星，已完成）
✅ v0.9.3: TTS Provider Gateway（卫星，已完成）
```

## 能力缺口全景

下表汇总研究文档（[vs OpenAI/Claude SDK](../research/orchest-vs-openai-claude-sdk.md)、[vs DeerFlow/Craft](../research/orchest-vs-craft-deerflow-gap-analysis.md)、[vs pi-agent](../research/orchest-vs-pi-agent-gap-analysis.md)、[Actor Model 评估](../research/actor-model-evaluation.md)、[Sub-agent Handoff vs Agent-as-Tool](../research/sub-agent-handoff-vs-agent-as-tool.md)）识别的缺口与规划版本的映射：

| 能力 | 当前状态 | 规划版本 |
|------|---------|---------|
| Actor Model（Ractor）PoC | ~~proto-actor~~ → WorkerActor (Ractor) | ✅ v0.7 |
| Hook / 中间件框架 | ~~零扩展点~~ → Hook trait（9 hook 点） | ✅ v0.7 |
| Sub-agent 语义统一（Agent-as-Tool + Handoff） | ~~双路径并行~~ → Agent-as-Tool + Handoff 统一 | ✅ v0.7 |
| LLM Retry / 容错 | ~~失败直接 return~~ → RetryPolicy（指数退避） | ✅ v0.7 |
| Loop Detection | ~~无~~ → LoopDetectionHook（warn + abort） | ✅ v0.7 |
| Session 持久化 | ~~无~~ → SessionStore trait + InMemory + SQLite | ✅ v0.8 |
| Guardrails | ~~无~~ → 四层 Guardrail 框架（Input/Output/ToolInput/ToolOutput） | ✅ v0.8 |
| 权限模型扩展 | ~~`requires_approval: bool`~~ → run 级 ApprovalMode 策略 | ✅ v0.8 |
| 双向通信 + 多方事件订阅 | ~~RunHandle 只有 wait + approval~~ → Multi-subscriber + Watcher + InjectCmd | ✅ v0.8 |
| Supervised Delegation 基础 | ~~无~~ → Watcher trait + InjectCmd 双向通信 | ✅ v0.8 |
| Mid-run Steering | ~~只有 approval~~ → Steering API | ✅ v0.9 |
| Supervised Delegation 完整 | ~~无~~ → LlmWatcher + supervisor 恢复 + 多 watcher FIFO | ✅ v0.9 |
| Image AIGC Gateway | ~~无~~ → **v0.6.1 已完成** | ✅ |
| ASR Provider Gateway | ~~无~~ → **v0.9.1 已完成** | ✅ |
| TTS Provider Gateway | ~~无统一 TTS provider crate~~ → **v0.9.3 已完成** | ✅ |
