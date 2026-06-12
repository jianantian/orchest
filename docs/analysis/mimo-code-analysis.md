# MIMO-code 调研：对 Multivac 架构的启发

调研日期：2026-06-11
仓库：`~/GitHub/agent/MIMO-code`
身份：OpenCode fork，terminal-native AI coding agent，MIT 开源
规模：10M+ 总下载量（GitHub + npm），日均 ~300K 增量，2026 年 1 月至今增速未降
技术栈：TypeScript + Effect.ts + Bun + Drizzle ORM + SQLite + SST (AWS) + Electron 桌面端

包结构：`packages/opencode`（核心）、`packages/app`（Web UI）、`packages/desktop`（Electron）、`packages/slack`（Slack 通知）、`packages/enterprise`、`packages/console`、`packages/containers`


## 1. 总体判断

MIMO 在 OpenCode 的基础上追加了三个 agent orchestration 深度层，这是它和「调 API 的 terminal 工具」的根本差异，也是 Multivac 当前架构文档尚未充分设计的部分：

1. **Subagent 不是 fire-and-forget** — 有 context mode 控制、ReAct hook 循环可重新驱动、completion gate 独立验证、background/foreground 区分
2. **上下文管理不是定时保存** — 有专门的 checkpoint-writer agent 决策何时保存、token-budgeted 恢复、context reconstruction
3. **Agent 本身是可编程资源** — 可 LLM 动态生成、可通过 dream/distill 从历史中学习进化、可被 workflow 脚本编排


## 2. Agent 体系

### 2.1 Agent 类型层次

```
预置 agent（build / plan / compose / max / general / explore / title / summary / checkpoint-writer）
    +
LLM 动态生成 agent（agent.generate(description) → identifier + systemPrompt）
    +
用户自定义 agent（.mimocode/mimocode.json agent 配置）
    +
Workflow agent（compose 模式下脚本编排 agent 协作）
```

每个 agent 定义（`Info`）含：

| 字段 | 说明 |
|------|------|
| `name` | 标识符 |
| `mode: "primary" \| "subagent" \| "all"` | primary 可通过 Tab 切换；subagent 仅可被调用 |
| `permission: Ruleset` | 独立的权限规则集，三层 merge：defaults → agent 默认 → user override |
| `model` / `modelRef` | 绑定的 model |
| `prompt` | 可选的独立 system prompt（覆盖默认） |
| `toolAllowlist` | 工具白名单（空 = 全部可用） |
| `steps` | 最大步数 |
| `temperature` / `topP` | 模型参数 |
| `color` | TUI 中的展示色 |
| `hidden` | 是否在 agent 列表中隐藏 |

### 2.2 Agent 动态生成（运行时子 agent 专业化）

`agent.generate(description)` 表面上是「LLM 帮你写 system prompt」——但用户手动创建 agent 的场景根本不值得做这个功能。它的真正用途在另一个地方：**primary agent 在运行时为自己创建特化的子 agent**。

具体场景：

```
build agent 在修一个 DB schema 迁移的 bug
  → 它调用 agent.generate({
      description: "review only database migration files, check backward compatibility and index safety"
    })
  → LLM 生成一个特化 agent：system prompt 嵌入了 FK 约束、索引安全、数据类型转换的审查规则
  → tool allowlist 只有 read + grep + glob（不会写）
  → spawn，干完活，消失
```

这不是用户预配置的——用户不会预料到要做 DB 迁移审查。是 **agent 自己在运行时做 delegation 决策时，动态创建的专业化分身**。

没有这个能力时，primary agent 做 delegation 只有两个选择：

- 用 generic subagent（general/explore），把所有领域知识塞进 task prompt → prompt 膨胀、token 浪费
- 不 delegating，自己做 → 主 agent 时间窗口被消耗

有 `agent.generate()` 后：领域知识进 system prompt，task prompt 保持干净；tool allowlist 精准匹配任务粒度；agent 特化程度匹配 delegation 的 fidelity 需求。

本质不是「省 YAML」——是 **LLM 作为 meta-agent，在运行时按需配置专业化子 agent，提高 delegation fidelity**。和 workflow 脚本里的 `agent(prompt, { agent: "build" })` 对称：一个是调用 agent，一个是**创建** agent。两者指向同一个命题：**agent 是可编程资源，不只是静态配置项**。

### 2.3 权限模型

```typescript
// Permission 是 3 层 merge
const effective = Permission.merge(defaults, agentOverrides, userOverrides)

// 值： "allow" | "deny" | "ask"
// Pattern 支持 glob： "*.env", "src/**/*.ts"
// 后者覆盖前者

// 例：explore agent（只读）
explore: {
  "*": "deny",
  grep: "allow", glob: "allow", read: "allow",
  bash: "allow", list: "allow",
  webfetch: "allow", websearch: "allow", codesearch: "allow",
}

// 例：plan agent（编辑仅 .mimocode/plans/）
plan: {
  edit: { "*": "deny", ".mimocode/plans/*.md": "allow" },
}
```

每个 agent 有自己的独立 permission ruleset。Agent 被 spawn 时，子 agent 继承**父 agent 的 ruleset**（而非自己的），确保权限语义一致（`ForkContext.parentPermission`）。


## 3. Subagent Spawn（actor/spawn.ts）

这是 MIMO 最精巧的模块。

### 3.1 SpawnInput 契约

```typescript
interface SpawnInput {
  mode: SpawnMode               // "subagent" | "peer"（peer 创建新 session）
  sessionID: SessionID
  parentSessionID?: SessionID   // 子 session / 父 session 分离
  agentType: string
  task: string
  description?: string
  context: ContextMode           // "full" | "diff" | "none"
  tools: ToolWhitelist           // 子 agent 的工具白名单
  background: boolean            // 后台运行 → inbox 通知；前台 → 直接返回
  lifecycle: "ephemeral" | "persistent"
  format?: json_schema           // 结构化输出
  task_id?: string               // 绑定到 task，spawn 时自动 start
  parentActorID?: string
  model?: { providerID; modelID }
  onActorID?: (actorID) => void  // spawn 时机回调
}
```

关键设计：

- **parentSessionID ≠ sessionID**：checkpoint-writer 子 agent 在独立 child session 运行，但向父 session 的 checkpoint.md 写入
- **context 三种模式**：`"full"`（不浪费 token）、`"diff"`（减少无关历史）、`"none"`（纯独立微任务）
- **task_id 自动 start**：spawn agent for task = task 自动进入 running 状态。这是结构性副作用，不依赖 model

### 3.2 ReAct Hook 循环

子 agent 完成后不是直接结束——进入 hook 循环：

```
subagent 产出结果
  → preStop hooks 检查 → 可以返回 { action: "continue", reason: "…" }
    → 如果 continue：重新调用 runAgentLoop，注入 reason 作为新 task
    → 最多 3 次（MAX_PRE_REACT）
  → postStop hooks 检查（同上）
  → completion gate（独立 judge model 判定）
```

核心代码结构：

```typescript
let iteration = 0
while (true) {
  const turn = yield* runTurn(runAgentLoop({
    task: lastDecision ? lastDecision.reason : input.task,
    source: lastDecision ? "hook" : "spawn",
  }))
  
  iteration++
  if (iteration > MAX_PRE_REACT) break

  lastDecision = yield* hooks.preStop(actorID, outcome)
  if (!lastDecision) break  // hook 说 stop
}
```

这意味着 hook 不只是通知——它可以**重新驱动 agent**。

### 3.3 Completion Gate

对于 `gateEligible=true` 的 subagent，最终输出由独立的 judge model（更便宜/更小）评估是否真正完成任务：

```typescript
type AgentOutcome =
  | { status: "success"; finalText?: string; structured?: unknown;
      reportedStatus?: "complete" | "partial" | "blocked";
      incompleteTasks?: string[] }
  | { status: "failure"; error: string }
  | { status: "cancelled" }
```

如果 gate 判定未完成，`reportedStatus` 降级为 `"partial"` 或 `"blocked"`，父 agent 收到不完整信号。

### 3.4 ForkContext

spawn 时捕获父 agent 的瞬时快照，供子 agent 的 runLoop 使用：

```typescript
interface ForkContext {
  system: string[]               // 父 agent 的 system prompt
  tools: Record<string, AITool>  // 父 agent 视角的 tool schema
  parentPermission: Ruleset      // 继承父的权限规则
  inheritedMessages: ModelMessage[]
  watermarkMsgID: MessageID      // 快照时刻的边界标记
  model: { providerID; modelID }
}
```

子 agent 的 tool 可见性和权限语义与父 agent 一致。


## 4. 上下文管理（session/checkpoint.ts + memory/）

### 4.1 Checkpoint-Writer 子 agent

不是定时保存——有一个专门的 **checkpoint-writer 子 agent** 后台持续运行，负责：

- 基于 context window 使用率决策何时写（不固定间隔）
- 维护多个文件：`checkpoint.md`、`MEMORY.md`、`notes.md`、`tasks/<id>/progress.md`
- 自动注入 autonomous loop reminder：让 agent 知道自己 mid-loop
- Stop-guard：检查 progress.md，阻止「乐观停止」

### 4.2 Context Reconstruction

当上下文逼近窗口上限：

```
重建上下文 = 最新 checkpoint.md（token-budgeted）
           + MEMORY.md（按重要性注入）
           + 当前 task 的 progress.md
           + 保留的最近 N 条消息
```

### 4.3 Token-Budgeted Injection

不是全量加载，按 token budget 和优先级注入：

```typescript
const CHECKPOINT_SECTION_BUDGETS = {
  "Current Task":    0.35,
  "Recent Changes":  0.25,
  "Open Questions":  0.15,
  "Project State":   0.15,
  "Next Steps":      0.10,
}
```

### 4.4 Memory 系统

- SQLite FTS5 全文搜索
- Scoped 搜索：`path` / `scope`（project / global）/ `type`
- Reconcile：增量索引新文件 + 清理已删除文件
- 独立于 checkpoint——memory 是跨 session 的项目知识，checkpoint 是单 session 的运行时状态


## 5. Workflow / Compose（workflow/）

### 5.1 声明式脚本

```javascript
export const meta = {
  name: "fullstack-feature",
  phases: [
    { title: "Design", agent: "plan", prompt: "Design the architecture" },
    { title: "Implement", agent: "build", prompt: "Implement the design" },
    { title: "Review", agent: "build", prompt: "Review the implementation" },
    { title: "Test", agent: "build", prompt: "Write and run tests" },
  ]
}
```

### 5.2 运行时原语

```javascript
// agent() — 阻塞到完成
const design = await agent("Design the schema", { agent: "plan" })

// parallel() — 并发
const [api, ui] = await parallel([
  () => agent("Build the API"),
  () => agent("Build the UI"),
])

// pipeline() — 流水线
const result = await pipeline(items,
  (item) => agent(`Analyze ${item}`),
  (analysis) => agent(`Implement ${analysis}`),
)
```

### 5.3 安全与可靠性

- **Sandbox**：QuickJS 沙箱执行脚本，不是 `eval`
- **Meta 解析**：递归下降 parser 纯数据读取——`export const meta = { … }` 在解析阶段**不执行任何代码**
- **文件 I/O jailed** 到 workspace
- **Per-agent timeout**：单个 agent 超时 → 取消，返回 null（不阻塞 parallel barrier）
- **全局 deadline**：整个 workflow 最大 12h
- **并发上限**：`maxConcurrentAgents = 16`
- **生命周期上限**：`MAX_LIFECYCLE_AGENTS = 1000`
- **Journal 持久化**：崩溃后可恢复，script 变更时 fresh journal


## 6. Task 系统（task/registry.ts）

- 树形 ID：`T1` → `T1.1` → `T1.2` → `T2`
- 状态机：`pending` → `running` → `done` / `blocked` / `abandoned`
- Auto-start on spawn：spawn agent for task = task 自动 running（结构性副作用）
- Task events：每次状态变更写入事件日志
- Gate-state：子 agent 报告完成，judge model 验证后才是真 `done`


## 7. Dream & Distill
- **Dream**（`/dream`）：扫描 session traces → 提取持久知识 → 更新 MEMORY.md，移除过时条目
- **Distill**（`/distill`）：发现重复手动模式 → 自动打包为 skill / subagent / command

项目知识和 skill 不是纯手工维护——agent 从执行历史中自我学习。


## 8. 对 Multivac 的启发

### 8.1 对 `multivac-reconstruction-analysis.md`

| 优先级 | 修改 | 依据 |
|--------|------|------|
| **P0** | Sub-agent spawn 契约吸收 `context_mode` / `output_format` / `lifecycle` / `background` / `gate` | SpawnInput §3.1 |
| **P0** | Hook 返回值扩展 `HookDecision::Continue` / `Escalate`，agent loop 支持 ReAct 重新驱动（cap 3） | ReAct Hook §3.2 |
| **P1** | Agent 运行时专业化：LLM 作为 meta-agent 按需创建特化子 agent（system prompt + tool allowlist + permission），提高 delegation fidelity | agent.generate() §2.2 |
| **P1** | Completion Gate：独立 judge model 验证 task 完成 | §3.3 |
| **P1** | Checkpoint-Writer 子 agent 替代被动 AuditHook + token-budgeted 上下文注入 + context reconstruction | §4.1–§4.3 |
| **P1** | ContextMode（`Full` / `Diff` / `TaskOnly`）进入子 agent spawn | §3.1 |
| **P1** | Workflow/Compose 预留最简原语 `agent()` + `parallel()` | §5.2 |
| **P1** | Employee 增加 Task 承接关系和 runtime context 绑定 | §2.1 agent 模式 |
| **P2** | Dream/Distill 远期规划——knowledge trait 不堵死从执行历史学习的路径 | §7 |

### 8.2 对 `multivac-frontend-design.md`

| 优先级 | 修改 | 依据 |
|--------|------|------|
| **P1** | Agent 颜色标识（`--color-agent-*`），TurnCard 中 ActivityRow 标注 agent icon + color | agent.color §2.1 |
| **P1** | Completion Gate 的前端状态展示（gate 评估中 / 未通过理由 / 继续执行） | §3.3 |
| **P2** | Inbox 通知：后台 subagent 完成后 toast/侧边栏 badge | background: true §3.1 |
| **P2** | Workflow Phase 进度在前端的可视化 | §5.1 meta.phases |
