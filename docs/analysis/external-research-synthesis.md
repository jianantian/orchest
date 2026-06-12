# 外部研究对 Multivac 架构的综合启发

调研日期：2026-06-11
调研对象：
- `docs/analysis/create-squad-salvage.md` — 旧产品 Create Squad 40+ 设计精华提取
- `docs/analysis/feishu.md` — 飞书原生 Multi-Agent 的战略威胁与非对称价值
- `docs/analysis/lark-channel-bridge 优秀设计总结.md` — 飞书 bridge 的设计范式
- `~/GitHub/agent/MIMO-code` — OpenCode fork，10M+ 下载，terminal-native coding agent

结论定位：不是对各个调研对象的独立总结，而是「这些外部研究对 Multivac 架构文档的启发」的整合。


## 1. 四份文档的统一框架

```
feishu.md                    战略层：我们到底在做什么产品
    ↓
create-squad-salvage.md      遗产层：旧产品哪些设计值得带走
    ↓
lark-channel-bridge 总结     范式层：同类产品把什么做对了、怎么做的
    ↓
MIMO-code                    竞争层：当前增长最快的开源竞品怎么做的
    ↓
multivac-reconstruction      实现层：我们的架构
multivac-frontend-design     实现层：我们的前端
```

四个外部来源共同指向一个 Multivac 两份文档尚未显式声明的核心命题：

> **Multivac 的产品本体是 execution control plane。Session 只是 task/runtime 的一个视图，外部 IM 只是 distribution surface。所有架构决策都应该从这个定位推导，而不是反向被「聊天 UI」或「IM 接入」定义。**


## 2. 对 `multivac-reconstruction-analysis.md` 的修改建议

### 2.1 P0 — 定位声明（feishu.md §1, §6）

在重建文档开头增加：

> Multivac 的产品本体是 execution control plane。它的核心价值在 task（执行真相）、runtime（执行宿主）、workspace（执行环境）、artifact（执行产物）、execution transparency（执行透明度）——不在「有几个 agent」或「聊天界面长什么样」。外部 IM（飞书/Slack）是 distribution surface，不是产品定义。

### 2.2 P0 — Sub-agent Spawn 契约（MIMO actor/spawn.ts）

当前 `RuntimeBackend::start_task()` 的参数 `StartAgentTask` 应吸收以下字段：

```rust
pub struct StartAgentTask {
    pub task_id: TaskId,
    pub agent_config: AgentConfig,
    pub prompt: String,

    // 新增 ↓
    pub context_mode: ContextMode,    // Full | Diff | TaskOnly
    pub output_format: Option<OutputFormat>,  // 结构化输出
    pub lifecycle: AgentLifecycle,    // Ephemeral | Persistent
    pub background: bool,             // true → inbox 通知，不阻塞
    pub parent_actor_id: Option<ActorId>,
    pub gate: Option<CompletionGate>, // 独立 judge model 验证
}

pub enum ContextMode {
    Full,       // 完整对话历史 + project knowledge
    Diff,       // 仅 checkpoint + project knowledge + 当前 task
    TaskOnly,   // 仅 task 描述 + project knowledge
}

pub struct CompletionGate {
    pub judge_model: ModelRef,
    pub criteria: Vec<String>,
}
```

其中 ContextMode 解决「长对话中 sub-agent 浪费大量 token」的问题。

### 2.3 P0 — Hook ReAct 循环（MIMO forkWork while loop）

Hook trait 的 `on_run_end` 返回值应扩展：

```rust
pub enum HookDecision {
    Continue { reason: String },  // 带着新指令再跑一轮
    Stop,                          // 正常结束
    Escalate,                      // 升级到父 agent
}
```

agent loop 在 `on_run_end` 后检查 `Continue` 则注入 reason 再跑一轮，cap 在 3 次（`MAX_REACT`）。

### 2.4 P0 — SessionIdentity（lark-bridge §2.2）

在 `session/` 模块中增加：

```rust
pub struct SessionIdentity {
    pub scope_id: ScopeId,
    pub agent_id: AgentId,
    pub workspace_fingerprint: String,  // cwd + git revision + env hash
    pub policy_fingerprint: String,     // access policy + sandbox mode + resource scope
}
```

用于 resume 判定：「这是不是同一个运行上下文」——不只是同一个 chatId。不该 resume 的 session 不会因为「刚好有个旧 sessionId」而误续上。

### 2.5 P1 — Agent 工厂（MIMO agent.generate()）

在 `org/` 或 `agent/` 模块中增加：

```rust
#[async_trait]
pub trait AgentFactory: Send + Sync {
    /// LLM 驱动的 agent 生成：用户描述 → 完整 AgentConfig
    async fn generate(
        &self,
        description: &str,
        org_id: OrgId,
    ) -> Result<AgentConfig>;
}
```

返回的 `AgentConfig` 含 system_prompt + 推荐 skill + permission 默认值 + model 推荐。用户可在前端确认/修改后创建。

### 2.6 P1 — Checkpoint-Writer 子 agent（MIMO checkpoint.ts）

当前 `AuditHook` 被动在 `on_run_end` 写 transcript。应改为：

- 专门的 **checkpoint-writer 子 agent**（后台运行）
- 基于 context window 使用率决策何时写 checkpoint（不是固定间隔）
- Token-budgeted 上下文注入（按优先级注入各 section，不是全量）
- Context reconstruction：当上下文逼近上限，重建 = 最新 checkpoint + project memory + task progress + 保留的最近消息

### 2.7 P1 — Workflow/Compose 预留（MIMO workflow/）

在 `task/` 或新建 `workflow/` 模块中预留最简原语：

```rust
#[async_trait]
pub trait WorkflowRuntime: Send + Sync {
    async fn agent(&self, prompt: String, opts: AgentOpts) -> Result<AgentOutcome>;
    async fn parallel(&self, tasks: Vec<WorkflowTask>) -> Result<Vec<AgentOutcome>>;
}
```

声明式脚本（`export const meta = { … }`）和 sandbox 执行放 v0.3+，但接口现在就要预留。

### 2.8 P1 — Employee Task 承接 + Runtime Context（feishu.md §8.2）

当前 Employee 映射为静态 `AgentConfig`。需增加：

- `EmployeeTaskBinding`：Employee 当前承接了哪些 task
- `AgentConfig::with_task_context(task: &Task) -> AgentConfig`：根据 task 动态派生配置
- `EmployeeRuntime`：Employee 的活跃 runtime 列表、当前状态

### 2.9 P1 — MessageIngress trait（lark-bridge §2.3）

在 `session/` 或新增 `ingress/` 模块中预留外部 IM 入口：

```rust
#[async_trait]
pub trait MessageIngress: Send + Sync {
    async fn ingest(&self, scope: ScopeId, msg: IngressMessage) -> Result<IngestResult>;
    async fn queue_status(&self, scope: ScopeId) -> Result<QueueStatus>;
}

pub enum IngressMessage {
    Command { text: String, sender: SenderId },   // 跳过 debounce，有中断优先级
    UserMessage { text: String, sender: SenderId }, // debounce 合并
}
```

### 2.10 P2 — PTY 相关

- **Workspace deny-list**（lark-bridge §2.5a）：`PtyBackend` 初始化 workspace 时拒绝系统根、Home 根、temp 根、Desktop/Downloads、卷根
- **Sideband input**（salvage §6.1）：`PtyBackend` 预留 `sideband_write()` 方法——agent 通过独立 channel 向 PTY 注入命令，不干扰用户键盘输入

### 2.11 P2 — Knowledge Locator URI（salvage §5.1）

在 `knowledge/` 模块中定义统一 URI：

```rust
pub struct KnowledgeLocator {
    pub source: KnowledgeSource,  // File | TaskOutput | MeetingTranscript
    pub path: String,
}
// knowledge://workspace/docs/design.md
// knowledge://task/TASK_001/summary
```

### 2.12 P2 — Desktop 模式 + 可观测性（lark-bridge §2.6-2.8）

- Desktop 模式 profile 隔离目录：`~/.multivac/profiles/<name>/{config,sessions.db,workspace,logs,runtime}`
- 结构化日志：JSONL 格式、trace_id 贯穿全链路、敏感字段脱敏（token/secret/paths）

### 2.13 P2 — Dream/Distill 远期限定（MIMO dream/distill）

在知识库/技能系统的远期规划中增加两个能力：

- **Dream**：从 session traces 提取持久化知识 → 更新 project knowledge，移除过时条目
- **Distill**：发现重复的手动工作流 → 自动打包为可复用 skill/subagent/command

不在 v0，但 knowledge trait 接口不要堵死这条路。


## 3. 对 `multivac-frontend-design.md` 的修改建议

### 3.1 P0 — Turn 初始化：HTTP POST → messageId → Skeleton（salvage §4.1）

在前端文档 §5.1 和 §5.2 之间增加：

> **Turn 初始化**：发送消息是同步 HTTP POST。前端在收到 201 `{ message_id, turn_id }` 后立即在 SessionViewer 中插入 TurnCard skeleton（`phase: pending`），**不等第一个 WS 事件**。后续 WS 事件通过 `turn_id` 匹配并填充骨架。这消除了「用户发消息后 UI 空白等第一个 event」的延迟。

前端 `api/client.ts` 需同时暴露 `sendMessage()` 的 HTTP POST 方法和 WS 事件监听。

### 3.2 P0 — TurnState 中间状态模型（lark-bridge §2.4）

在 §4.4 状态管理中增加显式的 `TurnState` 中间状态：

```typescript
interface TurnState {
  turnId: string
  phase: TurnPhase
  activities: ActivityRowState[]
  responseText: string
  subTasks: SubTaskState[]
  permissions: PendingPermission[]
}

// 纯函数：event → TurnState
function reduceTurnState(prev: TurnState, event: TurnEvent): TurnState
```

好处：可重放、可测试、边缘状态不散落在 if/else 里。

### 3.3 P1 — 执行透明度信任界面（feishu.md §4.4）

在 §3.2 TurnCard 结构说明中增加：

> TurnCard 是 Multivac 执行透明度的用户界面。不是「美化 agent 输出」，而是让用户看到 agent 的真实执行过程：每一步调了什么、结果如何、中间失败了几次、为什么需要确认。这是 Multivac 区别于「黑盒 agent 聊天」的根本 UX 差异。

### 3.4 P1 — `turn:completed` Commit 边界（salvage §4.2 + MIMO gate）

在 §4.5 事件处理中增加 `turn:completed` 的处理逻辑——将缓冲的 ResponseCard 标记为最终态，不再接收增量。如果 completion gate 判定未完成，前端展示 gate 的拒绝理由 + agent 继续工作。

### 3.5 P1 — Agent 颜色标识（MIMO agent.ts Info.color）

在 §4.2 色彩系统或 §3.2 TurnCard 中增加 agent 专属颜色：

```css
--color-agent-build: #fb8147;
--color-agent-plan: #c7e2a8;
--color-agent-compose: #a7a3d8;
```

TurnCard 中每个 ActivityRow 标注当前 agent 的 icon + color，让用户区分「这个 tool call 是哪个 agent 做的」。

### 3.6 P2 — Inbox 通知（MIMO inbox.ts）

在 §4.3 组件树或 §4.4 状态管理中增加 `Inbox` 概念。后台 subagent 完成后非侵入式通知（toast/侧边栏 badge），点击后 pushPanel 到 subagent 的 session view。

### 3.7 P2 — 多终端 Renderer 抽象预留（lark-bridge §4.3）

不只 React DOM，预留：

```
TurnState → renderWeb(state)  |  renderCard(state)  |  renderPush(state)
```

### 3.8 P2 — Annotation 优先级降级论据（salvage 缺失 annotation）

90-130K 行成熟产品也没有 annotation 系统——可以作为「先做 `<mark>` + CSS `::after`」的论据强化。


## 4. 不应吸收的确认

以下决策与外部研究的结论一致，保持现状：

| 外部结论 | Multivac 现状 | 一致性 |
|----------|-------------|--------|
| JSON string 数组存 IDs → relation table | `MultivacDb` trait relation table | ✅ |
| Channel 耦合 chat+task+meeting | Session / Task / Meeting 独立 | ✅ |
| gRPC agent engine 独立进程 | Orchest in-process library | ✅ |
| 10 个 Zustand stores | Jotai atom family | ✅ |
| WS + SSE 双重推送 | 仅 WebSocket | ✅ |
| bridge 的本地 profile 文件视为运行真相 | Multivac 的真相在 control plane + runtime 模型 | ✅ 不应照搬 |
| bridge 直接 spawn Claude/Codex CLI | Multivac 有 backend → agent-engine → runtime-host 分层 | ✅ 不应照搬 |
| 入口层价值会被平台压缩 | Multivac 定位 execution control plane | ✅ |


## 5. 优先级总表

### 对 `multivac-reconstruction-analysis.md`

| 优先级 | 修改 | 来源 |
|--------|------|------|
| **P0** | 文档开头增加「execution control plane」定位声明 | feishu.md |
| **P0** | Sub-agent spawn 契约吸收 context_mode / format / lifecycle | MIMO |
| **P0** | Hook 增加 ReAct 循环（`HookDecision::Continue`） | MIMO |
| **P0** | SessionIdentity 类型（scope + agent + workspace_fingerprint + policy_fingerprint） | lark-bridge |
| **P1** | Agent 工厂（LLM 动态生成 AgentConfig） | MIMO |
| **P1** | Completion Gate（独立 judge model 验证 task 完成） | MIMO |
| **P1** | Checkpoint-writer 子 agent 替代被动 AuditHook | MIMO |
| **P1** | ContextMode（full/diff/none）进入 sub-agent spawn | MIMO |
| **P1** | Workflow/Compose 预留（agent() + parallel() 原语） | MIMO |
| **P1** | MessageIngress trait 预留外部 IM 入口 | lark-bridge |
| **P1** | Employee 增加 Task 承接关系和 runtime context 绑定 | feishu.md |
| **P1** | RuntimeBackend trait 文档注释增加产品语义说明 | feishu.md |
| **P2** | Dream/Distill 远期规划（接口预留，不堵死） | MIMO |
| **P2** | PtyBackend workspace deny-list | lark-bridge |
| **P2** | PTY sideband input 预留 | salvage |
| **P2** | Knowledge Locator URI scheme | salvage |
| **P2** | Desktop 模式 profile 隔离 + 结构化日志脱敏 | lark-bridge |
| **P3** | Task Post-Processor Chain → handler chain 细化 | salvage |
| **P3** | Meeting summary by employee persona | salvage |

### 对 `multivac-frontend-design.md`

| 优先级 | 修改 | 来源 |
|--------|------|------|
| **P0** | Turn 初始化：HTTP POST → messageId → 前端 skeleton 渲染 | salvage |
| **P0** | TurnState 中间状态模型（reduceTurnState 纯函数） | lark-bridge |
| **P1** | TurnCard 增加「执行透明度信任界面」的产品语义 | feishu.md |
| **P1** | `turn:completed` commit 边界 + completion gate 状态展示 | salvage + MIMO |
| **P1** | Agent 颜色标识（--color-agent-*） | MIMO |
| **P2** | Inbox 通知机制 | MIMO |
| **P2** | 多终端 Renderer 抽象预留 | lark-bridge |
| **P2** | Annotation 优先级降级论据 | salvage 缺失 |
| **P3** | Workspace deny-list 前端错误提示 | lark-bridge |
