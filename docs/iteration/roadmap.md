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
| v0.9.4 | 重构 | Runtime Failure Semantics（ErrorKind taxonomy、structured tool failure return、RetryHint dispatch、repeated failure hook、ActorRef restart 注释） |
| v0.9.5 | 重构 | Agent Control-Flow Hardening（ContextMode、handoff snapshot-then-swap、control-flow tests、`run_one_step` 拆分、Supervised Delegation 验证边界） |
| v0.9.6 | 卫星 | ASR Follow-up Providers（one-shot `transcribe()`、Deepgram、ElevenLabs Scribe、Soniox、AssemblyAI、Speechmatics、docs/examples） |
| v0.9.7 | 重构 | Tool Surface Extensions（Draft/Commit、deferred tool discovery、可选并行 tool execution） |
| v0.9.8 | 重构 | Runtime Safety and Observability Hygiene（code execution executor 注入、核心 observability、Python GIL 行为、静态错误 payload 清理） |
| v0.9.9 | 重构 | API Cleanup and Product Patterns（deprecated API 移除、binding shared helpers、message history CoW 评估、guardrail/team pattern examples） |
| hotfix 06-17 | 重构 | LLM Catalog 信息扩展（`Modality` / `ModelScene` / `ThinkingSpec`，`LlmModelEntry` 加 6 字段，18 条 model 数据填实，修复 DeepSeek `supports_thinking()` bug） |
| v0.9.10 | 卫星 | Minimax 多模态 Provider 接入（多模态 ContentBlock 地基、Minimax LLM/TTS/Video/Voice/Music adapter，5 个 issue 全部落地） |
| v0.9.11 | 卫星 | Omni Realtime Provider Evidence（Volcengine realtime 全双工 session、事件映射、barge-in/close/error 语义、live validation、provider-unification evidence） |
| v0.9.12 | 重构 | Provider 统一（两维重组 + registry/umbrella 墙：`orchest-protocol` 脊柱 + `orchest-provider-core` + http/stream/visual 三层 impl crate + `orchest-provider` 墙；omni + Chameleon 双标尺；realtime/asr/tts/aigc 四个模态 crate 吸收，`agent-runtime-{model,providers}` shell 移除；node/py 只经 protocol + 墙；`features=["llm"]` 无 tungstenite/OSS；8 个 issue 全部落地） |
| v0.9.13 | 重构 | `core/node/py` 改名收尾（ADR-0001 Decision 2 的延后项）：`agent-runtime-core` → `orchest-runtime`，`agent-runtime-py` → `orchest-py`，`agent-runtime-node` → `orchest-node`；Python 包 `agent_runtime` → `orchest`，npm 包 `@orchest/agent-runtime` → `@orchest/sdk`；crate 目录、workspace members、跨 crate 依赖、示例、guide 文档同步更新 |

## 迭代编号约定

- **主线迭代**（v0.7、v0.8、v0.9）：runtime 核心能力演进，有依赖链
- **卫星迭代**（v0.6.1、v0.8.1 ...）：与主线并行或从已完成主线切出的独立模块（易用性工具、扩展 crate 等）。独立 crate，不阻塞主线，按就绪时间合入

## 规划中

### v0.10 — Demo A: 能力组合广度验证（规划）

两轮 demo 验证策略的第一轮，**广度优先**：用一个真实但小的完整产品 dogfood SDK，验证运行时各能力在产品场景下的协同。广度覆盖两个轴——编排控制面（model/tool/approval/event/session/sub-agent）与**模态网关**（ASR/TTS/多模态图像输入）。v0.10 用 demo 证据决定哪些 API / 文档 / runtime 问题必须在公开发布前修。

当前产品形态锁定为 **Briefing Desk**：本地**多媒体**研究简报 agent。它读取一组混合材料（Markdown/text 笔记 + 图表截图 + 录音访谈），转写音频（ASR）、读图（多模态图像输入）、检索文本，围绕用户问题生成报告，展示事件流，写文件前走 approval，可选合成简报音频版（TTS），并支持 session resume。模态不是为凑覆盖率而堆——研究简报工具本就自然地吸收访谈录音与图表。AIGC 生图为可选 stretch，视频/音乐与音频直喂 LLM 不在范围内。详见 [`v0_10/prd.md`](./v0_10/prd.md)。

v0.10 至少验证一个轻量 reviewer sub-agent 或 handoff 路径（Agent-as-Tool 或 Handoff）。v0.10 验证报告中的 Supervised Delegation 摩擦点会直接移交 v0.11。

**依赖**：v0.9.2 文档

### v0.11 — Demo B: Supervised Delegation 深度验证（规划）

两轮 demo 验证策略的第二轮，**深度优先**：专门验证 Supervised Delegation API 面——即 Multivac M2 avatar 所依赖的 Orchest seam。

产品形态为 **Research Pipeline**：两层委派 demo。Supervisor Orchest agent 委派任务给 Worker Orchest agent，`LlmWatcher` 挂载并实时监控 worker 事件流，通过 `InjectCmd` 注入一次 steering 修正，触发受控 fault injection 验证 supervisor recovery 路径。详见 [`v0_11/prd.md`](./v0_11/prd.md)。

需验证的 seam API：`LlmWatcher` attach/detach、`InjectCmd` / Steering、`ContextMode::Fresh | Fork`、supervisor 故障检测、worker 重启或升级、multi-watcher FIFO、completion gate。产出物是 **Seam Gap Analysis 报告**，决定哪些 API 在 v1.0 冻结前必须修改。

Worker 是普通 Orchest agent，不是 Claude Code。Claude-Code-as-tool 风格的长运行 Supervised Delegation 是 Multivac M2 产品层的验证场景，不进 v0.11。

**依赖**：v0.10 完成（验证报告中的 SD 摩擦点）、v0.9.5 Control-Flow Hardening、v0.9.4 Failure Semantics

### v1.0 — 首次公开发布（规划）

第一个公开发布到 crates.io 的版本。包含发布准备的全部内容：Cargo publish 元数据、license 定稿、release workflow、CHANGELOG、版本号策略文档。

**依赖**：v0.11 Demo B 完成（Supervised Delegation API 经产品验证后才冻结公开 API）

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
✅ v0.9.4: Failure Semantics
            │
            ▼
✅ v0.9.5: Control-Flow Hardening
            │
            ▼
✅ v0.9.6: ASR Follow-up Providers
            │
            ▼
✅ v0.9.7: Tool Surface Extensions
            │
            ▼
✅ v0.9.8: Runtime Safety + Observability
            │
            ▼
✅ v0.9.9: API Cleanup + Product Patterns
            │
            ▼
   v0.10: Demo A — 能力组合广度验证（Briefing Desk）
            │
            ▼
   v0.11: Demo B — Supervised Delegation 深度验证（Research Pipeline）
            │
            ▼
   v1.0: 首次公开发布（crates.io + release workflow + license 定稿）

✅ v0.9.1: ASR Provider Gateway（卫星，已完成）
✅ v0.9.3: TTS Provider Gateway（卫星，已完成）
✅ v0.9.10: Minimax 多模态 Provider 接入（卫星，已完成）
            │
            ├──────▶ v0.10 Demo A 消费（ASR/TTS/多模态图像 → 模态广度验证）
            ▼
✅ v0.9.11: Omni Realtime Provider Evidence
            │
            ▼
✅ v0.9.12: Provider 统一（重构，已完成）
        （orchest-protocol 脊柱 + http/stream/visual 三层 + orchest-provider 墙；
         omni + Chameleon 双标尺；四个模态 crate 吸收 → docs/iteration/v0_9_12/）
            │
            ▼
✅ v0.9.13: core/node/py 改名收尾（重构，已完成）
        （agent-runtime-{core,py,node} → orchest-{runtime,py,node}；
         python 包 agent_runtime → orchest；npm 包 → @orchest/sdk；ADR-0001 revisit 项关闭）
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
