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
| v0.9.13 | 重构 | `core/node/py` 改名收尾（ADR-0001 Decision 2 的延后项）：`agent-runtime-core` → `orchest`，`agent-runtime-py` → `orchest-py`，`agent-runtime-node` → `orchest-node`；Python 包 `agent_runtime` → `orchest`，npm 包 `@orchest/agent-runtime` → `@orchest/sdk`；crate 目录、workspace members、跨 crate 依赖、示例、guide 文档同步更新 |
| v0.10 | 功能 | Demo A: Briefing Desk 能力组合广度验证（本地多媒体研究简报 agent；search/read/write 工具 + approval、real ASR/TTS gateway（`FakeAsr`/`FakeTts` 从零补全）、Agent-as-Tool reviewer sub-agent、跨进程 session persist + resume；6 个 issue 全部落地；[验证报告](../review/v0_10_demo_validation.md)产出 5 项 release blocker，均已建独立追踪 issue（[#195](https://github.com/jianantian/orchest/issues/195)–[#199](https://github.com/jianantian/orchest/issues/199)），最大发现：多模态图片输入当前无公开 API 可走） |
| hotfix 2026-07-02 | 重构 | v0.10 验证报告 release blocker 清偿（[#195](https://github.com/jianantian/orchest/issues/195)–[#199](https://github.com/jianantian/orchest/issues/199) 全部关闭）：新增 `RunInput` 打通多模态图片输入公开入口、`AgentRun::resume_with_input` 支持带新问题续会话、resume 对「曾持久化但 session_store 缺失」响亮报错（`ConfigError::SessionStoreMissing`）、`SubAgentBuilder::build()` 改 `Result`、`orchest-provider` 新增可复用 `fakes::{FakeAsr, FakeTts}`（`testing` feature）；Briefing Desk demo 全量重跑（`cargo test -p briefing-desk-demo` 20/20 + 手动 `--fake` run/resume 全流程），[验证报告](../review/v0_10_demo_validation.md)与本表同步更新；live provider 验证仍未做（无凭证环境），v1.0 前必须补 |
| hotfix 2026-07-12 | 重构 | ADR-0002 Phase 1–2（非破坏）：六个 provider 全走 protocol factory + `ProviderProfile` 抽取、catalog 作为能力事实来源、`provider/[protocol/]model` 语法、Elss 溶解为纯 `ProviderEntry`；10 个 slice 全部落地（[#204](https://github.com/jianantian/orchest/pull/204)） |
| v0.12 | 重构 | ADR-0002 Phase 3（破坏性收尾）：删除 legacy `ProviderFactory` trait 与 per-provider `*Factory`/`*Adapter` 结构，四个 Chat adapter 收敛为共享 `ChatAdapter`、两个 Messages adapter 收敛为共享 `MessagesAdapter`，provider 差异降为 `ProviderProfile` 数据（`option_support` 数据化 Strict/degrade、`capabilities`、`chat_sse_reasoning`、fallible `replay_reasoning`、canonical stop-reason 映射 + Messages 四个 hook：`messages_wire_role`/`encode_multimodal_block`/`messages_supports_adaptive`/`messages_auth_headers`）；registry 只存 `ProviderEntry`；每 provider 请求/响应逐字节保持，完整 per-provider 测试套件迁移到共享核；公开面收窄 + v1.0 迁移说明（[migration-notes](../archive/iteration/v0_12/migration-notes.md)）；3 个 slice 全部落地（[#205](https://github.com/jianantian/orchest/pull/205) → hotfix，[#206](https://github.com/jianantian/orchest/pull/206) → main） |
| hotfix 2026-07-18 | 重构 | ASR 流式方言 SegmentRef 填充：五个 WS ASR 方言的 `Transcript`/`EndOfSpeech` 事件从原生协议填充段标识（volcengine 状态 diff + utterance `start_time`、deepgram `start`、aliyun `begin_time`/计数器兜底、soniox `final`/`tail` 合成段、elevenlabs 计数器），volcengine 顺带消除 `result_type:"single"` 全量重发噪音；跨方言 Snapshot/Committed 同 id 契约写入 `orchest-protocol` 文档；[#210](https://github.com/jianantian/orchest/issues/210)–[#212](https://github.com/jianantian/orchest/issues/212) 关闭（[#213](https://github.com/jianantian/orchest/pull/213)） |
| hotfix 2026-07-18b | 重构 | Provider 请求正确性 + run 终止语义（music-gift 质量梳理发现）：`cache_control` 从请求顶层移到 content block 级（system 转 block 数组挂末 block，无 system 挂最后一条消息的末可缓存 block、跳过 thinking 块）；非 adaptive 模型 thinking budget ≥ max_tokens 时抬升至 budget+4096 并记 `OptionAdjustment`（修复默认配置 400）；异常 stop_reason 无 tool_use 直接 `RunFailed`（不再 push 空 User 消息循环至 max_steps）；[#214](https://github.com/jianantian/orchest/issues/214)–[#216](https://github.com/jianantian/orchest/issues/216) 关闭（[#217](https://github.com/jianantian/orchest/pull/217)） |
| v0.13 | 重构 | 生成质量地基（music-gift 质量梳理）：`AgentRun::start_with_messages` 公开多轮启动入口（Py/Node 透传 `messages`）；`RuntimeEvent::RunCompleted` 新增 `stop_reason` 区分截断完成；catalog `context_window` 经 adapter capabilities 在 `pre_start` 回填 `ModelSpec`（调用前硬校验自此生效）；`RetryClass::StreamInterrupted` + `RetryPolicy::recommended()` + 绑定 `retry=True` 一行开启重试；[#219](https://github.com/jianantian/orchest/issues/219)–[#222](https://github.com/jianantian/orchest/issues/222) 关闭（[#223](https://github.com/jianantian/orchest/pull/223)） |
| v0.14 | 重构 | Skill 机制（polaris「渐进式披露作为一等抽象」落地）：scanner 错误上报 `ScanOutcome` + `SkillLoadWarning`、frontmatter 行级解析；**零配置渐进式披露**（`<available_skills>` 元数据注入 + 内置 `load_skill` 三级加载、零 CWD 依赖）；注册容错（单 skill 失败跳过+警告、strict 开关、重名先注册者胜）；标准兼容（allowed-tools 双拼写、name/description 校验）+ env 路径注入防护；死字段裁定（声明预留不 enforce、`SkillDependencyError` 移除）；[#225](https://github.com/jianantian/orchest/issues/225)–[#229](https://github.com/jianantian/orchest/issues/229) 关闭（[#230](https://github.com/jianantian/orchest/pull/230)） |

## 迭代编号约定

- **主线迭代**（v0.7、v0.8、v0.9）：runtime 核心能力演进，有依赖链
- **卫星迭代**（v0.6.1、v0.8.1 ...）：与主线并行或从已完成主线切出的独立模块（易用性工具、扩展 crate 等）。独立 crate，不阻塞主线，按就绪时间合入

## 规划中

### v0.11 — Demo B: Supervised Delegation 深度验证（规划）

两轮 demo 验证策略的第二轮，**深度优先**：专门验证 Supervised Delegation API 面——即 Multivac M2 avatar 所依赖的 Orchest seam。

产品形态为 **Research Pipeline**：两层委派 demo。Supervisor Orchest agent 委派任务给 Worker Orchest agent，`LlmWatcher` 挂载并实时监控 worker 事件流，通过 `InjectCmd` 注入一次 steering 修正，触发受控 fault injection 验证 supervisor recovery 路径。详见 [`v0_11/prd.md`](./v0_11/prd.md)。

需验证的 seam API：`LlmWatcher` attach/detach、`InjectCmd` / Steering、`ContextMode::Fresh | Fork`、supervisor 故障检测、worker 重启或升级、multi-watcher FIFO、completion gate。产出物是 **Seam Gap Analysis 报告**，决定哪些 API 在 v1.0 冻结前必须修改。

Worker 是普通 Orchest agent，不是 Claude Code。Claude-Code-as-tool 风格的长运行 Supervised Delegation 是 Multivac M2 产品层的验证场景，不进 v0.11。

**依赖**：v0.10 完成（[验证报告](../review/v0_10_demo_validation.md)中的 SD 摩擦点）、v0.9.5 Control-Flow Hardening、v0.9.4 Failure Semantics

### v1.0 — 首次公开发布（规划）

第一个公开发布到 crates.io 的版本。包含发布准备的全部内容：Cargo publish 元数据、license 定稿、release workflow、CHANGELOG、版本号策略文档。

**依赖**：v0.11 Demo B 完成（Supervised Delegation API 经产品验证后才冻结公开 API）、v0.12 完成（ADR-0002 Phase 3 的 legacy adapter/factory 移除必须在冻结前落地）。发布前必须清偿两份验证报告各自的 release blocker 清单：[v0.10 Demo A 验证报告](../review/v0_10_demo_validation.md)的 5 项 release blocker **已全部由 hotfix 2026-07-02 清偿**（[#195](https://github.com/jianantian/orchest/issues/195)–[#199](https://github.com/jianantian/orchest/issues/199) 全部关闭，详见验证报告 Triage 表与本文档「已完成」表的 hotfix 2026-07-02 行），与 v0.11 Demo B 的 Seam Gap Analysis 报告（届时补链接）。**v0.10 报告的 live provider 验证仍未完成**（hotfix 2026-07-02 所在环境无 LLM/ASR/TTS 凭证）——未完成前，不得仅凭该报告推进 v1.0。

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
   v0.12: ADR-0002 Phase 3 — legacy adapter/factory 移除（重构，v1.0 冻结前）
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
         omni + Chameleon 双标尺；四个模态 crate 吸收 → docs/archive/iteration/v0_9_12/）
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
| 多模态图片输入（`ContentBlock::Image` 经 agent loop） | ~~v0.10 demo 验证发现无公开 API 可走~~ → **hotfix 2026-07-02 已完成**：新增 `RunInput` 类型，`AgentRun::start(config, RunInput, ..)`；`RunInput::text(..)`/`.with_image(..)`/`.from_blocks(..)` 覆盖纯文本与多模态；demo `describe_image` 工具驱动真实 `ContentBlock::Image` → `ModelAdapter::complete()` 调用（[#195](https://github.com/jianantian/orchest/issues/195)） | ✅ |
