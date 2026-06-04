# pi-subagents

**仓库**: https://github.com/nicobailon/pi-subagents
**一句话描述**: 让 Pi（AI 编码 agent）将工作委托给专注的子 agent，实现代码审查、调研、实现、并行审计和后台任务。

## 概述

pi-subagents 是 Pi Coding Agent 的扩展（npm 包 `pi-subagents`，v0.26.0），实现"父 agent 委托给子 agent"架构。父 Pi 会话通过 `subagent(...)` 工具调用启动子 agent 会话，每个子 agent 作为独立的 Pi 进程运行，拥有自己的上下文、提示词和工具。

该扩展不做任何自动化——它只提供委托能力。子 agent 有 8 个内建角色，支持四种执行方式：**单 agent**、**链式**（顺序串行）、**并行**（并发多 agent）和**动态扩展**（从结构化输出展开）。所有模式都支持前台和后台（`async: true`）运行。

核心哲学：父 agent 拥有完整的编排控制权；子 agent 是聚焦的、临时的、带有明确职责边界的工作者。

## 子 Agent 类型

| Agent | 职责 | 关键限制 |
|-------|------|----------|
| **scout** | 快速代码库侦察：定位关键文件、入口点、类型/接口、数据流和风险。输出 `context.md` 用于交接。 | 只读+write, intercom；降低思考预算（low） |
| **researcher** | 自主网络调研：分解 2-4 个角度，搜索官方文档/规范/基准，合成有来源的研究简报。输出 `research.md`。 | 需要 pi-web-access 扩展获取 web_search/fetch_content |
| **planner** | 从需求和代码上下文创建具体的实现计划，列出明确的任务、文件、依赖和风险。输出 `plan.md`。 | 不编辑代码；默认使用 fork 上下文；高思考预算 |
| **worker** | 实现 agent：单一写入者线程。执行批准的计划，如果发现未批准的决策则通过 contact_supervisor 上报。 | 不能自行做产品/架构决策；默认使用 fork 上下文；高思考预算 |
| **reviewer** | 多功能审查 specialist：审查代码 diff、计划、提案方案、代码库健康状况或 PR/issue。可进行小的修正性编辑。 | review-only/no-edit 约束优先于一切 |
| **context-builder** | 构建交接上下文：分析需求和代码库，生成 `context.md` 和 `meta-prompt.md`，供后续 agent 使用。 | 可做 web 调研；输出结构化交接文档 |
| **oracle** | 决策一致性顾问：从 fork 的父 session 中重建已继承的决策，检测方向漂移，建议最安全的下一步行动。 | 不编辑文件；不成为第二决策者；默认 fork 上下文；高思考预算 |
| **delegate** | 轻量级通用委托 agent：行为接近父 session。默认为 append 模式。 | 无默认输出或读取；最低配置 |

### 额外配置

这些字段统一了 agent 的行为：

- **`systemPromptMode`**: `replace`（默认，清除 Pi 基础提示词并替换）或 `append`（追加到基础提示词后）。
- **`inheritProjectContext`**: 是否继承 `AGENTS.md`、`CLAUDE.md` 等项目指令。内建 agent 默认开启。
- **`inheritSkills`**: 是否继承 Pi 的 skills 目录。内建 agent 默认关闭。
- **`defaultContext`**: `fresh`（干净的子会话）或 `fork`（从父 leaf 分支的会话）。planner、worker、oracle 默认 fork。
- **`defaultReads`**: 运行时预读取的文件。
- **`completionGuard`**: 带 bash 等变异工具的 validator/researcher 应设为 `false`，防止被误判为实现 agent。

## 通信模型

### 父→子：委托

父 agent 调用 `subagent(...)` 工具，参数决定执行方式：

```typescript
// 单 agent
subagent({ agent: "scout", task: "Map the auth flow" })

// 并行
subagent({ tasks: [
  { agent: "scout", task: "Frontend context" },
  { agent: "reviewer", task: "Review API client" }
]})

// 链式（顺序）
subagent({ chain: [
  { agent: "scout", task: "Gather context" },
  { agent: "planner", task: "Plan from {previous}" },
  { agent: "worker", task: "Implement {previous}" }
]})
```

委托的核心参数：

- **`agent` / `tasks` / `chain`** — 执行模式选择
- **`task`** — 任务描述，链中支持模板变量：`{task}`、`{previous}`、`{outputs.name}`、`{chain_dir}`
- **`context`**: `"fresh"`（干净上下文）或 `"fork"`（从父 session 分支，继承历史）
- **`async`**: `true` 时后台运行，父 agent 可继续自己的工作
- **`concurrency`**: 并行任务的最大并发数（默认 4）
- **`worktree`**: 并行写入时创建隔离的 git worktree
- **`output` / `outputMode`**: 输出文件和处理方式（inline / file-only）
- **`model`、`skill`、`reads`、`progress`**: 单次覆盖

### 子→父：结果返回

子 agent 完成后，父 agent 通过工具调用结果接收输出。并行任务输出聚合并传递给下一步。链中通过 `{previous}` 和命名输出 `{outputs.name}` 传递。

对于异步（后台）运行，父 agent 通过 `subagent({ action: "status", id: "..." })` 查询状态，完成后结果自然返回。

### 子→父：运行时通信（pi-intercom）

可选组件 `pi-intercom`（`npm:pi-intercom`）在父子之间建立双向通信通道：

- **`contact_supervisor({ reason: "need_decision", message: "..." })`**: 子 agent 阻塞等待父决策。父通过 `intercom({ action: "reply", message: "..." })` 回复。
- **`contact_supervisor({ reason: "progress_update", message: "..." })`**: 子 agent 非阻塞汇报进展或重要发现。
- 父侧的 `pi-subagents` 通过 pi-intercom 发送分组成品结果：每组前台执行一条消息，每个完成的异步结果文件一条消息。

通信桥通过环境变量和桥指令注入到子 agent 会话中。子 agent 的提示词末尾会追加一条桥指令，说明可使用的 supervisor 通信通道。

### 嵌套事件系统

子 agent 如果有 `subagent` 工具（仅当显式配置），可以进一步委托。嵌套运行通过文件系统事件机制通信：

- 事件路由（event sink / control inbox）写入 `/tmp/nested-subagent-events/`
- 事件类型：`started`、`updated`、`completed`、`control-result`
- 父 agent 读取嵌套事件以构建实时状态树
- 最大深度：3 层；最大步骤数：12；最大子 agent 数：16

## 执行方式

### 前台 vs 后台

- **前台**: 子 agent 在会话中流式输出进度，父 agent 等待完成后收到结果。
- **后台**（`async: true`）: 子 agent 独立运行，父 agent 可以结束本次轮次或继续本地工作。Pi 在后台完成时自动投递结果。

### 上下文模式

- **`fresh`**: 启动干净的子 agent 会话，不带父历史。适用于需要无偏见的审查或独立任务。
- **`fork`**: 从父 session 的当前 leaf 创建分支会话。子 agent 继承父的历史对话并以此为基线合同。适用于 oracle 审查、worker 延续线程。

fork 失败（session 未持久化、leaf 缺失）时会直接报错，不会悄悄降级为 fresh。

### 链式变量

| 变量 | 来源 |
|------|------|
| `{task}` | 链中第一步的原始任务 |
| `{previous}` | 上一步的输出 / 并行步骤的聚合输出 |
| `{chain_dir}` | 链临时目录路径 |
| `{outputs.name}` | 通过 `as: "name"` 存储的特定步骤输出 |

### 动态扩展（Dynamic Fanout）

链式执行中，第一步以结构化输出（`structured_output` + `outputSchema`）返回项目列表，然后 `expand` 步将其展开为多个并行的子 agent 实例。例如：scout 返回审查目标列表 → 每个目标启动一个 reviewer → 结果聚合到 `collect.as`。

### 工作区隔离

`worktree: true` 为每个并行的写入任务创建独立的 git worktree，避免文件冲突。要求工作区干净、在 git 仓库内运行。

## 关键设计决策

1. **子 agent 是独立进程** — 每个子 agent 作为独立的 Pi 进程启动，有自己完整的环境和生命周期。不是轻量级函数调用。

2. **子安全边界** — 子 agent 默认不接收 `pi-subagents` 技能、不注册 `subagent` 工具（除非显式配置 `tools: subagent`）、继承上下文时过滤掉父 agent 的编排消息和旧的控制消息。子 agent 收到明确指令："你不是父编排器，不要提议或运行子 agent"。

3. **Fork 上下文 vs Fresh 上下文** — fork 需要父 session 已持久化。这是有意的设计：它确保子 agent 有真实的历史可继承，而不是静默失败。

4. **单写入者模式** — 一条黄金规则："Keep the write path single-threaded even when the run is async"。worker 是唯一允许进行写入的 agent；reviewer 只在需要修正时做小的编辑。

5. **决策上报，不是猜测** — worker 遇到未批准的决策时必须通过 `contact_supervisor` 上报，不能自己猜测。oracle 的角色是发现这些决策间隙。

6. **桥可选，不影响可用性** — `pi-intercom` 是可选依赖。没有桥时子 agent 通过正常工具调用结果返回，只是缺少运行时双向通信。

7. **内建 agent 可覆盖** — 不通过复制 agent 文件来改模型，而是用 `agentOverrides` 配置。覆盖支持 `model`、`thinking`、`fallbackModels`、`tools`、`skills` 等多个字段。

8. **链式优先于复杂提示** — 推荐的工作流程是明确分解为 scout → planner → worker → reviewer 等步骤，而不是在一个复杂的提示词中要求一个 agent 完成所有事。

## 局限

1. **嵌套深度限制** — 子 agent 只能进行有限层级的嵌套委托（最大深度 3）。不是递归无限制的 agent 树。

2. **Fork 要求持久化** — `context: "fork"` 假设父 session 已持久化且存在当前 leaf。在早期会话或短会话中 fork 会失败。

3. **工作区隔离条件** — `worktree: true` 要求干净的 git 状态。未提交的更改会阻止使用。

4. **无自动后台审查** — 安装扩展不会自动在后台启动审查。父 agent 必须显式请求。

5. **嵌套不支持动态扩展** — 动态扩展（`expand`）不支持嵌套 fanout、动态 agent 选择、reducer、条件或任意表达式。只在顶层链中可用。

6. **`.chain.md` 不支持动态语法** — 动态扩展只能通过直接 `subagent({ chain: [...] })` JSON 或 `.chain.json` 文件使用。`.chain.md` 文件不支持动态展开。

7. **父 session 必须是 Pi** — 该扩展是一个 Pi Coding Agent 扩展。它不能嵌入到其他系统中或独立运行。

8. **子 agent 不共享内存状态** — 所有通信都通过进程边界（stdin/stdout、JSON 文件、可选的 intercom 通道）。没有共享内存、数据库或 RPC 系统。
