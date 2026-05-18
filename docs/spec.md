# Skill-First Agent Runtime: 技术设计

> 核心概念与设计哲学见 [overview.md](./overview.md)。

## 长时异步 Tool 设计

视频生成、音频合成、大规模数据处理等 tool 需要几秒到数十分钟才能完成，不能用 `execute().await` 阻塞 loop。本 runtime 用**两阶段执行模型**处理这类 tool。

### 设计模型

`execute()` 的返回类型从 `Result<Value, ToolError>` 扩展为 `Result<ToolOutput, ToolError>`：

```rust
pub enum ToolOutput {
    Immediate(Value),     // 同步完成，直接返回结果
    AsyncJob(JobHandle),  // 异步提交，runtime 接管轮询
}

pub struct JobHandle {
    pub job_id: String,
    pub poll: Arc<dyn Fn() -> BoxFuture<'static, Result<JobStatus, ToolError>> + Send + Sync>,
    pub poll_interval: Duration,
    pub timeout: Option<Duration>,
}

pub enum JobStatus {
    Pending {
        progress: Option<f32>,    // 0.0–1.0，可选
        message: Option<String>,  // 进度描述，可选
    },
    Completed(Value),
    Failed(String),
}
```

Tool 的 `execute()` 只负责"提交任务"——调用外部 API、写入队列、发起渲染请求——然后立即返回 `AsyncJob(handle)`。真正的等待交给 runtime 处理。

### Runtime 行为

当 tool 返回 `AsyncJob` 时：

1. RunStatus 转为 `WaitingForAsyncTool { tool_call, job_handle, since }`
2. Runtime 在 background tokio task 里按 `poll_interval` 轮询 `job_handle.poll()`
3. 每次轮询发出 `AsyncToolProgress` 事件（含 progress 和 message）
4. 收到 `Completed` 或 `Failed` 后，将结果注入消息流，恢复 loop
5. 若超过 `timeout`，tool 视为失败，发出 `ToolCallFailed` 事件

Budget guard 的 `max_duration` 计入异步等待时间，防止 run 因长时 tool 无限挂起。

### 适用范围

这个模型对所有 ToolSource 均适用：
- **InProcess Tool**：提交云端任务，返回 job ID 和 poll 闭包
- **MCP Tool**：MCP server 侧实现轮询逻辑，SDK 侧封装成 `JobHandle`
- **Skill bundled script**：脚本输出 `{"__async_job": true, "job_id": "...", "poll_interval": 5}` 到 stdout，runtime 识别后切换轮询模式；轮询通过重新执行脚本并传入 `--poll <job_id>` 参数实现

Tool 作者只需要描述"提交 + 返回 handle"的行为，runtime 统一处理轮询和恢复。

## 原生流式输出

Agent 的最终答案通常是文本，用户需要看到 token 逐字输出，而不是等全文生成完才显示。流式输出是一等公民，不是可选附加。

### 模型层 Streaming

```rust
pub trait ModelAdapter: Send + Sync {
    // stream() 是 loop 内部使用的主路径
    // call() 是 stream() 的 convenience wrapper，收集完整 response 后返回
    async fn call(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<ModelResponse, ModelError>;

    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<ModelResponse, ModelError>;
}

pub enum ModelStreamChunk {
    Text { delta: String },
    Thinking { delta: String },                      // extended thinking
    ToolCallArgsChunk { id: String, delta: String }, // tool call 参数增量（仅供 UI 用，loop 仍从完整 response 解析 tool call）
    Done { usage: TokenUsage },
}
```

Loop 内部始终使用 `stream()`，完整 response 在流结束后一次性处理（保持 tool call 解析的简单性）。每个 `ModelStreamChunk` 封装成 `RuntimeEvent::ModelStreamChunk` 发出，SDK 的 async iterator 实时透传给用户。

### Tool 流式更新

Tool 执行中也可以推送中间结果（如 web search 命中的第一条、代码执行的 stdout 增量）：

```rust
pub struct ToolContext {
    pub run_id: RunId,
    pub tool_call_id: String,
    pub on_update: Option<mpsc::Sender<Value>>, // runtime 创建并持有 receiver，转发为 ToolCallUpdate 事件
}
```

Tool 通过 `ctx.on_update.send(partial)` 推送，runtime 封装成 `ToolCallUpdate` 事件发出。不发 update 的 tool 行为不变——这是可选扩展，不是必须实现的接口。

## 架构概览

```
                    ┌──────────────────────────────────┐
                    │  Rust Agent Runtime Core          │
                    │                                   │
                    │  - Agent run loop (streaming)     │
                    │  - State management (RunState)    │
                    │  - Skill discovery & loading      │
                    │  - Tool dispatch                  │
                    │  - Async job polling              │
                    │  - Model adapters (streaming)     │
                    │  - Event stream                   │
                    │  - Budget guard                   │
                    └──────────────────────────────────┘
                              ▲
                              │  FFI (PyO3 / napi-rs)
                              │
                ┌─────────────┴─────────────┐
                │                           │
                ▼                           ▼
        ┌──────────────┐            ┌──────────────┐
        │ Python SDK   │            │  TS SDK      │
        │              │            │              │
        │ - Agent API  │            │ - Agent API  │
        │ - @tool dec  │            │ - tool() reg │
        │ - Async iter │            │ - AsyncIter  │
        └──────────────┘            └──────────────┘
                │                           │
                └─────────────┬─────────────┘
                              ▼
        ┌──────────────────────────────────────┐
        │  User application                     │
        │                                       │
        │  Tools 来源：                          │
        │  ① 用户应用直接注册的 in-process tool  │
        │  ② 远程 MCP server（v0.2）             │
        │  ③ Skill bundled scripts              │
        │                                       │
        │  Skills（文件系统目录）                 │
        └──────────────────────────────────────┘
```

执行流：

1. 用户应用通过 SDK 创建 Agent，注册 tools，扫描 skills 目录
2. 用户调用 `agent.run(input)`，得到一个事件流（async iterator）
3. Runtime 在 Rust core 里执行 agent loop
4. Agent 通过 file read tool 按需读取 skill 内容
5. Agent 通过统一接口调用 tool，runtime 路由到对应后端
6. 每一步发出事件，用户消费做日志、UI、审计

## 核心实体

### Tool

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> &JsonSchema;
    fn output_schema(&self) -> Option<&JsonSchema>;
    fn metadata(&self) -> &ToolMetadata;
    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
}

pub enum ToolOutput {
    Immediate(Value),     // 同步完成
    AsyncJob(JobHandle),  // 长时任务，runtime 接管轮询（见"长时异步 Tool 设计"）
}

pub struct ToolMetadata {
    pub side_effect: bool,
    pub requires_approval: bool,
    pub cost_hint: Option<CostHint>,
    pub timeout: Option<Duration>,  // 适用于 execute() 调用本身（提交阶段）；AsyncJob 的等待超时在 JobHandle.timeout
    pub source: ToolSource,
}

pub enum ToolSource {
    InProcess,                          // 用户 SDK 注册，通过 FFI callback 调用
    McpServer { server_id: String },    // 远程 MCP server，通过 MCP 协议调用（v0.2）
    Skill { skill_name: String },       // skill bundled 脚本，spawn 子进程执行
    Builtin,                            // runtime 内置（如 read_file）
}
```

不同后端的 tool 共享同一个 trait，区别只在 `execute` 的实现。

### Skill

Skill 是文件系统目录，runtime 启动时扫描指定路径下的所有 SKILL.md 文件。

```rust
pub struct SkillManifest {
    pub name: String,
    pub description: String,                    // 进入 system prompt（约 80 token）
    pub path: PathBuf,                          // skill 目录路径
    pub allowed_tools: Option<Vec<String>>,     // 可选的 tool 限制
    pub bundled_tools: Vec<BundledTool>,        // SKILL.md 中声明的 script tool
    pub raw_frontmatter: Value,                 // 保留完整 frontmatter 供扩展
}

pub struct BundledTool {
    pub name: String,
    pub description: String,
    pub executable: String,     // python / node / bash 等
    pub script: PathBuf,        // 相对于 skill 目录的脚本路径
    pub input_schema: JsonSchema,
    pub metadata: ToolMetadata,
}
```

Skill 的加载通过 agent 主动读取实现——runtime 不做"加载 skill 进入 context"的特殊机制。Agent 看到 skill 列表（来自 system prompt），用 `read_file` 读取 SKILL.md，把内容自然地纳入推理。

skill 中声明的 `bundled_tools` 在启动时全部注册到 tool registry（选项 A），原因：与 Anthropic 官方一致，简化设计；实际场景 10-50 个 skill、每个 1-5 个 tool，总量可控；token 问题未来通过 Tool Search Tool 解决。

### Agent

Agent 是一次执行实例的配置：

```rust
pub struct AgentConfig {
    pub system_prompt: String,
    pub model: ModelSpec,
    pub budget: BudgetConfig,
    pub max_steps: u32,
    pub allowed_skills: Option<Vec<String>>,
    pub allowed_tools: Option<Vec<String>>,
    pub mcp_servers: Vec<McpServerConfig>,  // v0.2 生效
}
```

Agent 没有"角色模板"概念——agent 就是一次配置。复用由用户在 SDK 层封装。

### RunState

```rust
pub struct RunState {
    pub run_id: RunId,
    pub schema_version: &'static str,  // 序列化版本，为 schema 演进预留
    pub config: AgentConfig,
    pub messages: Vec<Message>,
    pub available_tools: Vec<Arc<dyn Tool>>,
    pub step: u32,
    pub status: RunStatus,
    pub budget_used: BudgetUsage,
}

pub enum RunStatus {
    Running,
    WaitingForApproval { tool_call: ToolCall },
    WaitingForAsyncTool {
        tool_call: ToolCall,
        job_handle: JobHandle,
        since: std::time::Instant,
    },
    Completed { output: Value },
    Failed { error: String },
    Aborted,
}
```

RunState 可以序列化到磁盘，支持长 session 中断后恢复。注意：`JobHandle.poll` 闭包不可序列化，跨进程恢复时异步 job 状态丢失，需要 tool 作者在重新 `execute()` 时处理幂等性。

### Runtime Event

```rust
pub enum RuntimeEvent {
    RunStarted { run_id: RunId },

    // 模型调用
    ModelCallStarted { step: u32 },
    ModelStreamChunk { delta: ModelStreamChunk },   // token 级流式输出
    ModelCallCompleted { tokens: TokenUsage },

    // Tool 执行（同步）
    ToolCallStarted { tool: String, source: ToolSource, input: Value },
    ToolCallUpdate { tool: String, tool_call_id: String, partial: Value },
    ToolCallCompleted { tool: String, output: Value, duration: Duration },
    ToolCallFailed { tool: String, error: String },

    // Tool 执行（异步 job）
    AsyncToolStarted { tool: String, job_id: String },
    AsyncToolProgress { tool: String, job_id: String, status: JobStatus },
    AsyncToolCompleted { tool: String, job_id: String, output: Value, elapsed: Duration },

    // Skill
    SkillContentRead { skill_name: String, file: String, tokens: u32 },

    // 审批
    ApprovalRequested { tool_call: ToolCall },
    ApprovalGranted { tool_call: ToolCall },
    ApprovalDenied { tool_call: ToolCall },

    // 预算 / 生命周期
    BudgetWarning { used: BudgetUsage, limit: BudgetConfig },
    RunCompleted { output: Value },
    RunFailed { error: String },
}
```

## 执行流程

整个 runtime 只有一个 loop：

```
loop {
    if step >= max_steps: return Aborted("max_steps_reached")
    if budget_exceeded: return Aborted("budget_exceeded")

    // 始终使用 stream()，ModelStreamChunk 通过 event channel 实时发出
    emit ModelCallStarted { step }
    let (tx, rx) = mpsc::channel()
    spawn { for chunk in rx { emit ModelStreamChunk { delta: chunk } } }
    let response = model.stream(state.messages, state.available_tools, tx).await?
    emit ModelCallCompleted { tokens: response.usage }

    match parse_response(response) {
        FinalAnswer(output) => {
            emit RunCompleted
            return Completed(output)
        }
        ToolCalls(calls) => {
            // v0.1 顺序执行，保持审批门和调试简单性
            for call in calls {
                if requires_approval(call) {
                    emit ApprovalRequested
                    // 通过 oneshot channel 等待用户调用 agent.respond_approval()
                    wait_for_approval()
                }

                emit ToolCallStarted
                match execute_tool(call).await {
                    ToolOutput::Immediate(value) => {
                        emit ToolCallCompleted
                        if is_skill_content(call) { emit SkillContentRead }
                        state.messages.push(tool_result_message(value))
                    }
                    ToolOutput::AsyncJob(handle) => {
                        state.status = WaitingForAsyncTool { call, handle, since: now() }
                        emit AsyncToolStarted { job_id: handle.job_id }
                        // background task 轮询，每次 poll 发 AsyncToolProgress
                        let result = poll_until_done(handle).await
                        emit AsyncToolCompleted
                        state.messages.push(tool_result_message(result))
                    }
                }
            }
        }
    }

    step += 1
}
```

System prompt 的构造：

```
[user-provided system prompt]

## Available Skills
The following skills are available. To use a skill, read its SKILL.md file
using the read_file tool.

- research_topic (skills/research_topic/SKILL.md): 用户需要调研某个主题、
  需要多源信息整合时使用。
- format_data (skills/format_data/SKILL.md): 处理 JSON、CSV 等数据格式
  转换时使用。
- ...
```

Agent 自然地决定"我需要调研，先读一下 research_topic 的 SKILL.md"——这是它平时就能做的事，不需要特殊的 `load_skill` API。

## MCP 集成（v0.2）

MCP server 通过配置接入：

```rust
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransport,
    pub auth: Option<McpAuth>,
}

pub enum McpTransport {
    Stdio { command: String, args: Vec<String> },
    Http { url: String },
    StreamableHttp { url: String },
}
```

Runtime 启动时连接所有配置的 MCP server，调用 `tools/list` 获取可用 tool 列表，注册到 tool registry。Agent 调用这些 tool 时，runtime 通过对应连接发送 `tools/call`。

v0.1 不实现，`AgentConfig.mcp_servers` 字段预留，registry 设计为后续扩展留口。未来支持 MCP 的 resource 和 prompt primitive 时再扩展。

## 安全与控制

**Approval Gate**：tool metadata 中标记 `requires_approval: true` 的调用会暂停 run，发出 `ApprovalRequested` 事件，通过内部 oneshot channel 等待用户调用 `run_handle.respond_approval(approved)` 唤醒。未响应时 run 持续挂起，直到 `max_duration` 触发。

**Budget Guard**：

```rust
pub struct BudgetConfig {
    pub max_tokens: Option<u64>,
    pub max_tool_calls: Option<u32>,
    pub max_duration: Option<Duration>,
    pub max_cost_usd: Option<f64>,
}
```

每次 model call 和 tool call 完成后检查。`max_duration` 包含异步 job 等待时间。

**Permission Boundary**：通过 `allowed_tools` 和 `allowed_skills` 限制 agent 可见范围。

**`read_file` 安全**：v0.1 不限制路径，但发出 `SkillContentRead` 事件记录所有读取行为，用于审计。未来可以通过 path allowlist（如仅允许 skills_dir 内路径）收紧。

**Skill Tool 隔离**：v0.1 不做 sandboxing，要求用户审核 skill 来源。未来通过 firejail/bubblewrap 等机制隔离。

## SDK 设计

### Python SDK

```python
from agent_runtime import Agent

agent = Agent(
    model="claude-3-5-sonnet",
    system_prompt="你是一个研究助手",
    skills_dir="./skills",
    mcp_servers=[                          # v0.2 生效
        {"name": "github", "transport": {"stdio": {"command": "mcp-server-github"}}},
    ],
    budget={"max_tokens": 100_000, "max_tool_calls": 50},
)

@agent.tool
async def read_local_file(path: str) -> str:
    """读取本地文件内容"""
    return open(path).read()

@agent.tool(requires_approval=True, side_effect=True)
async def write_file(path: str, content: str) -> None:
    """写入文件"""
    with open(path, 'w') as f:
        f.write(content)

async for event in agent.run("帮我研究 Rust 异步运行时，写一份对比报告"):
    match event.type:
        case "model_stream_chunk":
            print(event.delta.get("text", ""), end="", flush=True)
        case "tool_call_started":
            print(f"\n→ {event.tool} (from {event.source})")
        case "async_tool_progress":
            print(f"⏳ {event.tool} {int((event.status.get('progress') or 0) * 100)}%")
        case "skill_content_read":
            print(f"📚 read {event.skill_name}/{event.file}")
        case "approval_requested":
            approved = await ask_user(event.tool_call)
            await agent.respond_approval(event.run_id, approved)
        case "run_completed":
            print(f"\n✓ {event.output}")
```

### TypeScript SDK

```typescript
import { Agent } from "@yourname/agent-runtime"

const agent = new Agent({
  model: "claude-3-5-sonnet",
  systemPrompt: "你是一个研究助手",
  skillsDir: "./skills",
  budget: { maxTokens: 100_000, maxToolCalls: 50 },
})

agent.tool({
  name: "read_local_file",
  description: "读取本地文件内容",
  input: z.object({ path: z.string() }),
  handler: async ({ path }) => fs.readFile(path, "utf-8"),
})

// 长时异步 tool：提交任务后立即返回 job handle
agent.tool({
  name: "generate_video",
  description: "生成视频",
  input: z.object({ prompt: z.string() }),
  handler: async ({ prompt }) => {
    const { jobId } = await videoApi.submit(prompt)
    return {
      asyncJob: {
        jobId,
        pollIntervalMs: 5000,
        poll: async () => {
          const res = await videoApi.status(jobId)
          if (res.status === "done") return { completed: res.url }
          return { pending: { progress: res.progress } }
        },
      },
    }
  },
})

for await (const event of agent.run("帮我研究 Rust 异步运行时")) {
  switch (event.type) {
    case "model_stream_chunk":
      process.stdout.write(event.delta.text ?? "")
      break
    case "tool_call_started":
      console.log(`\n→ ${event.tool} (from ${event.source})`)
      break
    case "async_tool_progress":
      console.log(`⏳ ${event.tool} ${(event.status.progress ?? 0) * 100}%`)
      break
    case "skill_content_read":
      console.log(`📚 read ${event.skillName}/${event.file}`)
      break
    case "run_completed":
      console.log(`\n✓`, event.output)
      break
  }
}
```

## 实现路线

### v0.1（最小可用）

**Rust core**：
- Agent run loop（基于 tokio task + mpsc channel）
- Model streaming（`ModelAdapter::stream()`，`ModelStreamChunk` 事件）
- RunState 管理与序列化（含 `schema_version` 字段）
- Skill 目录扫描和 manifest 加载
- Tool registry（InProcess + Skill bundled）
- 异步 job 轮询（`ToolOutput::AsyncJob`，`WaitingForAsyncTool`）
- 一个 model adapter（Anthropic Claude）
- Builtin `read_file` tool
- Skill bundled script 子进程执行
- 完整 event 流（含流式 chunk 和异步 job progress）
- Budget guard
- Approval gate（oneshot channel 暂停/恢复）

**Python SDK**：PyO3 binding，async generator 事件流，decorator 风格 tool 注册，支持返回 async job dict。

**TypeScript SDK**：napi-rs binding，AsyncIterator 事件流，函数式 tool 注册，支持返回 `{ asyncJob: ... }` 对象。

**先不做**：
- MCP server 集成（v0.2）
- Tool Search Tool（v0.2）
- Context compaction（v0.2）
- 多 model adapter
- Multi-agent 协作
- Skill 沙箱

### v0.2

- MCP server 集成（stdio + Streamable HTTP transport）
- Tool Search Tool 风格的渐进式 tool 加载
- 第二个 model adapter（OpenAI）
- Context compaction（超长 session 自动摘要）
- Persistent script mode（如果脚本冷启动开销成为瓶颈）
- Webhook 模式异步 tool（作为 polling 的补充）

### v1.0 之前的开放问题

- **Skill 依赖管理**：skill 的 Python/Node 脚本需要特定依赖时，runtime 怎么准备环境？目前依赖用户全局安装，不优雅但简单
- **Code Execution as MCP**：是否支持 agent 写代码调用 tool 而非直接调用？这是 Anthropic 在推的高级模式，能大幅降低 token，但实现复杂度高
- **Skill sub-agent**：skill 是否能在内部启动一个 sub-agent？budget 怎么继承、event 怎么嵌套

## 目录结构

```
crates/
  agent-runtime-core/
    src/
      lib.rs
      run.rs                # AgentRun, RunState, run loop
      tool/
        mod.rs              # Tool trait, ToolRegistry, ToolOutput
        in_process.rs       # FFI callback tool
        skill_bundled.rs    # script tool（含 async job 协议解析）
        async_job.rs        # JobHandle, JobStatus, poll loop
        mcp.rs              # MCP tool（v0.2）
        builtin.rs          # read_file
      skill/
        mod.rs              # SkillManifest, discovery, SKILL.md 解析
      model/
        mod.rs              # ModelAdapter trait
        anthropic.rs
        openai.rs           # v0.2
        streaming.rs        # ModelStreamChunk, stream 公共逻辑
      events.rs
      budget.rs
  agent-runtime-py/         # PyO3 binding
    src/lib.rs
    python/agent_runtime/
      __init__.py
  agent-runtime-node/       # napi-rs binding
    src/lib.rs
    js/index.ts

skills/                     # 示例 skill（兼容 agentskills.io 标准）
  research_topic/
    SKILL.md
    scripts/
    references/
  format_data/

examples/
  python_basic.py
  python_async_tool.py      # 长时异步 tool 示例（视频生成）
  ts_basic.ts
  ts_streaming.ts           # 流式输出示例
```

## 设计决策记录

**为什么 skill 不通过特殊 API 加载，而是用 read_file**：与 Anthropic 官方实现保持一致。这让 skill 的"加载"是 agent 推理的自然延伸，而不是一个特殊的 runtime 操作。也保持了 SKILL.md 跨平台兼容性。

**为什么 MCP 是传输协议而非 tool 类型**：按官方定义，MCP 是 tool 的传输层。把 MCP server 当作 tool 的提供方之一，runtime 内部统一通过 `Tool` trait 看到所有 tool，区别只在 `ToolSource` 和 `execute` 实现。

**为什么 v0.1 不做 MCP**：MCP 是相对独立的功能模块。v0.1 先验证核心架构（agent loop + skill + 双语言 SDK），MCP 加入是 v0.2 的扩展工作。

**为什么 skill bundled tools 启动时全部注册**：与 Anthropic 官方一致，简化设计。当 tool 数量真的成为问题时，引入 Tool Search Tool 风格的渐进披露，不在 skill 加载逻辑里特殊处理。

**为什么是 Rust 核心**：跨语言 SDK 需要 in-process 嵌入而不是 IPC。Rust 的 PyO3/napi-rs 生态成熟，可以同时嵌入 Python 和 TypeScript 运行时，在性能、类型系统、AI 写代码友好度上是这个场景的最优解。

**为什么流式输出是 v0.1 核心**：所有主流框架都把 token streaming 作为默认行为，用户构建 chat UI 时是刚需。`ModelAdapter::stream()` 的实现工作量和 `call()` 接近（model provider SDK 都原生支持），没有理由推迟。

**为什么长时异步 tool 是 v0.1 优先级**：如果 runtime 不原生支持异步 job，用户只能在 tool 内部阻塞式等待，无法发出进度事件，且 budget `max_duration` 失效。`ToolOutput::AsyncJob` 让 tool 作者的代码自然——提交就返回，等待和恢复由 runtime 负责。

**为什么 v0.1 tool call 顺序执行**：保持审批门（approval gate）逻辑简单，避免并发 FFI 调用的复杂性，调试体验更好。并行执行作为 v0.2 优化项，届时可以通过 `ToolMetadata.execution_mode` per-tool 控制。

**为什么 v0.1 不用 actor framework**：tokio task + channel 已经能表达 agent runtime 的核心模式。Ractor/Kameo 的真实价值在多 actor 协同的 supervision，v0.1 只有单 run 单 task，引入 framework 是过度设计。

**为什么对齐 Anthropic Agent Skills 开放标准**：这是当前事实标准，已被 Anthropic 之外的多个平台采纳（open-agents、deep-agents、pi 都实现了 SKILL.md 加载），社区有 awesome-claude-skills 等共享资源。自创 skill 格式会失去整个生态。
