# Nimbalyst vs Multivac 对比分析

> 2026-06-01 | Nimbalyst (~2,201 文件, 652K 行 TypeScript) vs Multivac (目标架构)

---

## 一、为什么 Nimbalyst 是最接近 Multivac 的产品

Kocoro 是 daemon，Craft Agents 是 desktop app。Nimbalyst 是 **daemon + 多 agent 编排 + 产品 UI**——三者合一。它是 Multivac 目标形态的最接近参照物。

| 维度 | Nimbalyst | Multivac |
|------|-----------|---------|
| **主 agent loop** | Claude Agent SDK / Codex ACP（委托 SDK） | Orchest `AgentRun`（自研 Rust SDK） |
| **子 agent 委派** | TeammateManager（2046行，in-process SDK subprocess） | `RuntimeBackend` trait + agent-as-tool |
| **CLI agent 接入** | Claude Code / OpenCode / Copilot CLI 通过 ACP/JSON-RPC stdio | `RuntimeBackend::start_task`（同构） |
| **多 provider** | AIProvider + AgentProtocol 双层抽象（8种） | ModelAdapter（模型无关） + RuntimeBackend（执行平面无关） |
| **Tool 系统** | ExtensionAITool → ToolDefinition → toolRegistry | `Tool` trait → `ToolRegistry`（类型安全） |
| **编辑/AI 集成** | Lexical editor + inline diff + DiffApprovalBar | Tiptap + TurnCard + react-markdown |
| **UI 模式** | 文档中心（agent 直接编辑 document） | 对话中心（agent 产生结构化事件→渲染为卡片） |
| **协作** | Yjs CRDT + Cloudflare DO + E2E 加密 | 先简单多用户观看，Yjs 后续 |
| **部署** | Electron 桌面 + iOS/Android 移动 | Tauri 桌面（all-in-one）+ Docker（cloud） |
| **许可** | 闭源 | 待定 |

---

## 二、多 Provider/Agent 架构：Nimbalyst vs Multivac

### 2.1 Nimbalyst 的两层抽象

```
AIProvider (base class, per session)
  ├── ClaudeCodeProvider   → Claude Agent SDK (query/streamInput)
  ├── ClaudeProvider       → @anthropic-ai/sdk (direct API)
  ├── OpenAICodexProvider  → Codex ACP (JSON-RPC stdio subprocess)
  ├── OpenCodeProvider     → OpenCode SDK
  ├── CopilotCLIProvider   → gh copilot CLI subprocess
  └── LMStudioProvider     → local LM Studio

AgentProtocol (normalizes SDK differences)
  ├── ClaudeSDKProtocol    → wraps claude-agent-sdk query()
  ├── CodexACPProtocol     → spawns ACP binary, JSON-RPC stdio
  └── ...                  → one per SDK
```

**Provider 角色**：管理 agent 生命周期（initialize, sendMessage, abort, interruptCurrentTurn），一个 provider 实例绑定一个 session。

**Protocol 角色**：隔离 SDK 细节——session 创建/恢复、消息发送、流式事件解析。Provider 通过 Protocol 调用 SDK，不直接接触 Claude/Codex API 差异。

**ProviderFactory** 静态 Map：`{type}-{sessionId}` → AIProvider。8 种 provider 类型通过 switch 创建。

### 2.2 Multivac 的等价抽象

```
ModelAdapter (Orchest trait)
  ├── AnthropicAdapter  → 直接 API
  └── OpenAIAdapter     → 直接 API

RuntimeBackend (Multivac trait)
  ├── Embedded local    → Claude Code PtyRuntime (spawn CLI, JSONL/PTY → TaskEvent)
  ├── RemoteGrpc        → tonic gRPC (cloud-internal)
  └── ReverseWebSocket  → user 本机主动连接（NAT traversal）
```

**关键差异**：
- Nimbalyst 的 Claude Code 是通过 SDK（in-process）跑的，Multivac 是通过 `RuntimeBackend` 委派到外部进程
- Nimbalyst 的 Protocol 层是 TypeScript 接口（运行时多态），Multivac 的 RuntimeBackend 是 Rust trait（编译期静态分发）
- Nimbalyst 的 Provider 和 Protocol 是两个独立层，Multivac 的 RuntimeBackend 把它们合二为一——因为远端 CLI agent 不需要「SDK 适配」，只需要「进程管理 + 事件转译」

### 2.3 谁更优

| 维度 | Nimbalyst | Multivac |
|------|-----------|---------|
| **新增 provider** | 实现 AgentProtocol + 注册到 ProviderFactory | 实现 RuntimeBackend trait |
| **类型安全** | TypeScript（运行时） | Rust trait（编译期） |
| **远端执行** | 不直接支持（均为 in-process） | 天然支持（RemoteGrpc, ReverseWS） |
| **SDK 版本耦合** | Protocol 隔离了 SDK 差异，但升级仍可能破坏 | RuntimeBackend 不耦合 SDK——只管理进程 |

Multivac 的 RuntimeBackend 比 Nimbalyst 的 Provider/Protocol 更干净——因为它不需要「适配多种 SDK」——它只做进程管理 + 事件转译。Orchest 的 `ModelAdapter` 承担了 Nimbalyst 中「适配不同 LLM API」的职责。

---

## 三、子 Agent / Team 系统

### 3.1 Nimbalyst 的 TeammateManager

2046 行代码，三种 agent 类型：

| 类型 | 生命周期 | 用途 |
|------|---------|------|
| **Teammate**（named, team-based） | 可 idle，可 resume | 长期协作：coder, reviewer, planner |
| **Background agent**（fire-and-forget） | 一次执行，无 idle | 后台任务：git cleanup, file indexing |
| **Plain sub-agent**（SDK native） | 一次性 | 临时委派 |

Teammate 的配置文件：`~/.claude/teams/{name}/config.json`

**消息传递**：通过 Claude SDK 的 `streamInput()` 注入——主 agent 在运行中，子 agent 通过 StreamInput 发送消息给主 agent。子 agent 之间通过 inbox 文件交换消息。

**核心机制**：
```
主 agent 调用 TaskCreate → TeammateManager 拦截
  → spawn Claude Code subprocess (via SDK query())
  → 子 agent 执行
  → 结果通过 streamInput() 注入主 agent 的上下文
  → 主 agent 继续决策
```

### 3.2 Multivac 的等价设计

```
Orchest agent 调用 start_agent_task("claude-code", prompt)
  → RuntimeBackend::start_task()
  → spawn Claude Code / Codex 进程
  → TaskEvent 流转译
  → 结果通过 Orchest tool result 返回主 agent
  → 主 agent 继续决策
```

**差异**：
- Nimbalyst 的 TeammateManager 是 SDK 级别的拦截——截获 tool call，在 SDK 内部 spawn subprocess
- Multivac 的 agent-task tools 是普通 Tool trait 实现——模型调用 `start_agent_task`，和调用 `read_file` 一样
- Nimbalyst 的 idle/resume 队友机制比 Multivac 更成熟——Multivac 目前是一次性 task

**Multivac 的优势**：因为 agent-task tools 是普通 Tool trait，所以**任何 Orchest agent 都可以调用**——不只是「主 agent 委派给子 agent」，也可以是「一个专门负责编排的 Orchest agent 管理和协调多个 Claude Code 实例」。这和 Nimbalyst 的 meta-agent MCP server 思路一致，但实现更通用。

---

## 四、Tool / Extension 系统

### 4.1 Nimbalyst：动态 JS 运行时

```
Extension manifest → ExtensionAITool[]
  → ExtensionLoader (JS bundle 激活)
  → ExtensionAIToolsBridge (工具注册)
  → ToolDefinition { name, description, inputSchema, handler } 
  → toolRegistry.set(name, definition)
  → MCP bridge (暴露给 Claude Code as MCP tool)
```

Extension 是 JS bundle，可以动态加载。Extension 可以注册：editor、panel、command、AI tool、theme。

### 4.2 Multivac：静态 Rust trait

```
impl Tool for KnowledgeSearchTool { ... }
  → registry.register(knowledge_tool)
  → ToolRegistry { name → Arc<dyn Tool> }
  → 模型在 system prompt 中看到 tool list
```

Multivac 不做动态 JS 加载——Product tool 在编译期注册。这更靠近 Nimbalyst 的 `BUILT_IN_TOOLS` 而不是 `ExtensionAITool`。

**差异定位**：Nimbalyst 有成熟的第三方扩展生态（20+ built-in extensions + marketplace）。Multivac 初期只需要 5 个 product tool + agent-task tools，动态扩展是远期的差异化能力。

---

## 五、编辑器/AI 集成：根本性哲学差异

这是 Nimbalyst 和 Multivac 最大的分歧点。

### 5.1 Nimbalyst：文档中心（agent edits document）

```
用户打开 document → Agent 直接编辑 document
  → Lexical editor 显示 inline diff（红/绿）
  → DiffApprovalBar：逐块 approve/reject
  → Agent 看到的是「编辑器状态」，不是「聊天历史」
```

Tool 执行结果直接写入 Lexical document state——`applyDiff` tool 把 diff 应用到编辑器，用户看到的是 WYSIWYG 变更。

### 5.2 Multivac：对话中心（agent produces structured events）

```
用户发送消息 → Agent 返回结构化事件
  → TurnCard 渲染：tool call（可折叠）+ text（流式缓冲）
  → Agent 看到的是「对话历史」，不是「编辑器状态」
  → 文件变更是 tool result 的一部分，渲染为 diff overlay
```

Tool 执行结果渲染为 TurnCard 中的 ActivityRow——不直接改变编辑器状态。用户和 agent 的交互通过**对话**进行，不是通过**共享文档**。

### 5.3 谁是更好的模型

| | Nimbalyst (文档中心) | Multivac (对话中心) |
|---|---------------------|-------------------|
| **代码编辑** | 极好——像结对编程 | 一般——diff 是 overlay |
| **多步骤推理** | 差——agent 只能「改文档」 | 极好——TurnCard 展示完整推理链 |
| **子 agent 可视化** | 困难——多个 agent 改同一文档 | 清晰——ActivityGroupRow 树状展示 |
| **追问/交互** | 困难——没有聊天概念 | 极好——Annotation Island 追问 |
| **初学者友好** | 极高——像用 Google Docs | 中——需要理解 agent 对话模式 |
| **复杂工作流** | 差——文档不是工作流抽象 | 极好——TurnCard + Tool call 是天然工作流 |

**Multivac 不应追求 Nimbalyst 的文档中心模式**——我们的定位是 agent orchestration，不是 AI-native 编辑器。Nimbalyst 的核心竞争力是 Lexical + inline diff + DiffApprovalBar 的编辑体验——这是他们 300+ 文件编辑器内核的护城河。Multivac 不应该在这个维度竞争。

**Multivac 应该保留对话中心 + TurnCard 模型**——这是 Craft Agents 已经验证的最佳实践，也是 agent orchestration（多 agent、推理链、追问）最自然的 UI。

---

## 六、Nimbalyst 比 Multivac 领先的

1. **Editor 内核**：Lexical + inline diff + DiffApprovalBar——300+ 文件，3 年迭代。不做。

2. **Extension marketplace**：动态 JS 加载 + 20+ built-in extensions + 第三方生态。初期不做。

3. **Teammate idle/resume**：命名 agent 可以 idle，后续 resume——长期记忆在 agent 级。我们的 session 级持久化可以演进到这个方向。

4. **Yjs CRDT 协作**：多人实时编辑同一文档。我们的多用户观看是简化版，CRDT 是后续。

5. **MCP server 暴露自身**：Nimbalyst 可以当 MCP server，让其他 agent 调用它的工具。我们的 MCP 集成目前是 client 方向。

6. **iOS/Android 客户端**：原生移动端。先做 Web。

## 七、Multivac 比 Nimbalyst 领先的

1. **Rust core**：类型安全、性能、无 GC。Nimbalyst 的 TypeScript 单线程模型受限于 V8。

2. **RuntimeBackend 远程执行**：Nimbalyst 的 agent 全部 in-process。Multivac 可以管理远端 CLI agent。

3. **Orchest SDK 独立演进**：Nimbalyst 的 agent loop 深度绑定 Claude Agent SDK。Multivac 的 Orchest SDK 可以独立升级。

4. **对话中心 UI**：TurnCard + Annotation——对 agent orchestration 场景更自然。

5. **Product tool 类型安全**：Rust trait vs TypeScript interface——编译期保证，不是运行时。

6. **SquadDb trait 双后端**：SQLite + Postgres 同一接口——Nimbalyst 只有 PGLite。

## 八、不做的事（受 Nimbalyst 启发后确认）

| Nimbalyst 做了但 Multivac 不做的 | 原因 |
|---------------------------------|------|
| Lexical editor + inline diff | 护城河太深，对话中心模型不需要 |
| 文档为中心的 agent 交互 | 和 agent orchestration 定位冲突 |
| Extension marketplace | 远期的差异化，不是 v0 |
| 20+ built-in extensions | 从 5 个 product tool 开始 |
| iOS/Android 原生客户端 | 先做 Web + Tauri 桌面 |
| Yjs CRDT 实时协作 | 先做简单多用户观看 |
| Copilot CLI / LM Studio 接入 | 先做 Claude Code + Codex |
