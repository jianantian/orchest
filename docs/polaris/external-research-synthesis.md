# 外部研究综合分析：API 自然度审计

> **文档状态**：Polaris 长期参考
>
> 综合 5 份外部研究（2026-05-31），审查 Orchest SDK 在实现业界最佳实践时的 API 自然度——不只是"能不能做到"，而是"用户是否会被 API 的形状引导到正确的做法"。

---

## 核心原则

一个好的 SDK API 不只是"允许"正确的用法，而是让正确的用法**比错误的用法更短、更明显、更不需要查文档**。

对照 5 份研究的共同模式，逐一检查我们的 API 是否满足这个标准。

---

## 1. 工具审批：从布尔值到意图表达

### 研究的共识

agents-best-practices、pi-subagents、meta-engineering-harness 三份研究一致要求：**审批应该按风险分类，而不是笼统的"需要/不需要"。**

### 当前 API

```rust
pub struct ToolMetadata {
    pub requires_approval: bool,
}
```

用户想实现"金融操作要审批，只读操作不要"时，要么靠 tool name 字符串匹配，要么把所有 tool 都设 `true` 然后在 Hook 里硬编码白名单。两种都脆弱。

### 自然 API 应该长什么样

```rust
pub struct ToolMetadata {
    pub approval: Approval,
}

pub enum Approval {
    Never,              // 只读、搜索、计算——不需要审批
    WhenRisky,          // 写入内部数据——用户策略决定（默认值）
    Always,             // 外部通信、金融、破坏性——总是审批
}
```

**为什么这个形状更自然**：

- 用户定义 tool 时被迫思考"这个操作有多危险"——三个选项比 true/false 更能引导分类
- Hook 代码自然写成 `match metadata.approval { Never => continue, WhenRisky => check_policy(...), Always => pause(...) }`——比 `if tool_name.starts_with("send_")` 清晰太多
- `WhenRisky` 是默认值——不做选择的用户偏安全，但不会被不必要的审批淹没

### 不需要的

- **14 级风险分类**（agents-best-practices）：SDK 不应该预定义"financial / communication / destructive"等标签。`Approval` 枚举已经足够表达意图。更细的分类是用户空间的事——用户可以在 Hook 中用 tool name 或自定义 tag 做进一步区分。
- **7 种权限决策**（allow / deny / ask_user / sandbox 等）：同样，三个 approval 级别足以让用户构建任何审批策略。

---

## 2. 错误处理：让失败分类成为肌肉记忆

### 研究的共识

meta-engineering-harness 的核心洞察：**错误分类是后续所有流程改进的基础**。如果你不知道一次失败是 Bug 还是 Spec Gap 还是 Noise，你无法决定重试还是修合约。

pi-subagents 的 worker agent 在遇到问题时必须上报决策——结构化错误是上报的前提。

### 当前 API

```rust
pub struct ToolError {
    pub message: String,
    pub code: Option<String>,
}
```

用户实现失败分类时只能正则匹配 `message`——这在生产环境中不可靠。

### 自然 API 应该长什么样

```rust
pub struct ToolError {
    pub message: String,
    pub kind: ErrorKind,
    pub retry: RetryHint,
    pub next_step: Option<String>,    // 告诉模型接下来可以尝试什么
}

pub enum ErrorKind {
    InvalidInput,       // 输入不合法 → 模型修正输入
    NotSupported,       // 工具不支持此操作 → 可能是 Spec Gap
    Transient,          // 超时/限流 → 重试
    Fatal,              // 内部错误/权限 → 不重试，上报
}

pub enum RetryHint {
    Safe,               // 幂等操作，可以重试
    Caution,            // 可能重复但有幂等键保护
    Unsafe,             // 不可重试（支付、发送、删除）
}
```

**为什么这个形状更自然**：

- 每个工具实现者被迫在返回错误时回答两个问题："这是什么类型的错误？"和"重试安全吗？"——这正是所有最佳实践要求的事
- 用户的 RetryPolicy 代码自然写成 `if error.retry == Safe { retry } else { abort }`——不需要解析字符串
- `next_step` 字段让模型知道"接下来可以做什么"——agents-best-practices 的结构化错误格式直接落地
- Hook 代码：`match error.kind { InvalidInput => fix_and_retry(), NotSupported => flag_spec_gap(), Transient => retry_with_backoff(), Fatal => escalate() }`

### 不需要的

- **FailureHint 或 FailureKind 枚举让用户选**：之前的方案是"SDK 提供标签，用户填充"。但这不够——标签可以被随意填充。`ErrorKind` 是强类型的 4 种分类，用户必须选一个。这比可选标签更能引导正确行为。
- **四路仲裁器**：SDK 不实现仲裁逻辑。但错误类型本身应该让仲裁器"不用想就知道怎么写"。

---

## 3. Tool 输出：让截断成为默认行为

### 研究的共识

agents-best-practices 和 harness 都强调工具结果大小控制。openclaw 和 ironclaw 的产品级实现都有这个机制。

### 当前 API

```rust
pub struct ToolMetadata {
    pub max_output_tokens: Option<u64>,   // 不存在
}
```

用户在每个 tool 的 `execute()` 中手动调用 `truncate_to_tokens()`——容易遗漏。

### 自然 API 应该长什么样

```rust
pub struct ToolMetadata {
    pub max_output_tokens: Option<u64>,
}
```

当 `max_output_tokens` 为 `Some(n)` 时，SDK 在 tool 返回结果后自动调用已有 `truncate_to_tokens()`。用户声明意图，SDK 执行。

**为什么这个形状更自然**：

- 用户定义 tool 时看到这个字段，自然想到"我这个 tool 的输出有多大？"——引导用户在注册时思考，而不是在 execute 实现里补救
- SDK 负责执行——不会遗漏

---

## 4. Agent-as-Tool：让子 agent 继承上下文成为显式选择

### 研究的共识

pi-subagents 的 fork 模式是所有复杂子 agent 场景的基础——oracle 审查父决策、worker 延续父线程、planner 基于已有上下文制定计划。

### 当前 API

```rust
impl AgentConfig {
    pub fn as_tool(&self, name: &str, description: &str) -> Arc<dyn Tool>;
}
```

子 run 始终从空 messages 开始。用户要做 fork 只能手动拼接父 messages 到 input prompt 里——笨拙且容易丢类型信息。

### 自然 API 应该长什么样

```rust
impl AgentConfig {
    pub fn as_tool(&self, name: &str, description: &str) -> SubAgentBuilder;
}

pub struct SubAgentBuilder {
    // builder 模式，链式调用
}

impl SubAgentBuilder {
    pub fn inherit_context(self, recent_messages: usize) -> Self;
    pub fn inherit_budget(self, parent_budget: &BudgetGuard) -> Self;
    pub fn input_schema(self, schema: JsonSchema) -> Self;
    pub fn output_extractor(self, f: impl Fn(Value) -> Value + Send + Sync + 'static) -> Self;
    pub fn build(self) -> Arc<dyn Tool>;
}
```

**为什么这个形状更自然**：

- 用户写 `config.as_tool("reviewer", "review my decisions").inherit_context(5).build()`——`inherit_context` 这个函数名本身就是文档，告诉用户"你可以让子 agent 看到父的历史"
- Builder 模式允许未来扩展（`inherit_budget`、`input_schema`、`output_extractor`）而不用改函数签名
- 不调 `inherit_context` 的行为与当前一致（fresh），确保后向兼容

### 常见模式在 Builder API 下的表达

```rust
// Fork 审查：继承最近 5 条消息
let reviewer = config.as_tool("oracle", "review decisions")
    .inherit_context(5)
    .build();

// Fresh 委托：空上下文
let worker = config.as_tool("worker", "implement plan")
    .build();

// 结构化输入：自定义 input schema
let analyst = config.as_tool("analyst", "analyze codebase")
    .input_schema(json!({"type": "object", "properties": {"focus": {"type": "string"}}}))
    .build();
```

三种场景都是自然的一行代码，不需要查文档。

---

## 5. Hook 系统：让正确的扩展点"伸手可及"

### 研究的共识

所有 5 份研究都依赖"在关键位置注入自定义行为"的能力——审批策略、日志、指标、失败分类、loop detection、guardrails。

### 当前设计（v0.7 spec）

```rust
#[async_trait]
pub trait Hook: Send + Sync {
    async fn on_run_start(&self, ctx: &mut RunHookContext) {}
    async fn on_run_end(&self, ctx: &RunHookContext, result: &RunResult) {}
    // ... 9 个方法，全部默认空实现
}
```

### API 自然度检查

这个 trait 设计本身没问题——所有方法默认空实现，用户只覆写关心的。但需要确认几个点：

**a）Hook 执行顺序是否可预测？**

用户注册多个 Hook 时，按注册顺序执行。这一点需要在 `with_hook()` 的文档中显式说明——顺序敏感的场景（如"先做权限检查，再做日志记录"）依赖这个 guarantee。

**b）Hook 能访问的信息是否充分？**

以 `before_tool` 为例，用户的 Hook 需要判断"这个 tool 是否危险"。如果 `ToolMetadata.approval` 换成 `Approval` 枚举（见第 1 节），Hook 代码变成：

```rust
async fn before_tool(&self, ctx: &mut ToolHookContext) -> HookAction {
    match ctx.tool_metadata.approval {
        Approval::Never => HookAction::Continue,
        Approval::WhenRisky => {
            if self.policy.allow(&ctx.tool_name, &ctx.tool_input) {
                HookAction::Continue
            } else {
                HookAction::Abort("policy denied".into())
            }
        }
        Approval::Always => HookAction::Abort("requires human approval".into()),
    }
}
```

Hook 不需要知道 tool 叫什么名字——`Approval` 枚举已经传达了意图。

**c）HookAction::Skip 的语义是否完整定义？**

当前 spec 中 `before_model` 返回 `Skip` → 跳过 model call。但 run loop 接下来做什么？如果 model 被跳过，tool_uses 为空，loop 的后续逻辑需要明确定义。这个 gap 已经在上次 v0.7 issue review 中指出。

---

## 6. 事件流：让用户消费事件时不需要"猜"

### 研究的共识

agents-best-practices 要求类型化事件——`tool_call`、`tool_result`、`approval_request`、`approval_result`、`error`——每个事件有明确的类型，不需要从 payload 中推断。

### 当前 API

```rust
pub enum RuntimeEvent {
    ToolCallStarted { tool_name: String, tool_call_id: String, ... },
    ToolCallCompleted { tool_name: String, tool_call_id: String, output: Value },
    ToolCallFailed { tool_name: String, tool_call_id: String, error: ToolError },
    // ...
}
```

### API 自然度检查

**已做好的**：
- 事件类型明确——`ToolCallStarted` 和 `ToolCallCompleted` 和 `ToolCallFailed` 各自独立类型
- Token usage 有专门字段
- Approval 有独立事件

**可以做更好的**：
- `ToolCallStarted` 里包含 `ToolMetadata` 的完整快照——让用户消费事件时不需要回头查 registry。特别是当 `Approval` 和 `max_output_tokens` 加入 metadata 后，这些信息需要在事件中可见。

---

## 7. 不动的部分

以下模式在 5 份研究中反复出现，但我们的 API 已经足够自然——用户用已有原语可以一目了然地表达：

| 模式 | 自然表达 | 为什么不需要改动 |
|------|---------|-----------------|
| Draft/Commit 分离 | 注册 `draft_email`（Never）和 `send_email`（Always） | `Approval` 枚举已经区分 |
| 独立验证 | 两个 `as_tool()` 调用，不同 system prompt | Builder API 一行代码 |
| 多 reviewer | 同上 | 同上 |
| 链式委托 | `let ctx = scout.call(); let plan = planner.call(ctx); worker.call(plan);` | async/await 就是编排 DSL |
| 并行委托 | `join!(analyst_a.call(), analyst_b.call())` | tokio 原语就是并行编排 |
| 合约编译 | 一个 Skill | SKILL.md 天然支持 |
| 校准循环 | 离线消费 event stream | 事件流已经足够丰富 |

这些不需要 SDK 改动——它们已经是用户代码的自然延伸。SDK 的工作是让底层原语（审批、错误、上下文继承）足够自然，让这些高层模式**自己浮现出来**。

---

## 汇总：3 个 API 改动

所有改动都在已有抽象上做——不引入新概念，只是让现有 API 的形状引导用户走向正确模式。

### A. `ToolMetadata.approval`: bool → Approval 枚举

```rust
pub enum Approval {
    Never,        // 只读、安全——不需要审批
    WhenRisky,    // 写入内部——用户策略决定（默认）
    Always,       // 外部通信、金融、破坏性——总是审批
}
```

影响：`ToolMetadata` 结构体、run loop 的审批分支、v0.7 Hook 上下文。没有新概念——就是把 `requires_approval: bool` 换成三态枚举。

### B. `ToolError` 增加 `kind`、`retry`、`next_step`

```rust
pub struct ToolError {
    pub message: String,
    pub kind: ErrorKind,          // InvalidInput | NotSupported | Transient | Fatal
    pub retry: RetryHint,         // Safe | Caution | Unsafe
    pub next_step: Option<String>,
}
```

影响：`ToolError` 结构体、所有 tool 实现者、v0.7 RetryPolicy。用户不再需要解析 error message——直接 match `kind` 和 `retry`。

### C. `as_tool()` : 直接返回 → Builder 模式

```rust
config.as_tool("reviewer", "review decisions")
    .inherit_context(5)
    .build()
```

影响：`AgentConfig`、`AgentAsTool`。现有 `as_tool(name, desc)` 保持不变（等同于 `.build()` 无额外配置）。Builder 链式调用自然引导用户发现 `inherit_context`、`inherit_budget`、`input_schema` 等扩展点。
