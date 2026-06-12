---
## 产品定位

**Multivac 的产品本体是 execution control plane——不是聊天插件，不是 IM bot 框架。**

Session（聊天）只是 task/runtime 的一个视图，不是系统真相的唯一容器。外部 IM（飞书/Slack）是 distribution surface，不是产品定义。当 Session 和 Task 的设计冲突时，Task 的完整性优先。

价值锚定在四个层面：
- **Task** 是执行真相
- **Runtime** 是执行宿主
- **Artifact** 是执行产物，不是聊天附件
- **Knowledge** 是任务与项目上下文，不是 IM 历史的副产物

---

## 六、目标架构：双交付形态，同一套 Rust Core + 可远程 Runtime Host

### 6.1 核心原则

**一套 Rust 控制面，两种产品交付形态，同一个前端，同一个 HTTP/WS 协议；CLI agent runtime 通过 RuntimeBackend 抽象接入，可本机嵌入，也可部署在另一台机器。**

```
                    ┌──────────────────────────────┐
                    │     Frontend (React/TS)        │
                    │     完全相同的代码              │
                    │  Tiptap, TurnCard, Jotai       │
                    └────────────┬─────────────────┘
                                 │ HTTP/WebSocket
                                 │ 完全相同的协议
                    ┌────────────▼─────────────────┐
                    │  multivac-core (Rust library)  │
                    │   所有业务逻辑                  │
                    │   Session, Task, Org,          │
                    │   Knowledge, Meeting,          │
                    │   Tool impls, RuntimeBackend   │
                    │   Orchest SDK 集成              │
                    ├────────────┬─────────────────┤
                    │  multivac-server (axum HTTP)   │
                    │  相同的 API handlers           │
                    │  相同的 WS hub                 │
                    └────────────┬─────────────────┘
                                 │ RuntimeBackend trait
                                 │ local impl / remote client
              ┌──────────────────┴──────────────────┐
              │                                      │
     ┌────────▼────────┐                  ┌─────────▼────────┐
     │  Mode 1: All-in-One              │  Mode 2: Cloud SaaS │
     │                                  │                     │
     │  Tauri 桌面壳                     │  Docker / K8s       │
     │  multivac-core 嵌入              │  multivac-core 独立进程│
     │  SQLite                          │  Postgres           │
     │  Orchest 本地运行                │  Orchest 服务端运行  │
     │  RuntimeHost 本机嵌入             │  RuntimeHost 可远程  │
     │  LLM API key 用户自备            │  LLM API 平台统一    │
     └─────────────────┘                └────────────────────┘

                    ┌──────────────────────────────┐
                    │  multivac-runtime-host         │
                    │     独立执行平面（可选进程）     │
                    │  Claude Code / Codex / OpenCode│
                    │  PTY / ACP / sandbox / workspace│
                    └──────────────────────────────┘
```

**边界修正**：
- `axum` 负责产品控制面：HTTP API、前端 WebSocket、auth、org/project/task/session、审计与权限。
- Orchest 作为 Rust library 被 `multivac-core` 进程内调用；这里不需要 gRPC/Proto 映射。
- Claude Code、Codex、OpenCode 这类 CLI agent 不属于普通 CRUD 后端模块。它们是长时执行平面，通过 `RuntimeBackend` 暴露 `start/attach/permission/pause/terminate/events` 等能力。
- `RuntimeBackend` 有本机实现，也有远程实现。远程 runtime-host 可以通过 `tonic` gRPC（云内双向可达）或 runtime-host 主动连接的 WebSocket/reverse session（用户本机/NAT 后设备）接入。
- 前端、task supervisor、审计日志只消费 normalized `TaskEvent` / `RuntimeEvent`，不直接消费 Claude Code JSONL、Codex ACP、PTY stdout 等内部协议。
- **`multivac-core` 是 daemon-first 的库，不是 request-response server**（Kocoro 的 daemon 模型）。axum router 只是 trigger surface 之一：SessionManager 同样可以被定时任务、文件监听、IM ingress、MCP 调用唤醒。Desktop 模式下 agent 常驻后台，不随窗口关闭而消失——「server」暗示被动响应，「daemon」暗示主动的后台 agent 生命周期，所有模块按后者设计。

### 6.2 为什么 Tauri 而不是 Electron

| | Electron | Tauri |
|---|---------|-------|
| **后端语言** | Node.js | **Rust（与 multivac-core 同语言）** |
| **multivac-core 集成** | 需要 napi-rs bridge，跨语言 FFI | 直接 `cargo add`，零开销调用 |
| **Orchest SDK** | 需要 napi-rs 封装，类型转换 | 直接依赖，原生类型 |
| **二进制大小** | ~200MB (Chromium) | ~5MB (系统 WebView) |
| **内存** | ~200MB+ baseline | ~50MB baseline |
| **Rust 契合度** | 需要额外的 FFI 层维护 | **天然一体** |

Tauri 的 webview 使用系统 WebView（macOS WebKit, Windows WebView2），前端代码完全相同——Vite dev server 在开发时，构建产物嵌入 Tauri 发布。

### 6.3 Workspace 结构

```
multivac/
├── Cargo.toml                       # workspace root
├── crates/
│   ├── multivac-core/               # ★ 全部业务逻辑（lib crate）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs               # re-exports, 初始化入口
│   │       ├── api/                 # axum router + handlers
│   │       │   ├── mod.rs           # build_router(pool, config) -> Router
│   │       │   ├── auth.rs
│   │       │   ├── sessions.rs
│   │       │   ├── tasks.rs
│   │       │   ├── orgs.rs
│   │       │   └── knowledge.rs
│   │       ├── ws/                  # WebSocket hub
│   │       │   ├── mod.rs
│   │       │   ├── hub.rs
│   │       │   └── handler.rs
│   │       ├── session/             # agent run 生命周期
│   │       │   ├── mod.rs
│   │       │   ├── manager.rs
│   │       │   ├── identity.rs      # SessionIdentity：resume 判定
│   │       │   ├── state.rs
│   │       │   └── relay.rs
│   │       ├── auth/                # JWT, OAuth
│   │       ├── task/
│   │       ├── org/
│   │       ├── knowledge/
│   │       ├── meeting/
│   │       ├── agent/               # AgentFactory + EmployeeTaskBinding
│   │       ├── ingress/             # MessageIngress trait（外部 IM 入口预留）
│   │       ├── workflow/            # agent()/parallel() 编排原语（接口预留）
│   │       ├── runtime/             # RuntimeBackend trait + normalized TaskEvent
│   │       │   ├── mod.rs
│   │       │   ├── backend.rs       # RuntimeBackend trait
│   │       │   ├── events.rs        # TaskEvent schema
│   │       │   ├── local.rs         # embedded/local runtime host adapter
│   │       │   └── remote.rs        # gRPC/WS runtime host client
│   │       ├── file_store/
│   │       ├── tool/                # product Tool impls + agent-task tools
│   │       │   ├── mod.rs
│   │       │   ├── knowledge_tools.rs
│   │       │   ├── task_tools.rs
│   │       │   ├── meeting_tools.rs
│   │       │   └── agent_task_tools.rs
│   │       └── db/                  # ★ 抽象 trait + 多后端
│   │           ├── mod.rs
│   │           ├── trait.rs         # MultivacDb trait
│   │           ├── postgres/        # sqlx Postgres impl
│   │           │   ├── mod.rs
│   │           │   ├── pool.rs
│   │           │   └── migrations/
│   │           └── sqlite/          # sqlx SQLite impl
│   │               ├── mod.rs
│   │               ├── pool.rs
│   │               └── migrations/
│   │
│   ├── multivac-server/             # ★ Mode 2: Cloud binary
│   │   ├── Cargo.toml              # depends on multivac-core with postgres feature
│   │   └── src/
│   │       └── main.rs              # axum::serve(multivac_core::build_app(...).router)
│   │
│   ├── multivac-desktop/            # ★ Mode 1: Tauri binary
│   │   ├── Cargo.toml              # depends on multivac-core with sqlite feature, tauri
│   │   ├── tauri.conf.json
│   │   ├── src/
│   │   │   └── main.rs              # tauri::Builder + multivac_core::build_router(sqlite_pool)
│   │   └── icons/
│
│   └── multivac-runtime-host/       # ★ 可选：远程 CLI agent 执行平面
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs               # gRPC server or reverse WS client
│           ├── pty_runtime.rs        # Claude Code: spawn CLI + JSONL/PTY adapter
│           ├── acp_runtime.rs        # Codex/OpenCode: ACP/JSON-RPC stdio adapter
│           ├── sandbox.rs            # Docker/local sandbox
│           └── workspace.rs          # workspace sync + file events
│
├── frontend/                        # React/TypeScript (同一份代码)
│   ├── package.json
│   ├── vite.config.ts
│   ├── src/
│   │   ├── components/
│   │   │   ├── chat/                # TurnCard, SessionViewer, ActivityGroup
│   │   │   ├── annotations/         # Island menu, overlay layer
│   │   │   ├── overlays/            # Fullscreen preview
│   │   │   ├── panels/              # Panel stack
│   │   │   ├── org/
│   │   │   ├── tasks/
│   │   │   └── knowledge/
│   │   ├── stores/                  # Jotai atom families
│   │   └── api/                     # HTTP/WS client (single backend URL config)
│   │       ├── client.ts
│   │       └── ws.ts
│   └── index.html
│
├── skills/                          # SKILL.md files
└── docs/
```

### 6.4 关键抽象：数据库 trait

```rust
// crates/multivac-core/src/db/trait.rs

#[async_trait]
pub trait MultivacDb: Send + Sync + 'static {
    // User
    async fn create_user(&self, email: &str, name: &str) -> Result<User>;
    async fn find_user_by_email(&self, email: &str) -> Result<Option<User>>;

    // Session
    async fn create_session(&self, user_id: UserId, title: &str) -> Result<Session>;
    async fn list_sessions(&self, user_id: UserId) -> Result<Vec<Session>>;
    async fn append_message(&self, session_id: SessionId, msg: &Message) -> Result<()>;
    async fn get_messages(&self, session_id: SessionId) -> Result<Vec<Message>>;

    // Task
    async fn create_task(&self, org_id: OrgId, task: &NewTask) -> Result<Task>;
    // ...

    // Org, Knowledge, Meeting, etc.
    // ...
}

// Each method is a single, well-defined query.
// Postgres and SQLite impls live in separate modules behind the same trait.
```

**设计决策**：
- 不是每个模块一个 trait（`SessionRepo`, `TaskRepo`）——避免 trait 爆炸。一个 `MultivacDb` trait，~30 个方法，每个方法对应一个 SQL 查询
- 如果方法数超 50，按领域拆分为 `MultivacDb: SessionStore + TaskStore + OrgStore + ...`，但初期不拆
- 不引入 ORM——Postgres 和 SQLite impls 各自手写 sqlx 查询，共享同一个 trait 签名
- Migration 文件各自维护（Postgres 和 SQLite 的 SQL 方言不同），但 schema 逻辑保持一致

### 6.5 关键抽象：应用构造器

```rust
// crates/multivac-core/src/lib.rs

pub struct MultivacApp {
    pub router: axum::Router,
    pub ws_hub: Arc<WsHub>,
    pub session_manager: Arc<SessionManager>,
    pub runtime_backend: Arc<dyn RuntimeBackend>,
}

/// Build the full axum router + all services.
/// Both multivac-server (cloud) and multivac-desktop (tauri) call this.
pub async fn build_app(
    db: Arc<dyn MultivacDb>,
    orchestr_config: OrchestConfig,  // model adapter, tool registry, skill loader
    app_config: AppConfig,           // auth secrets, file store path, runtime config
) -> Result<MultivacApp> {
    let ws_hub = Arc::new(WsHub::new());
    let runtime_backend = build_runtime_backend(&app_config.runtime).await?;
    let tool_registry = build_tool_registry(
        db.clone(),
        runtime_backend.clone(),
        app_config.clone(),
    ).await?;
    let skill_loader = SkillLoader::from_dir(&app_config.skills_dir)?;
    let model_adapter = build_model_adapter(&orchestr_config)?;

    let session_manager = Arc::new(SessionManager::new(
        db.clone(),
        tool_registry,
        skill_loader,
        model_adapter,
        runtime_backend.clone(),
        ws_hub.clone(),
    ));

    let router = api::build_router(db, session_manager.clone(), ws_hub.clone(), app_config);

    Ok(MultivacApp { router, ws_hub, session_manager, runtime_backend })
}
```

**Daemon 语义**：`MultivacApp` 本质是一个 daemon，不是一个 HTTP server。`build_app()` 返回后，router 只是被动入口之一；`session_manager` 同时被多种 TriggerSource 驱动——用户消息、cron 调度、文件监听（fsnotify + debounce）、IM ingress、MCP 调用。事件经 `ws_hub` 广播给所有 subscriber（前端 WS、审计日志、IM renderer），系统不假设「有浏览器连接才工作」。

### 6.6 关键抽象：RuntimeBackend

```rust
// crates/multivac-core/src/runtime/backend.rs

#[async_trait]
pub trait RuntimeBackend: Send + Sync + 'static {
    async fn start_task(&self, request: StartAgentTask) -> Result<RuntimeHandle>;
    async fn attach_task(&self, task_id: TaskId, input: AgentTaskInput) -> Result<()>;
    async fn respond_permission(&self, request_id: PermissionRequestId, response: PermissionResponse) -> Result<()>;
    async fn pause_task(&self, task_id: TaskId) -> Result<()>;
    async fn terminate_task(&self, task_id: TaskId, reason: TerminationReason) -> Result<()>;
    async fn task_status(&self, task_id: TaskId) -> Result<RuntimeProjection>;
    async fn subscribe_events(&self, task_id: TaskId) -> Result<TaskEventStream>;
}
```

`RuntimeBackend` 是 Multivac 作为 execution control plane 而非聊天插件的根本边界——所有 Task 的 runtime projection、所有 artifact 的产生与消费、所有 workspace 的变更，都通过这个 trait 流入系统真相。

**`StartAgentTask` 完整契约**（吸收 MIMO `SpawnInput` 的验证过的字段，这是 spawn 不退化为 fire-and-forget 的关键）：

```rust
pub struct StartAgentTask {
    pub task_id: TaskId,
    pub agent_config: AgentConfig,
    pub prompt: String,

    pub context_mode: ContextMode,            // 子 agent 看到多少父上下文
    pub output_format: Option<OutputFormat>,  // 结构化输出 schema
    pub lifecycle: AgentLifecycle,            // Ephemeral | Persistent（可 idle/resume）
    pub background: bool,                     // true → 完成后 Inbox 通知，不阻塞父 agent
    pub parent_actor_id: Option<ActorId>,
    pub gate: Option<CompletionGate>,         // 独立 judge model 验收
}

pub enum ContextMode {
    Full,       // 完整对话历史 + project knowledge
    Diff,       // 仅 checkpoint + project knowledge + 当前 task
    TaskOnly,   // 仅 task 描述 + project knowledge
}

pub struct CompletionGate {
    pub judge_model: ModelRef,       // 更小/更便宜的模型
    pub criteria: Vec<String>,
}
```

- `ContextMode` 解决「长对话中 sub-agent 浪费大量 token」——大多数委派任务用 `Diff` / `TaskOnly` 即可。
- `background: true` 的 task 完成后进 Inbox（见前端设计），父 agent 不被阻塞；`task_id` 绑定意味着 spawn 即 task 进入 running——结构性副作用，不依赖模型自觉。
- `gate` 判定未通过时，task 的 `reportedStatus` 降级为 `partial` / `blocked`，父 agent 与前端都收到不完整信号，而不是被「子 agent 说自己做完了」欺骗。
- 子 agent 继承**父 agent 的 permission ruleset**（而非自己声明的），保证委派链上权限语义一致。

`task_status()` 返回 `RuntimeProjection` 而不是存在 Task 表里——runtime projection 是瞬时执行状态（idle / working / waiting_for_user / processing_tools），由 RuntimeBackend 实时计算。不持久化避免了「数据库状态与实际执行不一致」的问题。

- 对模型暴露的是 `start_agent_task` / `attach_agent_task` / `respond_permission` 等 product tools，不暴露 `run_claude_code` 这种 vendor-specific tool。
- `cli-task-dispatch` skill 决定何时创建或附加 CLI task；`task-supervisor` skill 决定何时注入指令、批准、暂停或终止。
- `PtyRuntime` 负责 Claude Code：spawn `claude`，读取 JSONL / PTY 输出并转译为 `TaskEvent`。
- `AcpRuntime` 负责 Codex / OpenCode：spawn ACP adapter subprocess，通过 JSON-RPC stdio 转译事件与权限请求。
- `RemoteRuntimeBackend` 只关心远程协议，不关心具体 CLI。远程节点可以在云端沙箱，也可以在用户本机。

### 6.7 编排深度：spawn 契约之上的四个机制

这是 Multivac 与「调 API 的聊天工具」的根本差异层（MIMO 验证），其中 loop 级机制是对 Orchest SDK 的需求输入，产品级机制在 multivac-core 实现：

**1. Hook ReAct 循环**（Orchest 需求）。Hook 不只是通知——`on_run_end` 的返回值可以重新驱动 agent：

```rust
pub enum HookDecision {
    Continue { reason: String },  // 注入 reason 再跑一轮
    Stop,                          // 正常结束
    Escalate,                      // 升级到父 agent / 用户
}
```

agent loop 在 `on_run_end` 收到 `Continue` 时注入 reason 重新进入循环，硬上限 `MAX_REACT = 3`，每次 Continue 的 reason 写入审计。这让「stop-guard 检查 progress 后阻止乐观停止」「gate 未通过自动返工」成为 hook 而不是产品 if/else。

**2. Completion Gate**。`gate_eligible` 的 task 完成时由独立 judge model（更小更便宜）按 criteria 验收。gate 状态进入 `TaskEvent` 流，前端可展示「验收中 / 未通过理由 / agent 继续返工」。

**3. Checkpoint-writer 与多级压缩**（Kocoro + MIMO 合流）。被动 `AuditHook` 只负责 transcript 持久化；上下文管理由专门的 **checkpoint-writer 子 agent** 承担：

- 在独立 child session 后台运行，只写 checkpoint 文件，不污染主对话
- 基于 context window 使用率决策何时写（不是固定间隔）
- **任何压缩发生前先写 memory**（Kocoro 的 write-before-compact 模式：PersistLearnings → 再压缩）
- Context reconstruction：逼近窗口上限时，重建 = 最新 checkpoint（token-budgeted，按 section 优先级分配预算）+ project memory + task progress + 最近 N 条消息

v0 可以先做单路径 compaction + checkpoint 文件，但 `TaskEvent` 和 knowledge trait 不堵死这条演进路。

**4. AgentFactory：agent 是可编程资源**。

```rust
#[async_trait]
pub trait AgentFactory: Send + Sync {
    /// 描述 → 完整 AgentConfig（system_prompt + tool allowlist + permission + model 推荐）
    async fn generate(&self, description: &str, org_id: OrgId) -> Result<AgentConfig>;
}
```

两个消费方：(a) 用户在前端用自然语言创建 Employee，确认后落库；(b) **primary agent 在运行时为委派任务动态创建特化子 agent**——领域知识进 system prompt、tool allowlist 精准匹配任务，而不是把所有知识塞进 task prompt。Employee 不是静态配置：`EmployeeTaskBinding` 记录 Employee 当前承接的 task，`AgentConfig::with_task_context(&task)` 按 task 派生运行配置——Employee 是「可承接 task、拥有 runtime context 的执行实体」，不是「会聊天的 bot」（feishu.md 的战略要求）。

远期（接口不堵死）：**Dream**（从 session traces 提取持久知识更新 project memory）与 **Distill**（重复手动工作流自动打包为 skill）。

### 6.8 双层权限模型

两层互补，不混用（Kocoro 经验）：

| 层 | 适用对象 | 机制 |
|----|---------|------|
| **Approval 枚举** | structured product tools | per-tool 的 `Never / WhenRisky / Always`，配合 per-session 的 Explore / Ask / Auto 模式 |
| **命令前缀深度** | shell / CLI 执行 | `git → depth 1`（`git status` 放行，`git push` 询问）；hard-block patterns 优先；默认 ASK |

per-agent ruleset 三层 merge：defaults → agent 默认 → user override，pattern 支持 glob（`"*.env": "deny"`）。子 agent 继承父 ruleset（见 6.6）。workspace 初始化时拒绝危险根目录（系统根、Home 根、temp 根、Desktop/Downloads、卷根——lark-bridge 的 deny-list，朴素但挡住了大多数真实事故）。

### 6.9 外部 IM 接入：MessageIngress

IM 是 distribution surface，不是产品定义（feishu.md）——但接入层的**内部状态必须硬**（lark-bridge 范式）：

```rust
#[async_trait]
pub trait MessageIngress: Send + Sync {
    async fn ingest(&self, scope: ScopeId, msg: IngressMessage) -> Result<IngestResult>;
    async fn queue_status(&self, scope: ScopeId) -> Result<QueueStatus>;
}

pub enum IngressMessage {
    Command { text: String, sender: SenderId },     // 跳过 debounce，有中断优先级
    UserMessage { text: String, sender: SenderId }, // debounce 合并
}
```

设计约束（全部来自 lark-bridge 被验证的模型）：

- **输入按 IM 真实行为建模**：同 scope 短时 debounce 合并；运行中新消息排队到下一轮；同 scope 同时只允许一个 active run；全局并发上限
- **身份先于恢复**：resume 判定走 `SessionIdentity`（scope + agent + workspace_fingerprint + policy_fingerprint），不是 chatId → sessionId 的脆弱映射
- **回投走状态机**：`TaskEvent → TurnState → renderCard(state)`，平台 payload 限制集中在 renderer 消化（与前端 renderWeb 共用 TurnState，见前端设计 §五）
- **回调防伪造**：交互卡片按钮签 runId + scope + operator + action + policy_fingerprint + 过期 + nonce，HMAC + replay 防护
- adapter 本身尽量薄，不在 v0 实现——但 `ingress/` 模块和上述 trait 现在就定型，避免后补时侵入 session 模块

### 6.10 两个产品 Binary 的差异

```rust
// ===== Mode 2: Cloud (multivac-server/src/main.rs) =====

#[tokio::main]
async fn main() -> Result<()> {
    let config = load_cloud_config()?;  // from env vars
    let pg_pool = PgPool::connect(&config.database_url).await?;
    sqlx::migrate!("../multivac-core/src/db/postgres/migrations")
        .run(&pg_pool).await?;

    let db: Arc<dyn MultivacDb> = Arc::new(PostgresDb::new(pg_pool));
    let app = multivac_core::build_app(db, config.orchest, config.app).await?;

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app.router).await?;
    Ok(())
}

// ===== Mode 1: All-in-one (multivac-desktop/src/main.rs) =====

#[tokio::main]
async fn main() -> Result<()> {
    let config = load_desktop_config()?;  // from ~/.multivac/config.toml
    let sqlite_pool = SqlitePool::connect(&config.database_path).await?;
    sqlx::migrate!("../multivac-core/src/db/sqlite/migrations")
        .run(&sqlite_pool).await?;

    let db: Arc<dyn MultivacDb> = Arc::new(SqliteDb::new(sqlite_pool));
    let app = multivac_core::build_app(db, config.orchest, config.app).await?;

    // Tauri: embed the axum router into a local HTTP server, then open webview
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;  // random port
    let port = listener.local_addr()?.port();

    tokio::spawn(async move {
        axum::serve(listener, app.router).await.unwrap();
    });

    tauri::Builder::default()
        .setup(move |app| {
            // Point webview to local axum server
            let window = app.get_webview_window("main").unwrap();
            window.eval(&format!("window.__BACKEND_URL__ = 'http://127.0.0.1:{}'", port))?;
            Ok(())
        })
        .run(tauri::generate_context!())?;

    Ok(())
}
```

**两个产品 binary 的差异只在**：
1. 数据库后端（Postgres vs SQLite）
2. 端口绑定（0.0.0.0:8080 vs 127.0.0.1:random）
3. 配置来源（环境变量 vs 本地配置文件）
4. 进程外壳（裸 tokio vs Tauri webview）
5. RuntimeBackend 默认值（Desktop 默认 embedded/local；Cloud 默认 remote/managed）

**multivac-core 知道 deployment 与 runtime placement，但不关心具体 CLI 内部协议。** Claude Code JSONL、Codex ACP、PTY daemon stdin 都被限制在 runtime-host 实现里。

### 6.11 前端：两种模式下的连接策略

```typescript
// frontend/src/api/client.ts

// All-in-one mode: Tauri sets window.__BACKEND_URL__ before page load
// Cloud mode: configurable via login screen or env var
const BACKEND_URL = (window as any).__BACKEND_URL__
    ?? import.meta.env.VITE_BACKEND_URL
    ?? 'https://api.multivac.dev';

const ws = new WebSocket(BACKEND_URL.replace('http', 'ws') + '/ws');
```

前端不区分模式——它只知道一个 `BACKEND_URL`。开发时指向 `localhost:5173`（Vite proxy to Rust），生产时指向 Tauri 本地端口或云端 URL。

### 6.12 两种模式的行为差异

| 行为 | All-in-One (Tauri) | Cloud SaaS |
|------|-------------------|------------|
| **Orchest agent loop** | 用户本机 | 云端服务器 |
| **CLI agent runtime** | embedded local runtime-host | remote/managed runtime-host |
| **LLM API 调用** | 用户自己的 API key 直连 | 平台统一管理，用户不感知 |
| **User Shell PTY** | 用户本机 bash/zsh | 云端沙箱或浏览器连接的 remote shell |
| **文件访问** | 用户本地文件系统 | 云端 workspace 目录 |
| **数据库** | 本地 SQLite 文件 | 云端 Postgres |
| **多设备同步** | 不支持（本地数据） | 支持 |
| **离线工作** | 支持（LLM 调用除外） | 不支持 |
| **协作** | 不支持 | 支持 |
| **数据隐私** | 完全本地 | 云端存储 |
| **安装** | 下载 .dmg/.msi | 打开浏览器 |
| **升级** | Tauri updater | 服务端部署 |

### 6.13 AppConfig 的模式差异

```rust
// crates/multivac-core/src/lib.rs

pub struct AppConfig {
    // 两种模式都有的
    pub skills_dir: PathBuf,
    pub file_store_root: PathBuf,

    // 模式差异通过 enum 表达，不是 bool
    pub deployment: DeploymentMode,
    pub runtime: RuntimeConfig,
    pub features: FeatureFlags,
}

pub enum DeploymentMode {
    Desktop {
        data_dir: PathBuf,          // ~/.multivac/
    },
    Cloud {
        s3_bucket: Option<String>,
        multi_tenant: bool,
    },
}

pub enum RuntimeConfig {
    Embedded {
        shell: String,
        workspace_root: PathBuf,
        sandbox: SandboxMode,
    },
    RemoteGrpc {
        endpoint: String,
        auth_token_env: String,
    },
    ReverseWebSocket {
        bind_url: String,
        runtime_id: RuntimeHostId,
    },
}

pub enum SandboxMode {
    None,
    Docker { image: String },
}

pub struct FeatureFlags {
    pub collaboration: bool,        // cloud only
    pub offline_mode: bool,         // desktop only
    pub multi_device_sync: bool,    // cloud only
}
```

`multivac-core` 的代码通过 `DeploymentMode` 处理产品形态差异，通过 `RuntimeConfig` 处理执行平面位置差异。绝大多数模块（api handlers, session manager, ordinary product tools）不关心具体 CLI backend。

**Desktop 运维基线**（lark-bridge 经验：本地工具最大的敌人不是吞吐，而是「偶发坏掉且难定位」）：
- Profile 隔离目录：`~/.multivac/profiles/<name>/{config,sessions.db,workspace,logs,runtime}`，多 profile 互不污染
- 状态文件原子写（tmp + fsync + rename）；进程 registry 带 lock + stale prune
- JSONL 结构化日志，trace_id 贯穿全链路，token/secret/path 自动脱敏——为 `/doctor` 类自诊断准备稳定日志基座

### 6.14 技术栈选择

| 层 | 技术 | 两种模式的差异 |
|----|------|--------------|
| **桌面壳** | Tauri 2.x | 仅 Mode 1 |
| **HTTP 框架** | axum 0.8 | 相同 |
| **WebSocket** | axum::extract::ws | 相同 |
| **数据库** | sqlx 0.8 (Postgres + SQLite) | trait MultivacDb，两个 impl |
| **Auth** | jsonwebtoken + OAuth2 | 相同；Mode 1 可选跳过 OAuth |
| **序列化** | serde + serde_json | 相同 |
| **RuntimeBackend** | local impl / `tonic` gRPC / reverse WebSocket | Desktop 默认 local；Cloud 可 remote |
| **CLI agent runtime** | portable-pty + JSONL / ACP JSON-RPC stdio | runtime-host 内部实现 |
| **LLM 调用** | Orchest ModelAdapter | 相同；Mode 1 API key 来自用户本地配置 |
| **文件存储** | 本地 fs / object_store(S3) | trait FileStore |
| **日志/追踪** | tracing + tracing-subscriber | 相同 |
| **前端** | React 18 + Vite + Tiptap + Jotai | **完全相同** |

---

## 七、重构阶段

### 阶段 0：基础设施（先做，不依赖任何外部条件）

1. **Rust workspace**：
   - `multivac-core` lib crate + `MultivacDb` trait + Postgres impl
   - axum router + WebSocket echo handler
   - sqlx Postgres migrations（从零设计 6 张核心表）
   - CI: `cargo test --workspace`, `cargo clippy`, `cargo fmt`

2. **前端 scaffold**：
   - Vite + React 18 + Tiptap + Tailwind + Jotai
   - `api/client.ts` 连接 Rust backend（单 BACKEND_URL 配置）
   - WebSocket 连接 + 事件类型定义
   - 空壳 TurnCard + SessionViewer 组件

3. **数据库 Schema**（从零设计，不迁移 Prisma）：
   - `users`, `orgs`, `org_members`
   - `sessions`, `messages`
   - `tasks`, `task_events`
   - `knowledge_docs`（source enum `File | TaskOutput | MeetingTranscript` 实现 `Display`/`FromStr`，统一为 `knowledge://` URI——前端和 skill 用同一个字符串引用知识）
   - `meetings`, `meeting_transcripts`
   - 每表 ≤15 个字段

### 阶段 1：核心业务 + Orchest 集成（依赖 Orchest v0.7+）

4. **Session Manager**：
   - `SessionManager::start_run()` 调用 `orchestr::AgentRun`
   - `WsRelayHook` 将 `RuntimeEvent` 转发到前端
   - `AuditHook` 持久化 transcript 到 `messages` 表
   - `SessionIdentity` 判定 resume 可行性——`scope_id + agent_id + workspace_fingerprint + policy_fingerprint` 相同才允许 resume，避免运行上下文变化后误续旧 session（policy_fingerprint 覆盖权限模式、sandbox、resource scope）
   - `PermissionHook` 实现 Explore/Ask/Auto per session + shell 命令的 prefix-depth 规则（§6.8）
   - Hook `on_run_end` 支持 `HookDecision::Continue`（ReAct 循环，cap 3，§6.7）

5. **Product Tools**：
   - 从 Python agent engine 的 50+ tools 选出核心 15-20 个 ordinary product tools
   - 新增 `start_agent_task` / `attach_agent_task` / `respond_permission` / `pause_agent_task` / `terminate_agent_task`
   - 按 `orchestr::Tool` trait 重新实现
   - 注册进 `ToolRegistry`

6. **RuntimeBackend + agent-task skills**：
   - 定义 `RuntimeBackend` trait 与 normalized `TaskEvent`，`StartAgentTask` 按 §6.6 完整契约定型（context_mode / lifecycle / background / gate 即使 v0 只实现部分，schema 先锁定）
   - 实现 embedded local runtime-host（先支持 Claude Code PtyRuntime）
   - 将原产品 `cli-task-dispatch` / `task-supervisor` 迁移为 Orchest Skill
   - Skill 通过 agent-task tools 操作 RuntimeBackend，不直接调用 PTY daemon

7. **MCP 集成**：
   - 配置驱动的 MCP server 连接
   - 与 product tool 共用 `ToolRegistry`

### 阶段 2：双模式交付

8. **SQLite impl**：
   - 实现 `MultivacDb` trait 的 SQLite 版本
   - SQLite migrations（与 Postgres schema 对齐）

9. **multivac-desktop (Tauri)**：
   - Tauri shell 配置
   - 嵌入 multivac-core，本地 axum 服务器
   - 默认使用 embedded local RuntimeBackend
   - Tauri 窗口指向本地端口
   - 本地配置文件管理（profile 隔离目录，§6.13）
   - **Claude Code 配置一键导入**：scan `~/.claude/` 的 agents/skills/commands/MCP configs → diff → 原子 apply（Kocoro 迁移路径；onboarding 关键杠杆，迁移成本降为零）

10. **multivac-server (Cloud)**：
   - Dockerfile
   - 环境变量配置
   - 健康检查端点
   - 默认使用 remote/managed RuntimeBackend

11. **multivac-runtime-host**：
   - `tonic` gRPC server（云内 runtime-host）
   - reverse WebSocket client（用户本机 runtime-host）
   - Claude Code PtyRuntime + TaskEvent 转译
   - Codex/OpenCode AcpRuntime 后续接入

### 阶段 3：前端重写（UI/UX 对齐 Craft Agents）

12. **Chat UI**：TurnCard + TurnPhase 状态机 + 流式缓冲
13. **Annotation 系统**：Island 菜单 + 追问 + overlay layer
14. **Multi-Panel + Permission + Overlay**：Panel stack, per-session toggle, fullscreen preview

### 阶段 4：差异化能力

15. **Task System**：独立于 Session 的 Task 生命周期 + Draft/Merge
16. **Knowledge Workspace**：文件监听 + 自动索引
17. **Meeting/ASR**：Meeting lifecycle + ASR Gateway
18. **编排深度**：Completion Gate + checkpoint-writer 子 agent + AgentFactory 运行时特化（§6.7）
19. **外部 IM 接入**：MessageIngress 飞书 adapter + `TurnState → renderCard` 渲染链 + 回调签名（§6.9）

---

## 八、职责边界：Orchest vs Multivac vs Runtime Host

| 职责 | Orchest SDK | multivac-core (Multivac) | multivac-runtime-host |
|------|------------|--------------------------|--------------------|
| Agent loop | ✅ AgentRun | — | — |
| Tool dispatch | ✅ ToolRegistry | 注册 product / agent-task tools | — |
| Skill loading | ✅ SkillLoader | 提供 `cli-task-dispatch` / `task-supervisor` / product skills | — |
| Model adapter | ✅ ModelAdapter | 配置 API key / endpoint | — |
| Budget enforcement | ✅ BudgetGuard | 用户级 quota | — |
| Approval gate | ✅ approval channel | UI 交互 + agent-task permission routing | 执行 runtime-specific permission response |
| Sub-agent | ✅ SubAgentBuilder | — | — |
| Mid-run steering | ✅ (v0.9) | UI 控制 + RuntimeBackend steering | runtime-specific steering |
| Normalized event contract | ✅ RuntimeEvent | ✅ TaskEvent 持久化、广播、审计 | ✅ CLI/ACP/PTY → TaskEvent 转译 |
| | | | |
| **HTTP/WS server** | — | ✅ axum router | reverse WS client/server only |
| **Auth (JWT, OAuth)** | — | ✅ auth/ module | runtime-host token / binding |
| **User/Org/Project** | — | ✅ org/ module | — |
| **Task lifecycle** | — | ✅ task/ module + RuntimeBackend handle | 执行 task runtime lifecycle |
| **Knowledge store** | — | ✅ knowledge/ module | workspace file event source |
| **Meeting/ASR** | — | ✅ meeting/ module | — |
| **CLI process execution** | — | — | ✅ Claude Code / Codex / OpenCode subprocess |
| **PTY/User shell** | — | User shell API + event relay | ✅ PTY daemon / sandbox |
| **File storage** | — | ✅ file_store/ module | workspace sync producer |
| **DB abstraction** | — | ✅ MultivacDb trait | — |
| **双模式编译** | — | ✅ multivac-server + multivac-desktop | optional deploy unit |
| **Tauri shell** | — | ✅ multivac-desktop only | — |

---

## 九、不做的事

| 不做什么 | 原因 |
|---------|------|
| **不保留 NestJS / Python agent-engine / Prisma / Zustand** | 产品控制面与 agent loop 全部替换为 Rust + Orchest |
| **不保留旧的混合 `agent_engine.proto`** | orchestration RPC 与 runtime-management RPC 混在一起；新设计拆成 Rust in-process Orchest 调用 + RuntimeBackend 远程协议 |
| **不把 gRPC 作为进程内 Orchest 调用边界** | `multivac-core` 与 Orchest 同为 Rust library，直接调用更简单 |
| **不禁止远程 runtime-host 使用 gRPC/WS** | CLI agent runtime 可以在另一台机器上，必须保留稳定远程执行协议 |
| **不保留 Channel 概念** | 改为 Session |
| **不自研 Lexical** | Tiptap 足够 |
| **不用 Electron** | Tauri 与 Rust backend 天然一体 |
| **不重写 iOS/Android** | 先做桌面 + Web |
| **不做 E2E 加密协作** | 先做简单多用户 |
| **不做 20+ extension** | 从 5 个 product tool 开始 |
| **不做文档中心的 agent 编辑模式** | Nimbalyst 的 Lexical + inline diff 是其 3 年护城河；对话中心 + TurnCard 才匹配 orchestration 定位（多 agent、推理链、追问） |
| **不做动态 JS extension marketplace** | product tool 编译期注册；marketplace 是远期差异化 |
| **不做图状记忆 sidecar** | 先做 session 级记忆 + checkpoint；图记忆（Kocoro TLM 式）是 v2，knowledge trait 接口不堵死 |
| **不和 IM 平台打入口层** | 消息入口、卡片容器、组织分发是平台必赢的地盘；防守 task/runtime/workspace/artifact/透明度（feishu.md） |

---

## 十、最高风险点

| 风险 | 缓解 |
|------|------|
| **Orchest v0.7+ 延期** | `session/manager.rs` 先写 fallback loop（直接调 Claude API），接口对齐 `AgentRun` 形状，后续切依赖 |
| **MultivacDb trait 方法爆炸** | 超过 50 个方法时按领域拆分为 `SessionStore + TaskStore + ...` |
| **SQLite vs Postgres SQL 方言差异** | 只用到 INSERT/SELECT/UPDATE/DELETE + 简单 JOIN，方言差异可控。Migration 文件各自维护 |
| **Tauri WebView 兼容性** | 前端只用标准 Web API；Tauri 的 WebView（WebKit/macOS, WebView2/Windows）对现代 JS 支持良好 |
| **RuntimeBackend 边界变胖** | 只暴露 task lifecycle、permission、event stream、workspace sync；Claude/Codex 内部协议留在 runtime-host |
| **远程 runtime-host 网络不可达** | 云内用 `tonic` gRPC；用户本机优先 reverse WebSocket，由 runtime-host 主动连控制面 |
| **CLI 事件协议不稳定** | 每个 backend 独立 adapter 转译为 normalized `TaskEvent`；前端和 supervisor 不依赖 Claude JSONL / ACP 原始格式 |
| **PTY / ACP 双后端复杂度** | 先做 Claude Code `PtyRuntime`，Codex/OpenCode `AcpRuntime` 后续接入；二者共用 RuntimeBackend contract |
| **ReAct hook 循环失控** | `MAX_REACT = 3` 硬上限；每次 `Continue` 必须带 reason 且写入审计 |
| **Checkpoint-writer 与主 loop 互相干扰** | checkpoint-writer 在独立 child session 运行，只写 checkpoint 文件，不注入主对话 |
| **后台 background task 静默失败** | Inbox 通知 + completion gate 的 `partial/blocked` 降级信号 + idle watchdog |

---

## 十一、立即行动项

1. **Init Rust workspace**: `cargo new --lib crates/multivac-core` + `cargo new crates/multivac-server` + `cargo new crates/multivac-desktop`
2. **Init frontend scaffold**: Vite + React + Tiptap + Jotai，连接 localhost axum
3. **Define `MultivacDb` trait**: 从 Core 6 张表的方法签名开始
4. **Implement axum WebSocket echo**: 前端连上，收到 ping/pong
5. **Define `RuntimeBackend` trait + `TaskEvent` schema**: 先锁定 start/attach/permission/events 最小合同；`StartAgentTask` 按 §6.6 完整契约定型（含 `ContextMode` / `CompletionGate` 类型）
6. **Design DB schema**: 6 张核心表的 CREATE TABLE SQL（Postgres 先，SQLite 后续对齐）
7. **迁移 agent-task skills 设计**: 将 `cli-task-dispatch` / `task-supervisor` 对齐到 Orchest Skill + ToolRegistry
8. **确认 Orchest v0.7 timeline**: 决定 session/manager.rs 初版是 fallback loop 还是直接 AgentRun
