# 004 实现路线

## 依赖

- 003（Agent-as-Tool）必须先完成：
  - `ToolOutput::AgentDelegate` 已删除
  - `ToolOutput::Handoff(Value)` 占位 variant 已存在
  - `ToolContext.approval_bus` 字段已加入
- `cargo test --workspace` 基线绿（003 完成后验证）

---

## 步骤

### 步骤 1：新建 `handoff.rs` — 核心类型定义

新文件 `crates/agent-runtime-core/src/handoff.rs`：

```rust
use std::sync::Arc;
use async_trait::async_trait;
use serde_json::Value;
use crate::model::{ContentBlock, Message};
use crate::run::config::AgentConfig;
use crate::tool::Tool;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct HandoffError {
    pub message: String,
}

pub struct HandoffInputData {
    pub input_history: Vec<Message>,
    pub pre_handoff_items: Vec<ContentBlock>,
    pub new_items: Vec<ContentBlock>,
}

#[async_trait]
pub trait HandoffInputFilter: Send + Sync {
    async fn filter(&self, data: HandoffInputData) -> HandoffInputData;
}

#[async_trait]
pub trait HandoffResolver: Send + Sync {
    async fn resolve(&self, input: Value) -> Result<AgentConfig, HandoffError>;
}

pub enum HandoffTarget {
    Static(AgentConfig),
    Dynamic(Arc<dyn HandoffResolver>),
}

// Handoff 需要 Clone（注册到 config.handoffs Vec），用 Arc 包 trait object
#[derive(Clone)]
pub struct Handoff {
    pub tool_name: String,
    pub tool_description: String,
    pub input_schema: Value,
    pub target: Arc<HandoffTarget>,
    pub input_filter: Option<Arc<dyn HandoffInputFilter>>,
    pub nest_history: bool,
}

pub struct HandoffResult {
    pub target_config: AgentConfig,
    pub transfer_message: String,
    pub nest_history: bool,
    pub input_filter: Option<Arc<dyn HandoffInputFilter>>,
    pub tool_call_id: String,
}
```

在 `lib.rs` 中注册：`pub mod handoff;`

### 步骤 2：新建 `tool/handoff_tool.rs`

新文件 `crates/agent-runtime-core/src/tool/handoff_tool.rs`：

```rust
pub struct HandoffTool {
    pub(crate) handoff: crate::handoff::Handoff,
}
```

`Tool` impl：
- `name()` → `&self.handoff.tool_name`
- `description()` → `&self.handoff.tool_description`
- `input_schema()` → `&self.handoff.input_schema`（需要 Handoff.input_schema 是 JsonSchema 类型）
- `output_schema()` → `None`
- `metadata()` → `ToolMetadata { side_effect: false, requires_approval: false, ..., source: ToolSource::InProcess }`
- `execute(input, ctx)` →
  1. 从 `input["reason"].as_str().unwrap_or("")` 取 transfer_message
  2. 解析 target config：
     - `HandoffTarget::Static(ref cfg)` → `cfg.clone()`
     - `HandoffTarget::Dynamic(ref resolver)` → `resolver.resolve(input.clone()).await.map_err(|e| ToolError { message: e.message, code: None })?`
  3. 返回 `Ok(ToolOutput::Handoff(HandoffResult { target_config, transfer_message, nest_history: self.handoff.nest_history, input_filter: self.handoff.input_filter.clone(), tool_call_id: ctx.tool_call_id.clone() }))`

注：`filtered_messages` 不在 execute() 中构建（execute() 无法访问 messages 历史），移到 loop_.rs 的 Handoff 处理分支中构建。

在 `tool/mod.rs` 中添加 `pub mod handoff_tool;`。

### 步骤 3：实现 `Handoff::as_tool()`

在 `handoff.rs` 中：

```rust
impl Handoff {
    pub fn as_tool(&self) -> std::sync::Arc<dyn crate::tool::Tool> {
        std::sync::Arc::new(crate::tool::handoff_tool::HandoffTool {
            handoff: self.clone(),
        })
    }
}
```

### 步骤 4：将 `ToolOutput::Handoff` 从 `Value` 替换为 `HandoffResult`

文件 `crates/agent-runtime-core/src/tool/mod.rs`，将 003 添加的占位 `Handoff(Value)` 替换为：

```rust
Handoff(crate::handoff::HandoffResult),
```

为 `HandoffResult` 补充 `impl std::fmt::Debug`（手写或 derive）。

### 步骤 5：在 `events.rs` 中添加 `AgentUpdated` variant

文件 `crates/agent-runtime-core/src/events.rs`，在 `RunCompleted`（约 L129）之前新增：

```rust
AgentUpdated {
    previous_agent: String,
    new_agent: String,
},
```

### 步骤 6：在 `run/config.rs` 中添加 handoffs 字段和 `with_handoff()` builder

文件 `crates/agent-runtime-core/src/run/config.rs`：

1. `AgentConfig` struct（L38-46）新增字段：
   ```rust
   #[serde(skip)]
   pub handoffs: Vec<crate::handoff::Handoff>,
   ```

2. 在 `impl AgentConfig` block 新增：
   ```rust
   pub fn with_handoff(mut self, handoff: crate::handoff::Handoff) -> Self {
       self.handoffs.push(handoff);
       self
   }
   ```

### 步骤 7：在 `run/loop_.rs` 中注册 handoff tools

文件 `crates/agent-runtime-core/src/run/loop_.rs`，在 `connect_mcp_servers`（L83）之后、`filter_by_allowed`（L126）之前，注册 handoff tools：

```rust
for handoff in &config.handoffs {
    let tool = handoff.as_tool();
    if let Err(error) = registry.register(tool) {
        emit(&tx, RuntimeEvent::RuntimeWarning {
            message: format!("failed to register handoff tool: {error}"),
        }).await;
    }
}
```

### 步骤 8：修改 `run/loop_.rs`——Handoff 分支处理

**8.1** 将 `run_loop_inner` 的 `config: AgentConfig` 参数改为 `mut config: AgentConfig`（L64）。

**8.2** 在 tool dispatch for 循环之前（`let mut tool_results = Vec::new();`，约 L297）添加：

```rust
let mut handoff_triggered = false;
```

**8.3** 将 `for tool_call in &tool_uses {`（约 L299）改为：

```rust
'tool_loop: for tool_call in &tool_uses {
```

**8.4** 在 `Ok(ToolOutput::AsyncJob(handle))` arm 之前新增 `Ok(ToolOutput::Handoff(handoff_result))` arm：

```rust
Ok(ToolOutput::Handoff(handoff_result)) => {
    if handoff_triggered {
        tool_results.push(ContentBlock::ToolResult {
            tool_use_id: tool_call.id.clone(),
            content: json!({"error": "Only one handoff per turn is allowed"}),
        });
        budget.record_tool_call();
        continue;
    }
    handoff_triggered = true;

    // 1. 记录 tool result，保持 transcript 完整
    tool_results.push(ContentBlock::ToolResult {
        tool_use_id: tool_call.id.clone(),
        content: json!({"result": handoff_result.transfer_message}),
    });

    // 2. 调用 on_handoff hook（002 完成后补充）

    // 3. 发出 AgentUpdated 事件
    emit(&tx, RuntimeEvent::AgentUpdated {
        previous_agent: config.system_prompt[..config.system_prompt.len().min(60)].to_string(),
        new_agent: handoff_result.target_config.system_prompt[..handoff_result.target_config.system_prompt.len().min(60)].to_string(),
    }).await;

    // 4. 构建新消息列表（apply nest_history + input_filter）
    let new_messages = build_handoff_messages(
        &messages,
        &handoff_result,
    ).await;

    // 5. 预算处理（见步骤 9）
    let remaining = budget.remaining_config();
    if handoff_result.target_config.budget == crate::budget::BudgetConfig::default() {
        budget.reset_remaining(remaining);
    } else {
        let capped = crate::run::config::SubAgentRuntime::cap_budget(
            &handoff_result.target_config.budget, &remaining);
        budget = crate::budget::BudgetGuard::new(capped);
    }

    // 6. 切换 config + messages + registry
    config = handoff_result.target_config;
    messages = new_messages;

    // 重建 registry（仅 in-process handoff tools；MCP/skills 暂不重连）
    let mut new_registry = crate::tool::registry::ToolRegistry::new();
    for h in &config.handoffs {
        let _ = new_registry.register(h.as_tool());
    }
    registry = new_registry;
    tool_defs = registry.list();

    budget.record_tool_call();
    break 'tool_loop;
}
```

**8.5** 在 `run_loop_inner` 中引入 `build_handoff_messages` 辅助函数（定义见步骤 10）。

### 步骤 9：在 `budget.rs` 中新增 `BudgetGuard::reset_remaining()`

文件 `crates/agent-runtime-core/src/budget.rs`，在 `impl BudgetGuard` 中新增：

```rust
pub fn reset_remaining(&mut self, remaining: BudgetConfig) {
    self.config = remaining;
    self.usage = BudgetUsage::default();
    self.start = std::time::Instant::now();
}
```

同时检查 `BudgetGuard` 字段名是否与实际代码一致（`budget.rs:` 中 `config`、`usage`、`start` 字段名）。

### 步骤 10：实现 `build_handoff_messages` 辅助函数

在 `run/helpers.rs` 或 `run/loop_.rs` 内部：

```rust
async fn build_handoff_messages(
    messages: &[crate::model::Message],
    result: &crate::handoff::HandoffResult,
) -> Vec<crate::model::Message> {
    use crate::model::{ContentBlock, Message, Role};
    use serde_json::to_string_pretty;

    let system_msg = messages.first().cloned();
    let history: Vec<Message> = messages.iter().skip(1).cloned().collect();

    let mut new_messages = Vec::new();
    if let Some(sys) = system_msg {
        new_messages.push(sys);
    }

    if result.nest_history && !history.is_empty() {
        // 折叠历史为单条消息
        let history_text = to_string_pretty(&history).unwrap_or_default();
        let folded = format!("<CONVERSATION HISTORY>\n{history_text}\n</CONVERSATION HISTORY>");
        new_messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text(folded)],
        });
    } else {
        // 应用 input_filter（如有）
        if let Some(ref filter) = result.input_filter {
            use crate::handoff::HandoffInputData;
            let data = HandoffInputData {
                input_history: history,
                pre_handoff_items: vec![],
                new_items: vec![],
            };
            let filtered = filter.filter(data).await;
            new_messages.extend(filtered.input_history);
        } else {
            new_messages.extend(history);
        }
    }

    new_messages
}
```

### 步骤 11：编写测试

在 `crates/agent-runtime-core/src/run/tests.rs` 末尾新增：

**测试 A**：Static handoff，A → B，B 完成对话，验证 `AgentUpdated` 事件发出

**测试 B**：同一 turn 两个 handoff，只执行第一个，第二个收到 error tool result

**测试 C**：`nest_history = true` 时，新 agent 消息历史被折叠为单条

---

## 验证

```bash
cargo build --workspace 2>&1 | head -50

cargo test --workspace 2>&1 | tail -40

cargo clippy --workspace -- -D warnings 2>&1 | head -50

# 确认 AgentUpdated 事件存在
grep -rn "AgentUpdated" crates/ --include="*.rs"

# 确认 HandoffResult 类型存在
grep -rn "HandoffResult" crates/ --include="*.rs"

# 确认 ToolOutput::Handoff 类型正确
grep -n "Handoff" crates/agent-runtime-core/src/tool/mod.rs
```

---

## 关键决策

**`filtered_messages` 不在 `HandoffTool::execute()` 中构建**

`execute()` 时没有 messages 历史（`ToolContext` 不含消息）。`input_filter` 和 `nest_history` 逻辑下移到 loop_.rs 的 Handoff 处理分支。`HandoffResult` 携带 `nest_history` flag 和 `input_filter` Arc 传给 loop_.rs。

**handoff 后 registry 重建只含 in-process tools**

本 issue 仅重建 in-process handoff tools。MCP servers 需要异步重连，specs 未明确要求 handoff 后 MCP/skills 重新连接。完整重建留给后续 issue（005 actor 重构时统一处理）。

**`run_loop_inner` 中 `config` 改为 mut**

Handoff 需要切换 config，`run_loop_inner` 的 `config: AgentConfig`（L64）需改为 `mut config`。内部函数变更，无公开 API 影响。

**BudgetGuard::reset_remaining 语义**

reset 后 usage 归零，等价于"新 agent 从 remaining budget 开始，内部使用计数重置"。这是 Handoff"同一会话延续"语义——新 agent 能使用的最多是剩余的量，但内部计数从头开始。
