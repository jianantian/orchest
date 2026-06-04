# Kocoro vs Craft Agents OSS 对比分析

> 2026-06-01 | Kocoro (`shan` CLI + Desktop app) vs Craft Agents OSS (Electron + headless server)

---

## 一、规模与定位

| 维度 | Kocoro | Craft Agents OSS |
|------|--------|-----------------|
| **语言** | Go (~140K 行) | TypeScript (~346K 行) |
| **源文件** | ~460 .go 文件 | ~1,761 .ts/.tsx 文件 |
| **构建产物** | 单一二进制 `shan` (CLI + daemon) | Electron 桌面 + headless server + CLI |
| **桌面 UI** | 独立闭源 macOS Desktop app (通过 Unix domain socket RPC) | 开源 Electron app (React + Vite + shadcn) |
| **终端 UI** | Bubble Tea TUI | CLI 命令行 |
| **Web UI** | 无 | 内置于 headless server |
| **许可** | 混合（daemon 开源，Desktop 闭源） | Apache 2.0 |
| **LLM 后端** | Shannon Cloud gateway | Claude SDK + Pi SDK + 多 provider |
| **定位** | 生产级 macOS agent 守护进程 | 开源桌面 agent 平台 |

核心差异：Kocoro 是一个 **agent daemon**——运行在后台，可以被 Desktop app、TUI、IM 频道、定时任务、文件监听、MCP client 等各种触发器唤醒。Craft Agents 是一个 **desktop app**——用户打开它，打字，agent 回复。

---

## 二、架构对比

### 2.1 Kocoro

```
┌──────────────────────────────────────────┐
│          macOS Desktop App (闭源)          │
│        Unix domain socket RPC            │
└──────────────────┬───────────────────────┘
                   │
┌──────────────────▼───────────────────────┐
│           Daemon (HTTP :7533)             │
│  ┌─────────────────────────────────────┐ │
│  │         Agent Loop (6K行)            │ │
│  │  Tool dispatch, multi-tier compaction│ │
│  │  Memory preflight, mid-turn injection│ │
│  │  Idle watchdog, checkpoint persist   │ │
│  └─────────────────────────────────────┘ │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │ Tools    │ │ MCP      │ │ Session  │ │
│  │ Registry │ │ Client   │ │ Store    │ │
│  │ local>   │ │ + Server │ │ SQLite   │ │
│  │ MCP>     │ │ (dual)   │ │ FTS5     │ │
│  │ gateway  │ │          │ │          │ │
│  └──────────┘ └──────────┘ └──────────┘ │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │ Memory   │ │ Agents   │ │ Schedule │ │
│  │ TLM      │ │ Named    │ │ launchd  │ │
│  │ Sidecar  │ │ Configs  │ │ cron     │ │
│  └──────────┘ └──────────┘ └──────────┘ │
│  ┌──────────┐ ┌──────────┐              │
│  │ Perms    │ │ Watcher  │              │
│  │ Cmd      │ │ fsnotify │              │
│  │ Prefix   │ │ debounce │              │
│  └──────────┘ └──────────┘              │
└──────────────────────────────────────────┘
                   │
         ┌─────────┼──────────┐
         │         │          │
    ┌────▼──┐ ┌───▼───┐ ┌───▼──────┐
    │ TUI   │ │ IM    │ │ MCP      │
    │ Bubble│ │ Router│ │ Clients  │
    │ Tea   │ │       │ │ (外部)   │
    └───────┘ └───────┘ └──────────┘
```

### 2.2 Craft Agents OSS

```
┌──────────────────────────────────────────┐
│        Electron App (React/Vite)          │
│  ┌─────────────────────────────────────┐ │
│  │  SessionViewer, TurnCard, Annotation │ │
│  │  Overlay system, Panel Stack         │ │
│  │  Jotai atom families                │ │
│  └─────────────────────────────────────┘ │
│  ┌─────────────────────────────────────┐ │
│  │  Agent Backends                      │ │
│  │  ClaudeAgent (128K) / PiAgent (97K) │ │
│  │  BaseAgent (47K)                    │ │
│  └─────────────────────────────────────┘ │
│  ┌─────────────────────────────────────┐ │
│  │  Sources & MCP                       │ │
│  │  Session persistence (file system)   │ │
│  │  Auth, Credentials, Labels           │ │
│  └─────────────────────────────────────┘ │
└──────────────────────────────────────────┘
                   │
         ┌─────────┼──────────┐
         │         │          │
    ┌────▼──┐ ┌───▼───┐ ┌───▼──────┐
    │ Viewer│ │ Head  │ │ Messaging│
    │ (Web) │ │ less   │ │ Gateway  │
    │       │ │ Server │ │ WA/TG    │
    └───────┘ └───────┘ └──────────┘
```

### 2.3 关键架构差异

| 维度 | Kocoro | Craft Agents |
|------|--------|-------------|
| **进程模型** | 常驻 daemon（后台服务） | 按需启动 app（用户打开即用） |
| **Agent loop** | 自研 6K 行状态机 loop.go | 委托给 Claude/Pi SDK |
| **触发方式** | daemon 等待：Desktop 消息、IM @mention、定时任务、文件变更、MCP 调用 | 用户手动发送消息 |
| **工具系统** | 三层优先级 local > MCP > gateway，SafeChecker，CancelableMidTurn | 内置 tools + MCP sources |
| **MCP 角色** | 既是 Client（连接外部）又是 Server（暴露自己给其他 agent） | 主要是 Client，有 session MCP server |
| **Session** | SQLite FTS5 + JSON-on-disk，全文搜索，route-key 恢复 | 文件系统，按 workspace 组织 |
| **Memory** | 独立 TLM sidecar 进程，图状记忆召回 | 无 |
| **Named Agents** | 一等公民：目录化配置，独立 CWD/MCP/Session/Watches | 通过 workspace 间接实现 |
| **调度** | launchd cron + 内置 cron goroutine | 事件驱动的 automation engine |
| **文件监听** | fsnotify + debounce + glob，per-agent 配置 | 无 |
| **权限** | 命令前缀深度匹配、硬阻断 pattern、always-ask 前缀 | Permission mode (Explore/Ask/Auto) per session |
| **UI** | 独立 Desktop app (闭源) + TUI | 内建 Electron app (开源) + Web UI |

---

## 三、Kocoro 的独特优势（Craft Agents 没有的）

### 3.1 真正的 Agent Daemon

Kocoro 的 `shan daemon start` 启动一个常驻后台进程。它不是「打开一个 app 用 agent」——它是「agent 一直在线，被各种触发唤醒」：

- Desktop app 发消息 → daemon 处理
- Slack/IM @mention → daemon 处理
- 文件变更触发 watch → daemon 处理
- 定时任务 cron → daemon 处理
- 另一个 agent 通过 MCP 调用 → daemon 处理

**Multivac 启示**：我们的双模式（all-in-one Tauri + cloud SaaS）和这个模型有重叠。Desktop 模式下 Kocoro 的 daemon 模型更自然——agent 不应该只在用户打开 app 时才存在。

### 3.2 图状记忆系统

Kocoro 的 memory 系统是一个**独立 sidecar 进程**（TLM binary），通过 Unix domain socket 通信：

```
Agent Loop ──QueryRequest──> TLM Sidecar ──> Knowledge Graph
                                 │
                                 └── QueryResponse (direct_relation, path_query, typed_neighborhood)
```

不是 vector search，不是 embedding——是真正的图查询。"Agent A 和 User B 之间发生过什么？""这个 bug 上次是谁修的？"——图遍历直接回答。

**Multivac 启示**：这是 v2 的事。但架构上保留 memory sidecar 的扩展点是明智的——`RuntimeBackend` trait 可以加 `query_memory` 方法。

### 3.3 Named Agents 的目录化设计

```
~/.shannon/agents/
├── coder/
│   ├── config.yaml     # model, CWD, MCP servers, permissions, watches, heartbeats
│   ├── AGENT.md        # system prompt
│   ├── MEMORY.md       # auto-persisted learnings
│   ├── sessions/       # per-agent session store
│   └── skills/         # per-agent bundled skills
├── reviewer/
│   └── ...
└── ops/
    └── ...
```

每个 agent 是完全独立的命名空间——独立的 CWD、MCP servers、permissions、watches、heartbeats。不是 craft-agents 的「同一个 app 里切 workspace」——agent 之间物理隔离。

**Multivac 启示**：我们的 org/project 模型和这个互补。Kocoro 的 agent 粒度比我们的「session」粗——agent 是持久化配置实体，session 是一次对话。Multivac 可以两者兼有。

### 3.4 多级上下文压缩

Kocoro 的 loop.go 有 **6 种压缩路径**：

| 压缩类型 | 触发条件 | 行为 |
|---------|---------|------|
| Proactive | 预判 token 将超限 | 提前压缩 + 写 MEMORY.md |
| Preflight | 发 model request 前 | 检查 + 必要时压缩 |
| Reactive | model 返回 token 超限 error | 激进压缩后重试 |
| Force-stop | 紧急 | 最大压缩 |
| Time-based | 定时 | 轮询检查 |
| Write-before | 任何压缩前 | PersistLearnings → MEMORY.md |

每次压缩前先写 `MEMORY.md`（LLM 提取的持久化知识），压缩后做 `ShapeHistory` 摘要。不是在 context window 边界硬截断——是**智能压缩**。

**Multivac 启示**：Orchest 的 compaction 目前是单路径。Kocoro 的多级策略值得参考，特别是「压缩前先写 memory」这个模式。

### 3.5 命令级权限引擎

```
CheckCommand(cmd):
  1. 匹配 hard-block patterns → DENY
  2. 匹配 denied commands 列表 → DENY  
  3. 匹配 always-ask prefix 深度 → ASK
  4. 匹配 prefixDepthTable 规则 → 按层级决策
  5. 默认 → ASK
```

```
prefixDepthTable:
  "git"           → depth 1 (git status OK, git push ASK)
  "npm"           → depth 2 (npm install OK, npm publish ASK)
  "docker"        → depth 1
  "aws"           → depth 2
```

不是简单的 allow/deny——是按命令前缀深度逐级授权。`git status` 不需要审批，`git push --force` 需要。

**Multivac 启示**：我们的 Approval 枚举（Never/WhenRisky/Always）是 per-tool。Kocoro 的 prefix depth 是 per-command-argument。两者互补——Approval 用于 structured tools，prefix depth 用于 shell/CLI。

### 3.6 Claude Code 迁移

Kocoro 内置完整的 Claude Code → Kocoro 迁移路径：
- Scan：读取 `~/.claude/` 的 agents、skills、commands、rules、MCP configs
- Diff：对比 `~/.shannon/` 现状
- Apply：原子三步（Phase A staging → rename → Phase B），crash recovery via intent→applied→orphan manifest

**Multivac 启示**：用户 onboarding 的关键路径。如果 Multivac 能「一键导入 Claude Code 配置」，迁移成本降为零。

---

## 四、Craft Agents 的独特优势（Kocoro 没有的）

### 4.1 开源 UI 体系

Craft Agents 的整个前端是开源的——TurnCard、Annotation Island、Overlay 预览、Panel Stack、Permission UI。Kocoro 的 Desktop app 是闭源的。

**Multivac 启示**：我们的前端参考 Craft Agents 是正确的。Kocoro 没有开源 UI 可以参考——但我们仍然需要它的 daemon 模型。

### 4.2 Annotation / 追问系统

Craft Agents 的文本选择 → Island 菜单 → 追问 AI → 标注追踪，是整个 agent 交互领域最好的追问 UX。Kocoro 没有这个——它的 Desktop app 可能做得不错，但闭源看不见。

### 4.3 Multi-Session Inbox

Craft Agents 的 inbox 模型——多个 session 同时活跃，各自独立的 permission mode——是桌面 agent 的最佳实践。Kocoro 的 daemon 模型天然支持多 session 并发，但 UI 设计未知。

### 4.4 开源生态

Craft Agents 的 Apache 2.0 许可 + headless server + CLI + Web UI + 完整的开源 pipeline 让它可以被二次开发。Kocoro 的 daemon 虽然开源，但 Desktop app 闭源——你无法 fork 它的 UI。

### 4.5 自动化引擎

Craft Agents 的 event-driven automation（label 变更、schedule、tool use 触发）比 Kocoro 的 cron scheduling 更灵活。Kocoro 只有时间触发，没有事件触发。

---

## 五、对 Multivac 的启示

### 5.1 架构融合方向

```
Kocoro 的 daemon 模型 + Craft Agents 的 UI 体系 = Multivac 的目标
```

具体来说：

| 从 Kocoro 借鉴 | 从 Craft Agents 借鉴 |
|---------------|---------------------|
| Daemon 模式（agent 常驻后台） | TurnCard + SessionViewer 组件 |
| Named Agents 目录化配置 | Annotation Island 追问系统 |
| 多级上下文压缩（写前存 memory） | Multi-panel + Overlay 预览 |
| 命令级权限引擎（prefix depth） | Permission mode UI (Explore/Ask/Auto) |
| MCP dual role (client + server) | Jotai atom family 状态管理 |
| SQLite FTS5 session 全文搜索 | 多 session inbox |
| Memory sidecar（图状记忆） | 开源 UI 体系 |
| Claude Code 迁移路径 | Event-driven automation |
| fsnotify 文件监听 + debounce | |

### 5.2 不做的事

| Kocoro 做了但 Multivac 不做的 | 原因 |
|------------------------------|------|
| Go 语言 | Rust + Orchest SDK |
| 独立 Desktop app (闭源) | 我们用 Tauri（开源壳+WebView） |
| Shannon Cloud 绑定 | 我们模型无关 |
| TLM sidecar 进程 | 先做简单的 session 记忆，图记忆是 v2 |
| launchd cron | 先做内置 cron，不做 launchd 绑定 |
| Bubble Tea TUI | 先做 Web UI + Tauri 桌面壳 |
| IM channel router (Slack/Discord) | 不是 v0 优先 |

### 5.3 对 Multivac crate 结构的影响

Kocoro 的 daemon 模型对 Multivac 的架构有一个重要修正：**multivac-core 应该是一个 daemon-first 的库**，不是 request-response server。

```
multivac-core 应该暴露的不是 build_router() → Router
而是 build_daemon() → MultivacDaemon { router, session_mgr, event_bus, ... }

daemon 启动后：
- axum router 提供 HTTP/WS（前端连接）
- session_mgr 被各种 trigger 唤醒（消息、定时、文件变更）
- event_bus 广播给所有 subscriber（WebSocket、Desktop app、audit log）
```

这和我们现在的 `build_app()` 不矛盾——只是命名和语义从「server」变为「daemon」。「server」暗示被动响应 HTTP 请求，「daemon」暗示主动的后台 agent 生命周期。

---

## 六、总结

| | Kocoro | Craft Agents | Multivac (目标) |
|---|--------|-------------|----------------|
| **语言** | Go | TypeScript | **Rust** |
| **UI** | 闭源 Desktop + TUI | 开源 Electron + Web | **开源 Tauri + Web** |
| **模式** | Daemon | Desktop app | **Daemon (desktop) + Server (cloud)** |
| **Agent loop** | 自研 6K 行 Go | 委托 SDK | **Orchest SDK** |
| **Memory** | 图状 sidecar | 无 | **先 session 级，图是 v2** |
| **Permissions** | 命令前缀深度 | Session mode | **Approval 枚举 + prefix depth** |
| **Tools** | local > MCP > gateway | 内置 + MCP | **Orchest Tool trait + MCP** |
| **部署** | 单一 Go binary | Electron + 多包 | **Tauri binary + Docker** |
