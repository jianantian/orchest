# Agent Runtime 全景：持久化、权限、委派、取消

> 日期：2026-10-22
> 范围：`~/Github/01-agent-core`（23 个仓库）与 `~/Github/02-agent-ui`（28 个仓库），共 51 个。
> 方法：11 个只读子 agent 分组调研，固定 schema（定位 / agent loop / 持久化 / 崩溃恢复 / 权限审批 / 委派 / 取消 / 嵌入 / 可借鉴点），全部结论带 `path:line` 证据；官方文档用于补足闭源实现（Claude Code 引擎闭源，本地仓库只有插件与类型面）。
> 与本文的关系：这是**证据**，不是决定。已定的决策记在 [`../../iteration/direction.md`](../../iteration/direction.md)。

---

## 0. 一句话结论

业界已经收敛出三件事：**会话是 append-only 事件日志、消息历史是派生投影、崩溃后的进行中副作用必须显式标记为 unknown**。Orchest 目前是「全量快照 + `serde(skip)`」，是样本里唯一的异常形态。

---

## 1. 会话与执行态持久化：四种谱系

| 谱系 | 代表 | 形态 | 派生关系 |
|---|---|---|---|
| **A. 事件日志 + 派生投影** | dsh、Codex、LangGraph、openhuman、prime-agent、Claude Code、Pi、Kocoro | append-only JSONL / stream 集合 | 消息历史由日志折叠派生，从不单独存 |
| **B. 快照 + 增量 journal** | jcode | 全量 JSON 快照 + 追加 journal（512KB 阈值转快照） | 按消息 id 幂等去重回放；抢救粘连行、跳过坏行 |
| **C. 状态库 + 消息表** | monocode（SQLite snapshot + append-only events 表）、craft（JSONL header+messages）、moumantai（Drizzle） | 关系表 | 从表派生 |
| **D. 纯快照/消息行** | OpenAI Agents Python（`SQLiteSession` 存 JSON 消息行）、deepagents（全部委托 LangGraph checkpointer）、Pi 的持久层 | — | 无事件溯源 |

关键实现细节：

- **dsh**：`Session` 是 append-only `SessionEvent` 日志，消息历史由 `deriveMessages()` 派生（`packages/core/session/src/index.ts:984`）；`SessionHeader`（version/cwd/parentSession/isSeeded/delegationDepth/agentPreset）与日志分开存；单写者 handle（`open(id,'write')` 冲突抛 `SessionAlreadyOwnedError`）；`flush()` 是唯一承诺耐久的屏障，append 本身是 best-effort。
- **Codex**：JSONL rollout 每 **item** 追加（`codex-rs/rollout/src/recorder.rs:137-150`，`RolloutCmd::AddItems/Flush/Persist`）+ 多张 SQLite 状态库（`state/src/sqlite.rs:41-46`）；`resume` 与 `projection` 共用同一 decoder（`rollout/src/lib.rs:50-53`）。
- **LangGraph**：每个 super-step 一个 checkpoint，`CheckpointMetadata.source ∈ {input, loop, update, fork}`，`channel_versions` 做版本化状态（`libs/checkpoint/langgraph/checkpoint/base/__init__.py:40-70`）。
- **jcode**：`save_inner` 平时只追加 journal entry，journal 超 `MAX_SESSION_JOURNAL_BYTES=512KB` 或 metadata 变化才重写全量快照（`crates/jcode-base/src/session/persistence.rs:723-800`）。
- **Claude Code**：每会话一个 JSONL 转录，每行一条记录，`parentUuid` 链 + `isSidechain` 派生历史；**转录是唯一事实源，文件快照是派生层**（`claude-agent-sdk-python/.../session_import.py:5`、`sessions.py:1293`）。
- **openhuman**：stream 集合（`harness.journal`、`ledger.events`、`conversations.messages`）+ collection（`graph.checkpoints`、`ledger.tool_effects`、`harness.run_status`）（`crates/openhuman-core/src/storage/README.md`、`docs/storage-schema.md`）。
- **Pi**：`SessionManager` 追加式 JSONL 事件树（`packages/coding-agent/src/core/session-manager.ts:987`）。

**对 Orchest 的含义**：当前 `SessionSnapshot`（全量 messages + step + budget + `active_config`，且 hooks/retry/handoffs/store 被 `#[serde(skip)]`，`crates/orchest/src/run/config.rs:134-141`）是谱系 D 的极端形态——它既不能表达"进行中的副作用"，也不能在恢复后重建 hook 行为。

---

## 2. checkpoint 粒度

| 粒度 | 代表 | 证据 |
|---|---|---|
| 每 item / 每消息 | Codex（每 item 追加）、Pi（每条记录）、craft（逐消息 + debounce 队列） | `rollout/src/recorder.rs:137-150`；`packages/shared/src/sessions/storage.ts:92-98` |
| 每 step / 每 super-step | LangGraph（super-step）、openhuman（graph 节点级）、jcode（每消息/每工具结果后 `session.save()`） | `base/__init__.py:40-70`；`crates/jcode-app-core/src/agent/turn_loops.rs:959-961` |
| **语义边界**（模型请求前 / 顶层工具副作用前 / step 前） | dsh（`dsh-session-checkpoint-policy`） | `packages/session/session-checkpoint-policy/src/index.ts:46-66` |
| 每工具批次后 / 压缩后 / ForceStop 前 | Kocoro | `internal/agent/loop.go:1072` |
| 每 turn（一个 prompt） | Claude Code（转录每 turn 一条；文件快照每 prompt 一个，保留最近 100 个） | 官方 checkpointing 文档 |
| 仅结束/出错 | **Orchest 现状** | `crates/orchest/src/session/persistence_hook.rs` |

**决策 1 = A（每 step 写）落在主流区间内**；dsh 的"语义边界"更保守：它额外在**工具副作用派发前**落一个标记。若 Orchest 只在 step 结束写，"工具已执行、结果未落盘"的窗口无法判定——这是建议把 checkpoint 细化到"每 step + 每次工具副作用派发前"的理由。

---

## 3. 进行中副作用：崩溃后怎么办

这是本次调研里最一致、也最能拉开差距的一维。

### 第一档：显式 unknown 语义（不重跑、不丢弃、显式标记）

- **dsh**：崩溃留下未闭合的 `turn/start`；持久层**不修复**，由 agent-loop 的 `interruptedTurnClosers` 合成闭包事件——已记录无结果的调用补 `TOOL_OUTCOME_UNKNOWN`，未记录开始的调用补 `TOOL_NOT_STARTED`，并给模型明确文案"只对只读/幂等操作重试，否则先查外部状态"（`packages/core/session/src/repair.ts:27-35`）。
- **Kocoro**：副作用台账 `prepared → dispatching → committed / outcome-unknown`；崩溃后注入合成 tool_result「未重试，请先人工核查外部系统」，**绝不自动重放**（`internal/agent/side_effect_journal.go` `guardInterruptedToolExecutions`）。
- **openhuman**：`ledger.tool_effects` 按 run 记录 status/started_ms，配合 `agent_run_status` 判定 unknown，不盲目重跑（`src/agent_runs/replay/schemas.rs`）。

### 第二档：记录但不重发（不显式建模 unknown）

- **Claude Code**：中断的工具调用在恢复时"不完成也不重跑"，模型看到它被标记为 cut off，并被告知先确认是否已生效（官方 sessions 文档；`CLAUDE_CODE_RESUME_INTERRUPTED_TURN` 可改）。
- **Codex**：未持久化的 item 不回放；已完成的 tool call 因逐 item 追加已在日志中，resume 从日志末尾继续。
- **jcode / prime-agent**：transcript 线性，不重发不丢弃；prime-agent 的进行中 fetch 被 abort，turn 以 zero-usage aborted message 结算。

### 第三档：缺口（把幂等性甩给应用）

- **LangGraph**：node 重跑是确定性重执行，**没有内建 unknown 语义**，副作用幂等性由应用负责。
- **OpenAI Agents Python**：官方示例明言"生产需自行对工具副作用做对账后重试"（`docs/human_in_the_loop.md`）。
- **dsh-purge**（反向证据）：它靠 patch 本机 `@deepseek-ai` 包文件来剥离审批门与沙箱——**纯进程内强制点不构成真实安全边界**。

**对 Orchest 的含义**：Pi 没有这一层；Orchest 若补上"in-flight 副作用账本 + 合成闭包事件"，相对 Pi/Codex 就是**结构性差异**，而不是功能对齐。

---

## 4. 权限与审批

### 判定粒度谱系

| 粒度 | 代表 | 证据 |
|---|---|---|
| 工具级 | dsh（`toolName`，无参数/路径）、craft（命令级审批绑定） | `packages/interaction/user-approval/src/index.ts:260-262` |
| 工具级 + 命令模式级 | Codex execpolicy（`pattern=[...]`、`host_executable(paths=[...])`）、OpenCode（`"git *"`、`"rm *"`）、percho（bash 链取最严段）、jcode（`jcode-command-risk` 爆炸半径分级）、openhuman（`CommandClass`）、Vercel open-agents | `codex-rs/execpolicy/README.md`；`opencode.ai/docs/permissions.md`；`crates/jcode-command-risk/src/gate.rs:78` |
| 再加路径级 | OpenCode `external_directory`、percho（read/write/temporary 越界）、deepagents（操作类型 × 路径模式）、jcode `paths.rs`（保护 home/root/credentials） | `middleware/filesystem.py:367-383` |
| 再加网络/凭据域 | OpenCode `webfetch` 按 URL、openhuman trusted roots + always-forbidden（`~/.ssh`、`~/.aws`） | `src/security/policy/README.md` |

### 模式集合

- **Claude Code**：6 态 —— `default / acceptEdits / bypassPermissions / plan / dontAsk / auto`（`mods/types/claude-code.d.ts:6103-6105`）；`sec-default` mod 保证 **deny 规则连用户插件都无法解除**（`mods/sec-default/README.md`）。
- **OpenCode**：`allow / ask / deny` 三态 + 模式匹配（最后命中规则胜出）+ `--auto` 模式（显式 deny 仍生效）+ `external_directory` 与 `doom_loop`（同调用重复 3 次）守卫。
- **Codex**：沙箱 4 态（`DangerFullAccess | ReadOnly | WorkspaceWrite | ExternalSandbox`）+ 审批策略 4 态（`UnlessTrusted | OnRequest | Granular | Never`）（`protocol/src/protocol.rs:1097-1210`）。
- **dsh**：`ask | never` + 结果 `allowed-once | rejected | cancelled | unavailable`，fail-closed。
- **deepagents**：`allow | deny | interrupt`（`interrupt` 可与 HITL 组合）。
- **percho**：allow/ask/deny × 通配模式，bash 链取最严段。

### 强制点

| 强制点 | 代表 | 证据 |
|---|---|---|
| **OS 沙箱** | Codex（macOS Seatbelt `sandbox-exec`、Linux landlock/bwrap、Windows 受限令牌）、Claude Code（bwrap，`dangerouslyDisableSandbox`）、openhuman（`Local(jail/Seatbelt)` / `Docker`） | `codex-rs/core/README.md`；`sandboxing/src/landlock.rs`；`src/sandbox/types.rs` |
| 纯进程内 | dsh、jcode（自述 "defense in depth, not a sandbox"）、percho、craft、Kocoro、OpenCode | `crates/jcode-command-risk/src/assess.rs` |

**共识**：read-only 的**硬保证全部来自 OS 沙箱**，不是工具分类。Codex 的 `read-only` 是 seatbelt/landlock 级别的。凡是只做工具分类的系统（jcode、dsh、OpenCode），其 read-only 都只能算 soft。

### 批准作用域

一次性（多数）/ 会话级（Codex `ApprovalStore` 按 key 缓存；OpenCode "always" 仅当前会话；percho 会话内 always-allow）/ 持久化（Claude Code 的 settings 层、openhuman standing grants、craft 带 TTL）。

---

## 5. 委派与子 agent

| 形态 | 代表 | 持久性 | 监督 |
|---|---|---|---|
| 进程内子会话 | Claude Code（`agent.spawn`，`isolation: worktree|remote`，默认后台）、dsh、openhuman、Kocoro、craft、deepagents | dsh 子会话 session-backed 可 cold-resume；openhuman durable session/checkpoint | dsh `interrupt()` + `maxActiveSubagents` + `delegationDepth` **持久于 header**（重启不重置） |
| 进程外 worker | prime-agent（`rlm.spawn` 每子 agent 一进程，admission≠completion 句柄）、dsh out-of-process ACP | prime-agent 台账持久 + orphan 进程组回收 | prime-agent `list_subagents/collect/delete`；openhuman `subagent_cancel/subagent_steer` |
| 进程内 headless session | jcode swarm（仅 root 可 spawn，深度 0/1） | journal 持久 | 子树停止权限（`force=true` 才可停外部） |
| 生态扩展 | pi-subagents（depth 2 / 64 spawn / 20 并发 / worktree 隔离） | 文件 artifact + mission/schedule | FleetView / steer / stop |

**Orchest 现状**：`AgentAsTool` + `ChildRunRegistry` + `run_depth` 已有；缺 depth 持久化、后台执行、监督/停止接口、并发上限。

---

## 6. 取消与中断

- **共识：协作式 token + 在明确边界生效 + 进程组 kill 兜底。**
  - Claude Code：`AbortSignal` + `$.turn.abort({turnId})`；运行中的命令**不杀**，转后台并标 `interrupted`（`mods/types/claude-code.d.ts:12952`）。
  - dsh：`agent.cancel()` abort 每个 step 的 `AbortController`；未派发的调用记 `TOOL_ABORTED_BEFORE_DISPATCH`；模型流中断时 `assistant/message` 记 `interrupted: true`。
  - Codex / prime-agent / openhuman / jcode：分别是 interrupt、`AbortSignal` + 每个 await 点检查、`cancellation_handle` + 杀进程组、`InterruptSignal`（含 epoch 防丢取消）。
  - jcode：bash 超时**不杀**，升级为后台任务；显式 timeout 才 SIGKILL 进程组。
- **成熟模型：取消有硬边界。** Kocoro 的 `InterruptFilteredContext` 让安全工具（read/grep）可中断、危险工具（bash/write）跑完；moumantai 让进行中工具完成（副作用已提交）、只丢弃缓冲文本。

**对 Orchest 的含义**：当前 abort 只在下一个消息边界生效、审批等待最长 1 小时，与上述全部实现差距明显；且 Orchest 需要一个"取消期间副作用如何记账"的语义（承接第 3 节）。

---

## 7. 打包与生态分发

| 系统 | 包格式 | 分发 |
|---|---|---|
| **Claude Code** | 目录 + `.claude-plugin/plugin.json`（`name`/`version`/`description`/`author`）+ 可选 `commands/` `agents/` `skills/` `hooks/` `.mcp.json` | marketplace（`/plugin install name@marketplace`）或项目 `.claude/settings.json` |
| **Pi** | npm 包（`pi install npm:...`） | npm |
| **dsh** | pnpm/npm 包 + Cordis 插件 + profile 层叠（`dsh` 是唯一 launcher） | npm / git |
| **Codex / OpenCode / dsh** | MCP 作为跨语言工具通道 | — |

**结论**：业界没有为"agent 扩展"自建代码托管，而是「语言包管理器 + 目录约定 + marketplace 式索引」。这直接支持决策 5 的建议 B：**Orchest 提供 bundle manifest，不自建 registry**。

---

## 8. 宿主层（谁坐在 runtime 之上）

- **cindy**（`packages/maker-core`）：把 Claude/Codex/Pi 抽象成统一 Session/turn/abort/权限接口，持久化由 host 注入；能力探测 + 显式 `NotSupportedError`；6 态权限模式映射到各 CLI 旗标。**是 Orchest 最该对标的宿主层参考**。
- **agentsview**：只读旁路观察者——外部 JSONL 源 → 本地 SQLite 投影 → additive-only 稳定只读 API；检查点完整性用内容寻址 spool + outbox/ack。
- **moumantai**："tools mutate / faces read / server validates"——server 校验并执行所有工具；`clientMsgId` 幂等。
- **orca / monocode / multica / craft / Proma / ZCode**：宿主通过 NDJSON stdio / RPC / PTY 驱动外部 CLI，会话与事件持久化在宿主侧。
- **openhuman**：`openhuman-embed` 单进程单 Runtime（第二个 build 返回 `AlreadyRunning`），多租户用 `ProfileRuntime`；无 sidecar。

**对 Orchest 的含义**：作为 runtime，Orchest 应提供**能力探测 + 显式不支持**的面（而不是让宿主猜），这是所有上述宿主的共同诉求。

---

## 9. 对 Orchest 的可执行结论

| # | 结论 | 依据 | 落点 |
|---|---|---|---|
| 1 | **会话模型从「全量快照」改为「append-only 事件日志 + 派生投影」** | dsh / Codex / LangGraph / openhuman / jcode / Claude Code 一致 | M1（决策 1 = B 的实现前提） |
| 2 | **checkpoint 粒度 = 每 step，且在工具副作用派发前额外落标记** | dsh 语义边界；决策 1 = A | M1 |
| 3 | **新增 in-flight 副作用账本 + 崩溃时的合成闭包事件**（unknown / not-started / aborted） | dsh `repair.ts`、Kocoro 台账、openhuman `ledger.tool_effects` | M1；相对 Pi 的差异化点 |
| 4 | **提供原生权限模式表面**：工具级 + 命令模式级 + 路径级 + fail-closed 可配 + 批准作用域（一次性 / 会话级）+ 沙箱后端注入点 | Codex execpolicy、OpenCode、percho、jcode、Claude Code 6 态 | M1（只读强度由宿主注入的沙箱决定，见 §4） |
| 5 | **取消语义**：token 贯穿模型流 / 工具 / 审批等待；明确"安全工具可中断、危险工具跑完"的边界；进程组 kill 作为可选后端 | Claude Code、dsh、Kocoro、jcode、moumantai | M1 |
| 6 | **委派补齐**：depth 持久化（重启不重置）、后台执行、监督/停止接口、并发上限；core 只做原语，持久委派放生态包 | dsh（header 持久 depth）、prime-agent、openhuman、pi-subagents | M1（depth 持久）/ M3（生态包） |
| 7 | **打包**：提供 bundle manifest（name/version/description + skills/agents/prompts/mcp 目录），随语言包管理器分发，索引靠社区 | Claude Code plugin、Pi npm、dsh npm | M3（决策 5） |
| 8 | **能力探测面**：runtime 明确声明支持/不支持（模型切换、恢复、取消、审批、委派），宿主据此分支 | cindy `NotSupportedError`、openhuman `AlreadyRunning`、agentsview additive-only | M1（与 §4.1 边界配套） |

---

## 10. 仓库清单（51）

| 目录 | 类型 | 一句话 |
|---|---|---|
| 01/pi | coding agent | 产品优先 TS harness，JSONL 会话树，无内置权限，工具默认并行 |
| 01/pi-subagents | 扩展 | Pi 之上的委派层：子会话、workflow、mission、schedule、worktree 隔离 |
| 01/claude-code | 产品 | 闭源引擎 + 开源插件/类型面；6 态权限、deny 规则不可被插件解除、plugin manifest + marketplace |
| 01/codex | 产品 | Rust core；每 item rollout 日志 + SQLite 状态库；execpolicy + OS 沙箱；exec resume/fork |
| 02/deepseek-harness | runtime SDK | append-only 事件日志 + 派生投影 + flush 屏障 + 合成闭包修复（最佳参照） |
| 01/jcode | 产品 | 快照 + journal 增量持久化；进程内命令风险门（自述非沙箱）；swarm 单层扇出 |
| 02/prime-agent | 产品 | Rust TUI + daemon + Python RLM；保留式子 agent（admission≠completion）；无审批/沙箱 |
| 01/better-harness | 库 | "Harness as Code" DSL，委托宿主 SDK 会话；compare 层有 deny-by-default 工具权限 |
| 01/harness | — | revfactory/harness，仓库无源码语言统计，需单独确认 |
| 01/celld | 基础设施 | Cloudflare Workers 兼容的有状态分布式运行时；无 agent loop |
| 01/openhuman | harness | 桌面 + Rust 库；vendored tinyagents 提供 loop/会话/checkpoint；SandboxBackend 可选 Seatbelt/Docker |
| 02/monocode | 宿主 GUI | 多 CLI 宿主；SQLite snapshot + append-only events + receipts |
| 02/omp-desktop | 宿主 | Rust + JS，跑 OMP；本次未深挖 |
| 02/termany | 宿主 | 本次未深挖 |
| 01/langgraph | 框架 | 每 super-step checkpoint + interrupt HITL；副作用幂等性甩给应用 |
| 01/deepagents | 库 | LangChain 装配层；权限三态 allow/deny/interrupt（操作 × 路径） |
| 01/openai-agents-python | SDK | `RunState` 手工序列化恢复；无自动 checkpoint；副作用对账交给用户 |
| 01/pydantic-ai | SDK | 轻量；本次未深挖 |
| 01/smolagents | 库 | 极简；本次未深挖 |
| 01/claude-agent-sdk-python | SDK | 驱动 Claude Code CLI；SessionStore 镜像转录；resume/fork/rewind_files |
| 01/craft-agents-oss | 宿主 | 委托 Claude/Pi SDK；JSONL 会话 + 审批 broker（TTL）+ wasInterrupted 注入 |
| 01/open-agents (vercel) | 参考 app | agent 在 sandbox VM 外，durable workflow steps + VM 快照 |
| 01/moumantai | 产品 | server 拥有状态与工具执行；幂等 clientMsgId；取消不打断进行中工具 |
| 01/raft-source | — | 本次未深挖 |
| 01/ruflo | — | 本次未深挖 |
| 01/nimbalyst | — | 本次未深挖 |
| 01/LMCache | 基础设施 | KV cache 层，非 agent runtime |
| 01/pawin-brain-deepseek-harness | — | 12 个 TS 文件的壳仓库 |
| 02/AionUi | 宿主 GUI | 本次未深挖 |
| 02/Beav | 产品 | Rust + TS；本次未深挖 |
| 02/Eta | 客户端 | Kotlin；本次未深挖 |
| 02/Kocoro | 产品 | Go daemon；每工具批次 checkpoint + 副作用台账 + 自动 resume |
| 02/Proma | 产品 | Electron，委托 Pi SDK 0.87.1；JSONL 会话 + file checkpointing |
| 02/ZCode | 产品 | 宿主 stdio 协议驱动独立 agent 进程；git refs 做文件 checkpoint |
| 02/agency-agents | — | 4 个 Python 文件 |
| 02/agentsview | 工具 | 只读会话归档/审计；SQLite 投影 + additive-only API |
| 02/cindy | 宿主 | 多 harness 客户端；统一 Session/turn/abort/权限接口；能力探测 |
| 02/dsh-purge | 红队工具 | patch 本机 dsh 包剥离审批门/沙箱；反证进程内强制点不可信 |
| 02/frakio-work | 宿主 | 委托 Hermes 运行时；幂等 key + 收据 + SSE afterCursor 重放 |
| 02/happy | 客户端 | 本次未深挖 |
| 02/heard | — | 本次未深挖 |
| 02/herdr | 工具 | Rust + Python；本次未深挖 |
| 02/loopx | 产品 | 本次未深挖 |
| 02/multica | 产品 | Postgres 任务队列 + 26 个 CLI 适配器；runtime recovery 失败分类器 |
| 02/orca | 宿主 | 桌面编排器；PTY + JSONL-RPC 混合驱动 + worktree fan-out |
| 02/percho | 宿主 GUI | 内嵌 Pi SDK；逐工具规则引擎（allow/ask/deny × 模式） |
| 02/superset | 产品 | 本次未深挖 |
| 02/t3code | 产品 | 本次未深挖 |
| 02/tutti | 产品 | 本次未深挖（Multivac 已有研究笔记） |
| 02/vibeyard | — | 本次未深挖 |

---

## 11. 限制

- 全部结论来自**静态阅读**，未运行任何被测系统；行为描述以源码与官方文档为准。
- Claude Code 引擎闭源：本地仓库只有插件与 `mods/types/claude-code.d.ts` 类型面，loop 与持久化实现只能依据官方文档与 SDK 侧证据。
- "未深挖"仓库仅完成定位级覆盖，其持久化/权限细节可能与表中其他行不同。
- 各仓库在调研时点的实现可能已变化；本文件不替代各自的源码。
