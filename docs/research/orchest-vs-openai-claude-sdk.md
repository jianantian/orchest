# Orchest vs OpenAI Agents SDK / Claude Agent SDK 核心设计对比

> 日期: 2026-05-25
> 基线: orchest `HEAD` / openai-agents-python `HEAD` / claude-agent-sdk-python `HEAD`

## 1. 四系统概览

| 维度 | Orchest | OpenAI Agents SDK | Claude Agent SDK |
|------|---------|-------------------|-----------------|
| 语言 | Rust core + PyO3/napi-rs | Python | Python |
| Agent 循环 | 自实现 `run.rs`（4040 行单体） | `AgentRunner.run()` → `run_single_turn()` 多模块分层 | `ClaudeSDKClient` → `Query` → 子进程 CLI JSONL 协议 |
| 扩展模型 | `Tool` trait | `AgentHooks` + `RunHooks` + `Guardrail` + `Handoff` + `ToolGuardrail` | `HookMatcher`（10 种 Hook 事件）+ `can_use_tool` + `PermissionUpdate` |
| Sub-agent | `AgentDelegate` + `__sub_agent_request` 双路径 | `Handoff`（类型安全、input_filter、nest_history）| `AgentDefinition` + `Task` tool（CLI 原生 sub-agent） |
| 权限模型 | `requires_approval: bool` | `ToolApprovalItem` + `tool_guardrails`（三层：approval + input guardrail + output guardrail）| `PermissionMode`（6 种） + `can_use_tool` callback + `PermissionUpdate` + `PreToolUse` Hook |
| Guardrail | 无 | `InputGuardrail` + `OutputGuardrail` + `ToolInputGuardrail` + `ToolOutputGuardrail` | `PreToolUse` / `PostToolUse` / `PostToolUseFailure` / `PermissionRequest` / `Notification` Hooks |
| 错误处理 | 模型调用失败→直接 return | `ModelRetryBackoffSettings` + `ModelRetryAdvice` + `model_retry.py` | CLI 内置重试（SDK 层不感知） |
| 记忆/持久化 | 无 | `Session`（SQLite + OpenAI Server） + `SessionInputCallback` | `SessionStore`（可插拔后端） + JSONL 文件 |
| MCP | 内置 `McpStdioClient` + `McpHttpClient` | `MCPServer` + `MCPServerManager`（stdlib/SSE/Streamable HTTP）| `McpServerConfig`（stdio/sse/http/sdk）+ SDK MCP Server（`create_sdk_mcp_server`） |
| 沙箱 | Code Execution MCP | `SandboxSession` + `BaseSandboxClient` + `Manifest` + `Snapshot` | `SandboxSettings` + CLI 内置沙箱 |

## 2. 核心架构对比

### 2.1 扩展模型

**OpenAI Agents SDK** 有三层扩展：

| 层级 | 机制 | 作用 |
|------|------|------|
| Agent 级 | `AgentHooks`（7 个回调） | on_start, on_end, on_handoff, on_tool_start, on_tool_end, on_handoff_routed, on_agent_clone |
| Run 级 | `RunHooks`（7 个回调） | on_agent_start, on_agent_end, on_handoff, on_tool_start, on_tool_end, on_run_end, on_run_error |
| Tool 级 | `ToolInputGuardrail` + `ToolOutputGuardrail` | 工具输入/输出审查，三种行为：allow / reject_content / raise_exception |
| 全局 | `InputGuardrail` + `OutputGuardrail` | 输入/输出 tripwire，触发则终止 run |

每个层级的回调是明确定义的类型，不是字符串匹配：
```python
class AgentHooks(Generic[TContext]):
    async def on_tool_start(self, context, agent, tool) -> None: ...
    async def on_tool_end(self, context, agent, tool, result) -> None: ...
    async def on_handoff(self, context, from_agent, to_agent) -> None: ...
```

**Claude Agent SDK** 有三层扩展：

| 层级 | 机制 | 作用 |
|------|------|------|
| Hook | `HookMatcher`（10 种事件） | PreToolUse、PostToolUse、PostToolUseFailure、UserPromptSubmit、Stop、SubagentStop、PreCompact、Notification、SubagentStart、PermissionRequest |
| 权限 | `can_use_tool` callback | 工具级动态允许/拒绝，返回 `PermissionResultAllow(updated_input)` 或 `PermissionResultDeny(message, interrupt)` |
| 权限更新 | `PermissionUpdate`（6 种操作） | addRules、replaceRules、removeRules、setMode、addDirectories、removeDirectories |

Hook 回调函数签名统一：
```python
HookCallback = Callable[
    [HookInput, str | None, HookContext],  # input, tool_use_id, context
    Awaitable[HookJSONOutput]               # control output
]
```

每个 Hook 事件有独立的输入类型（discriminated union on `hook_event_name`）：
- `PreToolUseHookInput` → `tool_name`, `tool_input`, `tool_use_id`
- `PostToolUseHookInput` → `tool_name`, `tool_input`, `tool_response`, `tool_use_id`
- `PostToolUseFailureHookInput` → `tool_name`, `tool_input`, `error`
- `UserPromptSubmitHookInput` → `prompt`
- `PreCompactHookInput` → `trigger`, `custom_instructions`
- `SubagentStartHookInput` → `agent_id`, `agent_type`

**Orchest** 无任何扩展机制。所有行为硬编码在 `run.rs`。

### 2.2 Handoff / Sub-agent

**OpenAI Agents SDK — `Handoff`**（类型安全、可组合）：

```python
@dataclass
class Handoff(Generic[TContext]):
    tool_name: str              # 模型看到的手递工具名
    tool_description: str       # 模型看到的工具描述
    input_json_schema: dict     # 手递时传递的结构化参数 schema
    on_invoke_handoff: Callable[[RunContextWrapper, str], Awaitable[Agent]]
    agent_name: str
    input_filter: HandoffInputFilter | None  # 过滤传递给下个 agent 的输入
    nest_handoff_history: bool | None        # 是否折叠历史
    strict_json_schema: bool = True
```

关键特性：
- 手递是一个特殊类型的工具调用——模型决定何时调用
- `input_filter` 允许过滤/转换传递给下个 agent 的上下文
- `nest_handoff_history` 将上游历史折叠为一个 assistant message 嵌套注入
- `HandoffInputData` 提供完整上下文：`input_history` + `pre_handoff_items` + `new_items`
- 支持 `is_enabled` 动态开关：`bool | Callable[[RunContextWrapper, Agent], bool]`

**Orchest Sub-agent**（两个并行路径）：
1. `__sub_agent_request` 魔法字段：JSON 协议，通过 `execute_sub_agent_request()` 处理
2. `AgentDelegate`：原生 Rust trait，通过 `execute_agent_delegate()` 处理

两个路径都需要手动维护。没有输入过滤、历史折叠、动态启用/禁用。

### 2.3 Guardrail 体系

**OpenAI Agents SDK** 的四层 guardrail：

```
InputGuardrail  ──→ tripwire → InputGuardrailTripwireTriggered exception
    │                (run_in_parallel=True 时与 agent 并行)
    │
    ▼
Agent 执行
    │
    ├── ToolInputGuardrail  ──→ reject_content / raise_exception
    │   (每次工具调用前)
    │
    ├── ToolOutputGuardrail ──→ reject_content / raise_exception
    │   (每次工具调用后)
    │
    ▼
OutputGuardrail ──→ tripwire → OutputGuardrailTripwireTriggered exception
```

每个 guardrail 输出：
```python
@dataclass
class GuardrailFunctionOutput:
    output_info: Any           # 可选的审查详情
    tripwire_triggered: bool   # True 则终止

@dataclass
class ToolGuardrailFunctionOutput:
    output_info: Any
    behavior: RejectContentBehavior | RaiseExceptionBehavior | AllowBehavior
```

**Orchest** 无 guardrail 概念。`requires_approval` 只是最粗粒度的二元开关。

### 2.4 生命周期 Hook

**OpenAI Agents SDK** 的 `RunHooks` + `AgentHooks`：

```python
class RunHooks(Generic[TContext]):
    async def on_agent_start(self, context, agent) -> None: ...
    async def on_agent_end(self, context, agent, output) -> None: ...
    async def on_handoff(self, context, from_agent, to_agent) -> None: ...
    async def on_tool_start(self, context, agent, tool) -> None: ...
    async def on_tool_end(self, context, agent, tool, result) -> None: ...
    async def on_run_end(self, context) -> None: ...
    async def on_run_error(self, context, error: RunErrorDetails) -> None: ...
```

**Claude Agent SDK** 的 Hook 事件：
- `PreToolUse` — 工具调用前（可设置 permissionDecision: allow/deny/ask/defer、可修改 updatedInput）
- `PostToolUse` — 工具调用后（可注入 additionalContext、可替换 updatedToolOutput）
- `PostToolUseFailure` — 工具调用失败后
- `UserPromptSubmit` — 用户提交 prompt 前
- `Stop` — Agent 停止时
- `SubagentStop` — Sub-agent 停止时
- `PreCompact` — 上下文压缩前
- `Notification` — 系统通知
- `SubagentStart` — Sub-agent 启动时
- `PermissionRequest` — 权限请求时

### 2.5 记忆/会话持久化

**OpenAI Agents SDK — `Session`**：

```python
class Session:
    async def get_items(self, session_id) -> list[TResponseInputItem]: ...
    async def update_session(self, session_id, items, settings) -> None: ...
    async def create_session(self, ...) -> None: ...
    async def delete_session(self, session_id) -> None: ...
```

内置实现：
- `SQLiteSession` — 本地 SQLite 存储
- `OpenAIConversationsSession` — OpenAI 服务端 conversation API

`SessionInputCallback` 允许在恢复 session 时注入上下文：
```python
SessionInputCallback = Callable[[str, list[TResponseInputItem]], Awaitable[list[TResponseInputItem]]]
```

**Claude Agent SDK — `SessionStore`**：

```python
class SessionStore(Protocol):
    async def load(self, key: SessionKey) -> list[dict]: ...
    async def append(self, key: SessionKey, entries: list[dict]) -> None: ...
    async def keys(self, subkeys: SessionListSubkeysKey) -> list[SessionKey]: ...
    async def delete(self, key: SessionKey) -> None: ...
```

内置实现：
- `InMemorySessionStore`
- CLI 文件系统 JSONL（通过 `_internal/sessions.py`）

额外：
- `fork_session()` / `fork_session_via_store()` — 会话分叉
- `rename_session()` — 重命名
- `tag_session()` — 标签
- `fold_session_summary()` — 摘要折叠
- `import_session_to_store()` — 导入

### 2.6 错误处理与重试

**OpenAI Agents SDK — `retry.py`**：

```
ModelRetryBackoffSettings
  initial_delay: float         # 首次重试前延迟
  max_delay: float             # 最大延迟
  multiplier: float            # 退避乘数
  jitter: bool                 # 随机抖动

ModelRetryNormalizedError      # 归一化错误事实
  status_code, error_code, message, retry_after,
  is_abort, is_network_error, is_timeout

ModelRetryAdvice               # provider 返回的重试建议
  suggested, retry_after, replay_safety
```

`model_retry.py` 的 `get_response_with_retry()` / `stream_response_with_retry()` 自动处理重试。

**Orchest**：模型调用失败→直接 `return`，无重试。

## 3. 差距分析：Orchest vs 行业标杆

### 3.1 P0 —— Hook/扩展框架（零 vs 丰富）

| 能力 | OpenAI SDK | Claude SDK | Orchest |
|------|-----------|------------|---------|
| 工具调用前 hook | `AgentHooks.on_tool_start` | `PreToolUse` Hook | 无 |
| 工具调用后 hook | `AgentHooks.on_tool_end` | `PostToolUse` Hook | 无 |
| 工具调用失败 hook | 无（通过异常） | `PostToolUseFailure` Hook | 无 |
| Agent 启动 hook | `RunHooks.on_agent_start` | `SubagentStart` Hook | 无 |
| Agent 结束 hook | `RunHooks.on_agent_end` | `Stop` Hook | 无 |
| Handoff hook | `RunHooks.on_handoff` | `SubagentStop` Hook | 无 |
| 压缩前 hook | 无 | `PreCompact` Hook | 无 |
| 用户输入 hook | 无 | `UserPromptSubmit` Hook | 无 |
| 权限请求 hook | `ToolApprovalItem` | `PermissionRequest` Hook + `can_use_tool` | `requires_approval` 布尔 |
| Run 结束 hook | `RunHooks.on_run_end` | 无 | `RuntimeEvent` 枚举 |
| Run 错误 hook | `RunHooks.on_run_error` | 无 | `RuntimeEvent::RunFailed` |

**关键差异**：OpenAI 和 Claude SDK 的 hook 可以**修改行为**（修改 tool input、替换 tool output、注入 additional context、拒绝调用），而不仅仅是观察。Orchest 的 `RuntimeEvent` 只是事件广播，无法反馈到 run loop。

### 3.2 P0 —— Guardrail 体系（零 vs 四层）

OpenAI SDK 的四层 guardrail 形成了从输入到输出的完整审查链：

```
InputGuardrail         → 输入检查（安全检查、话题过滤）
ToolInputGuardrail     → 工具参数检查（注入攻击、越权调用）
ToolOutputGuardrail    → 工具输出检查（敏感信息泄露、格式校验）
OutputGuardrail        → 最终输出检查（有害内容、合规检查）
```

Claude SDK 通过 `PreToolUse` + `PermissionRequest` Hook 覆盖类似场景，且支持 `updatedInput` 修改。

Orchest 如果要构建生产级 Agent 产品，guardrail 是必须的——不管是安全合规还是业务规则校验。

### 3.3 P0 —— Sub-agent / Handoff 机制（简陋 vs 类型安全+可组合）

OpenAI SDK 的 `Handoff` 设计：

```python
# 声明式定义
billing_handoff = Handoff(
    tool_name="transfer_to_billing",
    tool_description="Transfer to billing agent for invoice/payment issues",
    input_json_schema={"user_summary": str},
    on_invoke_handoff=lambda ctx, args: resolve_billing_agent(args),
    agent_name="BillingAgent",
    input_filter=custom_input_filter,  # 可选：过滤传递的上下文
    nest_handoff_history=True           # 可选：折叠历史
)

# Agent 配置
triage_agent = Agent(
    name="TriageAgent",
    handoffs=[billing_handoff, support_handoff],
    tools=[...]
)
```

模型自动看到 `transfer_to_billing` 和 `transfer_to_support` 作为可用工具，参数 schema 由 `input_json_schema` 定义。

Orchest 的 `AgentDelegate` trait 提供了基础能力，但缺少：
- 声明式 handoff 定义（模型自动发现）
- 结构化参数传递（`input_json_schema`）
- 上下文过滤（`input_filter`）
- 历史折叠（`nest_handoff_history`）
- 动态启用/禁用（`is_enabled`）

### 3.4 P1 —— 会话持久化与记忆（零 vs 可插拔）

OpenAI SDK 的 `Session` 抽象和 Claude SDK 的 `SessionStore` 协议都是可插拔的持久化层。两者都支持：
- 会话列表/查询
- 会话恢复（包括跨进程）
- 会话删除/清理

Claude SDK 额外支持：
- 会话分叉（fork_session）
- 会话重命名/标签
- 摘要折叠（fold_session_summary）
- 跨 store 导入（import_session_to_store）

Orchest 的 `RunState` 仅有 `#[serde(skip)]` 的 `available_tools` 字段——无法序列化，无法跨进程恢复。

### 3.5 P1 —— 内置 Sandbox 集成（有 but 不完整）

OpenAI SDK 的 `sandbox/` 模块：
- `BaseSandboxSession` + `BaseSandboxClient` trait
- `Manifest`（声明式文件/包/命令）
- `Snapshot`（沙箱状态快照）
- `apply_patch` / `workspace_paths`
- 内置 `LocalSandbox`、`DaytonaSandbox` provider
- `SandboxAgent`（在沙箱内运行的 agent 包装器）

Claude SDK 通过 CLI 子进程内置沙箱（`SandboxSettings` + `sandbox` 配置）。

Orchest 的 Code Execution MCP 提供了基础代码执行，但缺少：
- 声明式环境配置（Manifest）
- 沙箱会话管理（复用/快照/恢复）
- 文件同步（workspace ↔ sandbox）
- 多 provider 抽象

### 3.6 P1 —— Tracing / Telemetry（无 vs 内置）

OpenAI SDK 的 `tracing/` 模块（~1500 行）：
- `Trace` / `Span` / `SpanData` 层次结构
- `AgentSpanData` / `TaskSpanData` / `ToolSpanData`
- 可插拔 `TraceProvider`（OpenAI、Langfuse、自定义）
- `SpanProcessor` pipeline（export、batch、filter）
- 自动将 span 与 OpenAI responses API 的 tracing 集成

Orchest 的 `telemetry.rs` 目前只有 56 行，仅有 span 创建函数但无 processor/provider 体系。

### 3.7 P2 —— 流式事件类型系统

OpenAI SDK 的 `StreamEvent`：
```python
StreamEvent = RawResponsesStreamEvent | RunItemStreamEvent | AgentUpdatedStreamEvent
```

- `RawResponsesStreamEvent` — LLM 原始流事件透传
- `RunItemStreamEvent` — 包装 `RunItem`（ToolCallItem, HandoffCallItem, ReasoningItem 等）
- `AgentUpdatedStreamEvent` — Agent 切换通知

Claude SDK 通过 `StreamEvent`（与 TypeScript SDK 对齐）提供丰富的消息类型。

Orchest 的 `RuntimeEvent` 枚举已有 40+ 变体，覆盖面广，但缺少 `AgentUpdated` 事件和 handoff 通知。

### 3.8 P2 —— Tool 元数据与注解

OpenAI SDK 的 `Tool` 协议包含丰富的元数据：
- `ToolOrigin`（function / mcp / computer / shell / apply_patch / custom）
- `ToolContext`（tool_call_id, run_context, tool_use_id）
- `FunctionTool` vs `CustomTool` vs `ApplyPatchTool` vs `ComputerTool`

Claude SDK 的 MCP 工具包含 `McpToolAnnotations`（readOnly / destructive / openWorld）。

Orchest 的 `ToolMetadata` 有 source + requires_approval + timeout + max_output_tokens，但缺少：
- 工具能力声明（readOnly / destructive）
- 工具来源分级（内置 vs MCP vs 用户自定义 vs skill）

## 4. 推荐优先级

### v0.7 必须（对标 OpenAI/Claude SDK）

| 优先级 | 能力 | 对标 |
|--------|------|------|
| P0 | **AgentMiddleware trait** + before/after hooks（可修改行为） | OpenAI `AgentHooks`/`RunHooks`，Claude `HookMatcher` |
| P0 | **Guardrail trait**（input/output/tool 三层 + tripwire） | OpenAI `InputGuardrail`/`OutputGuardrail`/`ToolGuardrail` |
| P0 | **Handoff 重构**（声明式、类型安全、input_filter、nest_history） | OpenAI `Handoff` |
| P1 | **Session 持久化**（可插拔 `SessionStore` trait） | Claude `SessionStore`，OpenAI `Session` |
| P1 | **Sandbox 抽象**（`SandboxProvider` trait + Manifest + Snapshot） | OpenAI `sandbox/` 模块 |
| P1 | **Tracing 体系**（SpanProcessor + TraceProvider 可插拔） | OpenAI `tracing/` 模块 |
| P2 | **ModelRetry 框架**（backoff + retry_after + normalized error） | OpenAI `retry.py` |

### 设计原则（从两个标杆 SDK 提炼）

1. **Hook 不只是观察者，是参与者**：Hook 可以修改 input、替换 output、拒绝调用、注入 context。`RuntimeEvent` 的广播模式不够。
2. **Handoff 是一等公民**：不是 sub-agent 的实现细节，而是 model-visible 的声明式工具。参数 schema、上下文过滤、历史折叠都是产品级手递的必需品。
3. **Guardrail 要分层**：输入、工具输入、工具输出、最终输出——每层独立判断，行为可组合（allow / reject_content / raise_exception）。
4. **持久化是可插拔的**：`SessionStore` / `Session` trait，不是硬编码的文件格式。
5. **Tracing 是基础设施**：Span Processor pipeline + Provider 注册，不被任何一个 vendor 绑定。

## 5. 关键差异总结

两套行业标杆 SDK 的共同设计模式：

1. **分层 hook 体系**：Agent 级 + Run 级 + Tool 级（OpenAI），或 HookEvent 统一类型（Claude）——但核心都是让调用方在 run loop 的各个阶段插入行为
2. **Guardrail 与 Permission 分离**：Guardrail 是安全检查（tripwire 终止），Permission 是用户交互（allow/deny/ask）。两个概念独立。
3. **Handoff 是声明式的**：模型通过 tool schema 自动发现可用的 sub-agent，不需要魔法字段
4. **Session 持久化是标准配置**：两个 SDK 都有可插拔的 session store，支持跨进程恢复
5. **Sandbox 是一等公民**：不仅仅是一个 tool，而是 Agent 运行的隔离环境——带 Manifest、Snapshot、文件同步

Orchest 的 Rust 核心提供了性能优势和类型安全，但在 Agent 框架的抽象层次上，距离行业标杆还有系统性的差距。v0.7 需要把"中间件+Guardrail+Handoff+Session+Sandbox"五个抽象体系建立起来，才能让 Orchest 成为一个能构建生产级 Agent 产品的 SDK。
