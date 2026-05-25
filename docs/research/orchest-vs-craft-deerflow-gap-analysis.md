# Orchest vs Craft Agents OSS / DeerFlow 核心 Agent 设计对比

> 日期: 2026-05-25
> 基线版本: orchest `HEAD` / craft-agents-oss `HEAD` / deer-flow `HEAD`

## 1. 三系统概览

| 维度 | Orchest | Craft Agents OSS | DeerFlow |
|------|---------|-------------------|----------|
| 语言 | Rust core + PyO3/napi-rs | TypeScript (Bun) | Python (LangGraph) |
| Agent 循环 | 自实现 `run.rs` loop（4040 行） | 包装 Claude Code / Pi Coding Agent SDK | LangGraph `create_agent` + StateGraph |
| 扩展模型 | `Tool` trait（单一维度） | `BaseAgent` 抽象类 + 回调注册 | `AgentMiddleware` 链（14+ 种） |
| 状态管理 | `RunState` 内存结构 | SessionManager + JSONL 持久化 | LangGraph StateGraph + Checkpointer |
| 工具注册 | `ToolRegistry`（线性注册） | SDK 原生工具 + Proxy 工具 + MCP Pool | LangChain `BaseTool` + `ToolNode` |
| 权限模型 | 单点 `requires_approval` 布尔标志 | 多模式权限 + pre-tool-use pipeline（6 步） | GuardrailMiddleware + SandboxAuditMiddleware |
| Sub-agent | `__sub_agent_request` 魔法字段 + `AgentDelegate` 双路径 | spawn-session + transfer | SubagentExecutor（ThreadPool + 隔离 event loop） |
| 代码规模 | core ~13200 行 + providers ~3000 行 | SessionManager 7934 行 + base-agent 1284 行 + backend 各 ~100KB | deerflow harness ~15000 行 |

## 2. 核心 Agent Loop 对比

### 2.1 Orchest（Rust，自实现）

```
loop {
    budget.check()?;
    model.complete() → ModelResponse (streaming via mpsc);
    budget.record_model_call();
    maybe_compact_context();
    for tool in tool_uses {
        if requires_approval → oneshot pend;
        tool.execute() → ToolOutput (Immediate/Structured/AgentDelegate/AsyncJob);
        budget.record_tool_call();
    }
    messages.push(tool_results);
    step += 1;
}
```

特点：单体循环，所有横切关注点硬编码在 `run.rs` 中（approval、budget、compaction、error mapping）。无拦截点，无中间件。

### 2.2 DeerFlow（Python，LangGraph）

```
StateGraph 节点链：call_model → tools → call_model → ...

中间件链（按注册顺序）:
  0-2: ThreadData → Uploads → Sandbox          (基础设施)
  3:   DanglingToolCallMiddleware                (修复)
  4:   GuardrailMiddleware                       (安全)
  5:   ToolErrorHandlingMiddleware               (容错)
  6:   SummarizationMiddleware                   (压缩)
  7:   TodoMiddleware                            (任务追踪)
  8:   TitleMiddleware                           (标题生成)
  9:   MemoryMiddleware                          (记忆更新)
  10:  ViewImageMiddleware                       (视觉注入)
  11:  SubagentLimitMiddleware                   (子代理限制)
  12:  LoopDetectionMiddleware                   (循环检测)
  13:  ClarificationMiddleware                   (末尾拦截)
  +:   SafetyFinishReasonMiddleware (after_model)
  +:   DynamicContextMiddleware (before_model)
  +:   LLMErrorHandlingMiddleware (wrap_model_call)
```

特点：每个中间件实现 `AgentMiddleware` trait，可选择覆盖 `before_model` / `after_model` / `wrap_model_call` / `wrap_tool_call` / `before_agent` / `after_agent`。链式组合，声明式配置。

### 2.3 Craft Agents OSS（TypeScript，SDK Wrapper）

```
SessionManager.prompt() →
  BaseAgent.chat() (provider-specific)
    → Pi/Claude SDK Session.prompt()
    → SDK 内部 loop (对 Craft Agents 不可见)
    → 事件流转发：EventQueue → EventSink → UI
  pre-tool-use pipeline (6 步):
    1. Permission mode check → 按模式阻止工具
    2. Source blocking → 阻止非活跃 MCP source 的工具
    3. Prerequisite check → 未读 guide.md 前阻止 source 工具
    4. call_llm interceptor → 拦截 LLM query 调用
    5. Input transforms → 路径展开、配置验证、skill 限定、metadata 剥离
    6. Ask-mode prompt decision → 是否需要用户审批
  source-activation auto-retry
  plan-approval flow
```

特点：在 SDK 层面做薄包装，核心 Agent 逻辑委托给第三方 SDK（Claude Code SDK / Pi Coding Agent）。重心在 session 管理、权限、source 编排、UI 事件桥接。

## 3. 差距分析：Orchest 缺少什么

### 3.1 P0 —— 中间件/Hook 框架（无）

这是 Orchest 与 DeerFlow/Craft Agents 之间最根本的架构差异。

**现状**：Orchest 的 run loop 是单体式的。approval、budget、compaction、tool 调度全部硬编码在 `run.rs` 中。要用 Orchest 构建一个产品级 Agent 应用时，无法注入自定义行为——要么 fork `run.rs`，要么在 tool 层 hack。

**DeerFlow 的 `AgentMiddleware` trait**（参考 `langchain.agents.middleware.AgentMiddleware`）：

```python
class AgentMiddleware(Generic[StateT]):
    def before_agent(self, state, runtime) -> dict | None: ...
    def after_agent(self, state, runtime) -> dict | None: ...
    def before_model(self, state, runtime) -> dict | None: ...
    def after_model(self, state, runtime) -> dict | None: ...
    def wrap_model_call(self, request, handler) -> ModelResponse: ...
    def wrap_tool_call(self, request, handler) -> ToolMessage | Command: ...
```

中间件可按 @Next/@Prev 注解插入到链中任意位置。

**Craft Agents 的 pre-tool-use pipeline**（参考 `pre-tool-use.ts`）：

```typescript
interface PreToolUseStep {
    name: string;
    check: (toolName, input, ctx) => PreToolUseResult;
    // Result: { action: 'allow' | 'block' | 'modify' | 'ask'; ... }
}
```

**建议**：v0.7 引入中间件 trait 和生命周期 hook。在 rust 侧定义：

```rust
#[async_trait]
pub trait AgentMiddleware: Send + Sync {
    async fn before_model_call(&self, messages: &[Message], runtime: &RuntimeContext) -> Result<Vec<Message>, AgentError> { Ok(messages.to_vec()) }
    async fn after_model_call(&self, response: &ModelResponse, runtime: &RuntimeContext) -> Result<ModelResponse, AgentError> { Ok(response.clone()) }
    async fn before_tool_call(&self, call: &ToolCall, runtime: &RuntimeContext) -> Result<ToolCall, AgentError> { Ok(call.clone()) }
    async fn after_tool_call(&self, call: &ToolCall, result: &ToolOutput, runtime: &RuntimeContext) -> Result<Option<ToolOutput>, AgentError> { Ok(None) }
}
```

### 3.2 P0 —— Loop Detection（无）

**问题**：Orchest 没有任何循环检测。模型可能反复调用同一工具直到 `max_steps` 耗尽或 budget 枯竭，且不会产生任何警告。

**DeerFlow 的 `LoopDetectionMiddleware`**（613 行）：

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `warn_threshold` | 3 | 相同工具调用集出现 3 次后注入警告 |
| `hard_limit` | 5 | 相同工具调用集出现 5 次后强制截断 tool_calls |
| `window_size` | 20 | 滑动窗口大小 |
| `tool_freq_warn` | 30 | 同工具类型调用 30 次后警告 |
| `tool_freq_hard_limit` | 50 | 同工具类型调用 50 次后强制停止 |
| `tool_freq_overrides` | — | 每工具阈值覆盖 |

去重策略：
- 基于 `(name + stable_key)` 的 MD5 哈希
- `read_file`：按 200 行分桶，避免相邻行被当作不同调用
- `write_file` / `str_replace`：全参数哈希（内容敏感）
- 其他工具：仅选 `path/url/query/command/pattern/glob` 等 salient 字段

警告消息在 `wrap_model_call` 中注入（避免破坏 AIMessage→ToolMessage 配对）。

**建议**：v0.7 必须补。这是所有 Agent 产品 faced 的第一大生产事故。实现要点：
- 两级防御（warn → hard stop）
- 智能去重（区分 read_file 和 write_file）
- per-tool 频率检测（防止跨文件读取死循环）

### 3.3 P0 —— 权限与安全中间件（单一维度）

**问题**：Orchest 只有 `requires_approval: bool`。当构建多租户、多用户权限级别的产品时完全不够。

**Craft Agents 的权限体系**：

| 模式 | 行为 |
|------|------|
| Safe | 只允许只读工具（Read/Glob/Grep）+ 审批后写文件 |
| AcceptEdits | 允许写文件 + 审批后 Bash |
| Plan | 先出计划再执行 |
| Ask | 每次工具调用都需审批 |
| Admin | 旁路所有审批 |

额外：
- 命令白名单（`bash-validator.ts`，1800+ 行）
- 路径边界验证（`validate-file-path`）
- ShellGuard Corpus（30KB 测试，24 条高风险规则）
- `permission-manager.ts`：基于工作区/会话的权限计算

**DeerFlow 的安全体系**：

| 组件 | 功能 |
|------|------|
| `GuardrailMiddleware` | 可插拔 `GuardrailProvider`（`allow/deny/modify`） |
| `SandboxAuditMiddleware` | bash 命令分类：24 条 HIGH_RISK pattern + 6 条 MEDIUM_RISK pattern |
| `SafetyFinishReasonMiddleware` | 检测 provider 安全终止（`content_filter`/`refusal`/`SAFETY`）后截断 tool_calls |
| `LoopDetectionMiddleware` | 防止无限循环 |
| `SubagentLimitMiddleware` | 限制子代理数量和深度 |

**DeerFlow SandboxAudit 的 HIGH_RISK patterns**（部分）：
- `rm -rf /*` / `dd if=` / `mkfs` — 破坏性文件操作
- `curl|sh` / `base64 -d|` — 管道注入
- `` `$(curl ...)` `` / `$()` — 命令替换
- `LD_PRELOAD` / `/dev/tcp/` — 权限提升
- `:(){ :|:& };:` — fork bomb
- `> /etc/` / `> ~/.bashrc` — 覆盖系统文件

**建议**：
1. v0.7 将 `requires_approval: bool` 升级为 `PermissionPolicy` 枚举（Safe / Ask / Plan / Admin）
2. 引入 `GuardrailProvider` trait（参考 DeerFlow）
3. bash 命令安全审计（参考 DeerFlow 的 24 条 pattern）
4. Safety termination 处理

### 3.4 P1 —— LLM Error Handling（无重试/熔断）

**问题**：Orchest 在 `model.complete()` 失败时直接 `return`，不重试。

**DeerFlow 的 `LLMErrorHandlingMiddleware`**（369 行）：

| 机制 | 详情 |
|------|------|
| 指数退避重试 | max_attempts=3, base=1000ms, cap=8000ms |
| 可重试错误 | HTTP 408/409/425/429/500/502/503/504 + "server busy" 等文本模式 |
| 不可重试错误 | Quota（`insufficient_quota`）+ Auth（`unauthorized`） |
| Circuit Breaker | failure_threshold → open → recovery_timeout → half_open probe |
| Retry-After | 解析 HTTP 头以确定等待时间 |
| 用户消息 | 每类错误生成用户可读的申诉消息 |

**Craft Agents 的错误处理**：
- `claude-sdk-error-mapper.ts`（用于 Anthropic SDK）
- `source-activated-auto-retry`：source 激活后自动重发用户消息
- OAuth token 过期自动刷新（`TokenRefreshManager`）

**建议**：v0.6 或 v0.7 在 model adapter 层或 run loop 层加入：
- 可重试错误分类
- 指数退避 + jitter
- Circuit breaker
- 用户可读错误消息

### 3.5 P1 —— 上下文压缩（部分有，不完整）

**现状**：Orchest 有 `maybe_compact_context`（基于 `compaction_threshold` + `compaction_recent_messages`），但实现较简。

**DeerFlow 的 `DeerFlowSummarizationMiddleware`**：

| 配置项 | 说明 |
|--------|------|
| `trigger` | 基于 token 阈值或消息数的触发条件 |
| `keep` | 保留最近 N 对消息 |
| `trim_tokens_to_summarize` | 总结本身长度限制 |
| `summary_prompt` | 自定义总结 prompt |
| `preserve_recent_skill_count` | 保护最近 N 个 skill 上下文不被压缩 |
| `preserve_recent_skill_tokens` | 保护最近 N tokens 的 skill 上下文 |
| `skill_file_read_tool_names` | skill 文件读取工具名称列表 |
| 独立 summarization model | 可能使用更便宜的模型做总结 |
| `memory_flush_hook` | 总结前先刷新记忆 |

**建议**：验证当前 `maybe_compact_context` 是否足够。考虑：
- 独立 summarization model
- skill 上下文保护
- 多触发条件（token 阈值 + 消息数）

### 3.6 P1 —— 记忆系统（无）

**问题**：Orchest 完全没有跨 session 的长期记忆。产品级 Agent 需要记住用户偏好、项目上下文、历史决策。

**DeerFlow 的 `MemoryMiddleware`**（+ memory 子系统共 ~1000 行）：

```
after_agent() →
  filter_messages_for_memory() → 仅保留用户输入和最终助手回复
  detect_correction() → 检测用户纠正信号（优先级高于强化）
  detect_reinforcement() → 检测用户强化信号
  queue.add() → debounce 批量更新
  MemoryUpdater (异步) → LLM summarization → 存储
```

prompt 注入格式：
```
<user_memory>
- (2026-05-20) User prefers tabs over spaces in all projects
- (2026-05-21) Project uses Rust 2024 edition with edition=2024 in Cargo.toml
</user_memory>
```

**建议**：v0.7 引入记忆 trait：
```rust
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn update(&self, thread_id: &str, messages: &[Message]) -> Result<(), MemoryError>;
    async fn query(&self, thread_id: &str) -> Result<Vec<MemoryEntry>, MemoryError>;
}
```

### 3.7 P2 —— Safety Termination 处理（无）

**问题**：DeerFlow 的 `SafetyFinishReasonMiddleware` 处理 provider 返回 `finish_reason=content_filter`（OpenAI）/ `stop_reason=refusal`（Anthropic）/ `SAFETY`（Gemini）但 tool_calls 部分形成的场景——截断 tool_calls 避免执行截断参数。

Orchest 的 `StopReason` 枚举已有 `ContentFilter`、`Refusal`、`Interrupted`（v0.5 新增），但 run loop 中未针对这些做特殊处理。

**建议**：在 tool dispatch 前检查 `stop_reason`，若为安全相关则丢弃 tool_calls 并注入解释消息。

### 3.8 P2 —— Dangling Tool Call 修复（无）

**问题**：DeerFlow 的 `DanglingToolCallMiddleware` 修复 summarization 截断历史后 AIMessage.tool_calls 缺少对应 ToolMessage 的问题。

**建议**：在 compaction 后添加消息完整性检查。

### 3.9 P2 —— Dynamic Context Injection（不足）

**问题**：DeerFlow 的 `DynamicContextMiddleware` 将当前日期、记忆摘要等作为 `<system-reminder>` 注入到 HumanMessage，而不修改 system prompt——避免破坏 prefix cache。

Orchest 目前只有静态 system_prompt。

**建议**：引入 `before_model_call` hook 或专门的 context injection 层。

### 3.10 P2 —— Skill 安全与工具策略（不足）

**问题**：DeerFlow 有：

| 组件 | 功能 |
|------|------|
| `skills/security_scanner.py` | Skill 文件安全扫描 |
| `skills/tool_policy.py` | `filter_tools_by_skill_allowed_tools` |
| `skills/validation.py` | Skill 元数据验证 |
| `skills/installer.py` | Skill 安装与管理 |
| `SkillManageTool` | 运行时 skill 管理工具 |

Orchest 的 skill 系统缺少安全扫描和 fine-grained 工具策略过滤。

### 3.11 P2 —— Todo/Task Tracking（无）

**问题**：DeerFlow 的 `TodoMiddleware` 为 Agent 提供 `write_todos` 工具，用于复杂多步任务的追踪。Craft Agents 也有内置的计划执行机制。

Orchest 没有此能力——Agent 对复杂多步任务的进度完全无感知。

### 3.12 P2 —— Title Generation（无）

DeerFlow 的 `TitleMiddleware` 在首次对话交换后自动生成会话标题。这是产品化 Agent 的标配功能。

### 3.13 P2 —— Auto-Retry on Source Activation（无）

Craft Agents 的 `source-activated-auto-retry`：当 session-scoped 工具成功激活新 source 后，自动终止当前 turn 并携带 `[{slug} activated]` 后缀重发用户消息，使新 source 的工具在下一 turn 可用。

## 4. Orchest 已有优势（不应丢失）

| 能力 | 状态 | 备注 |
|------|------|------|
| 多 provider adapter | v0.5 完成 | Anthropic/OpenAI/DeepSeek/OpenRouter 统一 `ModelAdapter` trait |
| MCP 集成 | 已有 | stdio + HTTP transport，MCP 工具透明注册到 ToolRegistry |
| Sub-agent | 已有 | 预算继承、审批路由、深度限制、child event 包装 |
| Budget guard | 已有 | token/tool_call/duration/cost 四维 + 子代理增量上报 |
| Approval gate | 已有 | oneshot 机制 + 父子代理路由（v0.6 升级为 ApprovalBus） |
| Async job | 已有 | polling + webhook 双路径 |
| Code execution | 已有 | MCP server 集成 |
| Skill loading | 已有 | SKILL.md 解析 + bundled tools + CapabilityValidator |
| Tool search | 已有 | SearchToolsTool（动态工具发现） |
| 跨语言 SDK | 已有 | PyO3 + napi-rs，类型安全 |
| 流式事件 | 已有 | mpsc channel + RuntimeEvent 枚举（40+ 变体） |
| Context compaction | 已有 | 待增强 |

## 5. 推荐路线图

### v0.6（已规划）
run.rs 模块化拆分、AgentConfig 重构、Budget 定价解耦、ApprovalBus、MCP 并发、tiktoken、HTTP 客户端共享、Python SDK 改进。

### v0.7（建议新增）

| 优先级 | 特性 | 参考 |
|--------|------|------|
| P0 | **AgentMiddleware trait** + 生命周期 hook | DeerFlow `AgentMiddleware`, Craft Agents pre-tool-use pipeline |
| P0 | **Loop Detection**（哈希+滑动窗口+两级阈值+per-tool 频率） | DeerFlow `LoopDetectionMiddleware` |
| P0 | **权限模型升级**：PermissionPolicy 枚举 + GuardrailProvider trait | DeerFlow `GuardrailMiddleware`, Craft Agents `mode-manager.ts` |
| P0 | **Bash 命令安全审计** | DeerFlow `SandboxAuditMiddleware` |
| P1 | **LLM Error 重试 + Circuit Breaker** | DeerFlow `LLMErrorHandlingMiddleware` |
| P1 | **记忆系统**（MemoryStore trait + LLM 总结 + debounce） | DeerFlow `MemoryMiddleware` |
| P1 | **上下文压缩增强**：独立 summarization model、skill 保护 | DeerFlow `DeerFlowSummarizationMiddleware` |
| P2 | **Safety Termination 处理** | DeerFlow `SafetyFinishReasonMiddleware` |
| P2 | **Dangling Tool Call 修复** | DeerFlow `DanglingToolCallMiddleware` |
| P2 | **Dynamic Context Injection** | DeerFlow `DynamicContextMiddleware` |
| P2 | **Skill 安全扫描与工具策略** | DeerFlow `skills/security_scanner.py`, `tool_policy.py` |
| P2 | **Todo/Task Tracking** | DeerFlow `TodoMiddleware` |
| P2 | **Title Generation** | DeerFlow `TitleMiddleware` |
| P2 | **Source Activation Auto-Retry** | Craft Agents `source-activated-auto-retry` |

## 6. 架构建议：中间件优先

中间件框架是所有 P0/P1 特性的基础设施。建议 v0.7 先完成中间件 trait 定义和 run loop 重构，然后在此基础上依次实现 Loop Detection、Guardrail、LLM Retry、Memory 等。

重构后的 run loop 结构：

```rust
// 中间件链在 AgentConfig 中配置
let middleware_chain: Vec<Arc<dyn AgentMiddleware>> = config.build_middleware_chain();

loop {
    // before_model_call hook (所有中间件)
    let messages = apply_before_model(&middleware_chain, &messages, &ctx).await?;

    let response = model.complete(&messages, &tool_defs, &opts, stream_tx).await;

    // 可在此注入 LLM error retry
    let response = apply_after_model(&middleware_chain, &response, &ctx).await?;

    budget.record_model_call(&response.usage);
    maybe_compact_context(...);

    for tool_call in &tool_uses {
        // before_tool_call hook (guardrail / permission / loop detection)
        let tool_call = apply_before_tool(&middleware_chain, &tool_call, &ctx).await?;
        let result = tool.execute(...).await;
        // after_tool_call hook (audit / memory / stats)
        let result = apply_after_tool(&middleware_chain, &tool_call, &result, &ctx).await?;
    }
}
```

中间件链可组合、可排序、可禁用，每个中间件只做一件事。这与 DeerFlow 的设计理念一致，但在 Rust trait 系统中实现，保持零成本抽象。
