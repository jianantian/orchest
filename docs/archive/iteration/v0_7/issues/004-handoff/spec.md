# 004 · Handoff + AgentUpdated Event

## 背景

Agent-as-Tool（003）解决了"委托子任务"语义。但还缺少"会话路由"语义——当前 agent 将整个会话交给另一个 agent，自身退出。

对标 OpenAI SDK `Handoff`（`run_loop.py:803-815`），这是 run loop 层面的控制流切换，不是 tool 层面的委托。模型通过特殊 tool 主动选择目标 agent。

设计文档：[sub-agent-handoff-vs-agent-as-tool.md](../../../research/sub-agent-handoff-vs-agent-as-tool.md) §3.3

## 目标

在 run loop 中实现 Handoff 控制流切换——模型调用 `transfer_to_X` 工具时，当前 agent 退出，目标 agent 接管会话。

## 范围

### 新增：Handoff 类型

新文件 `handoff.rs`：

```rust
pub struct Handoff {
    pub tool_name: String,
    pub tool_description: String,
    pub input_schema: Value,
    pub target: HandoffTarget,
    pub input_filter: Option<Arc<dyn HandoffInputFilter>>,
    pub nest_history: bool,
}

pub enum HandoffTarget {
    Static(AgentConfig),
    Dynamic(Arc<dyn HandoffResolver>),
}

#[async_trait]
pub trait HandoffResolver: Send + Sync {
    async fn resolve(&self, input: Value) -> Result<AgentConfig, HandoffError>;
}

#[async_trait]
pub trait HandoffInputFilter: Send + Sync {
    async fn filter(&self, data: HandoffInputData) -> HandoffInputData;
}

pub struct HandoffInputData {
    pub input_history: Vec<Message>,
    pub pre_handoff_items: Vec<ContentBlock>,
    pub new_items: Vec<ContentBlock>,
}

pub struct HandoffResult {
    pub target_agent: AgentConfig,
    pub transfer_message: String,
    pub filtered_input: Vec<Message>,
}
```

### 新增：Handoff Tool

`Handoff` 需要转换为 `Tool` 注册到 `ToolRegistry`——模型看到的是一个叫 `transfer_to_billing`（例）的普通工具。

```rust
impl Handoff {
    pub fn as_tool(&self) -> Arc<dyn Tool> {
        Arc::new(HandoffTool { handoff: self.clone() })
    }
}

// HandoffTool::execute() 返回 ToolOutput::Handoff(HandoffResult)
```

### 新增：AgentConfig 集成

```rust
impl AgentConfig {
    pub fn with_handoff(mut self, handoff: Handoff) -> Self {
        // 将 handoff.as_tool() 注册到 tools 列表
        self
    }
}
```

### 修改：Run Loop Handoff 分支

在 `run/loop_.rs` 的 tool dispatch 中新增 `ToolOutput::Handoff` 处理：

```rust
Ok(ToolOutput::Handoff(handoff)) => {
    // 1. 记录 handoff tool result（让 transcript 完整）
    tool_results.push(ToolResult {
        tool_use_id: tool_call.id.clone(),
        content: json!({"result": handoff.transfer_message}),
    });

    // 2. 调用 on_handoff hook（002 提供的 hook 点）
    for hook in &config.hooks {
        hook.on_handoff(&HandoffHookContext { ... }).await;
    }

    // 3. 发出 AgentUpdated 事件
    emit(&tx, RuntimeEvent::AgentUpdated {
        previous_agent: config.name(),
        new_agent: handoff.target_agent.name(),
    }).await;

    // 4. 切换 config + registry + messages
    config = handoff.target_agent;
    registry = build_registry(&config).await?;
    messages = handoff.filtered_input;

    // 5. 跳过剩余 tool calls，重新进入 loop
    break 'tool_loop;
}
```

### 新增：`RuntimeEvent::AgentUpdated`

在 `events.rs` 中新增：

```rust
AgentUpdated {
    previous_agent: String,
    new_agent: String,
},
```

### Budget 处理

Handoff 时的 budget 策略：

- **默认**：继承父 agent 剩余预算（Handoff 是同一会话的延续）
- **可选**：通过 `HandoffTarget` 配置独立预算（目标 AgentConfig 自带的 BudgetConfig 覆盖）

```rust
// run loop handoff 分支中
let remaining = budget_guard.remaining();
if handoff.target_agent.budget == BudgetConfig::default() {
    // 目标未配置独立预算 → 继承剩余
    budget_guard.reset_to(remaining);
} else {
    // 目标有独立预算 → 使用目标配置
    budget_guard = BudgetGuard::new(handoff.target_agent.budget.clone());
}
```

### 嵌套 Handoff 作用域

如果 Agent-as-Tool 调用的子 agent 内部触发了 Handoff，handoff 作用于**子 agent 的 run loop scope**，不冒泡到父 agent 的 session。父 agent 看到的仍然是一个普通 tool call 的结果——子 agent 内部的 agent 切换对父 agent 透明。

这与 Agent-as-Tool 的"工具级委托"语义一致：子 run 是一个黑盒，内部实现（包括 handoff）不泄漏到外层。

### 多 Handoff 检测

与 OpenAI 一致——如果模型在同一 turn 调用多个 handoff tool，只执行第一个，其余作为 error tool result 返回：

```rust
if handoff_already_triggered {
    tool_results.push(ToolResult::error(
        tool_call.id,
        "Only one handoff per turn is allowed",
    ));
    continue;
}
```

### `nest_history` 实现

当 `nest_history = true` 时，将上游对话历史折叠为一条 assistant message：

```
<CONVERSATION HISTORY>
[序列化的历史消息]
</CONVERSATION HISTORY>
```

新 agent 看到的是一条消息而非完整多 turn 历史，减少 token 消耗。

## 需要修改的文件

| 文件 | 变更 |
|------|------|
| 新增 `handoff.rs` | Handoff / HandoffTarget / HandoffInputFilter / HandoffInputData / HandoffResult |
| 新增 `tool/handoff_tool.rs` | HandoffTool（impl Tool） |
| `tool/mod.rs` | `ToolOutput::Handoff` variant（003 已占位） |
| `events.rs` | `RuntimeEvent::AgentUpdated` |
| `run/loop_.rs` | Handoff 分支处理 + 多 handoff 检测 |
| `run/config.rs` | `AgentConfig::with_handoff()` builder |
| `budget.rs` | Handoff budget 继承/独立逻辑 |

## 不在范围内

- Agent-as-Tool（003 已完成）
- `active_children` HashMap 清理——如果 003 未删除，本 issue 删除（Handoff 不 spawn 子 task，不需要 child tracking）
- Handoff 的 session snapshot 支持 → v0.8

## 依赖

- 002（Hook Framework）：`on_handoff` hook 调用
- 003（Agent-as-Tool）：`ToolOutput` enum 变更（`AgentDelegate` 已删除，`Handoff` variant 已占位）

## 验收标准

- [ ] `Handoff` 类型定义完整（Static / Dynamic target）
- [ ] `HandoffInputFilter` trait 可用，`HandoffInputData` 正确传递上下文
- [ ] `nest_history` 正确折叠历史为嵌套消息
- [ ] `AgentConfig::with_handoff()` builder 可用
- [ ] Run loop 中 handoff 分支正确切换 agent，后续 turn 使用新 agent
- [ ] `RuntimeEvent::AgentUpdated` 在 handoff 时正确发出
- [ ] `on_handoff` hook 在 handoff 时被调用
- [ ] Budget 继承/独立逻辑正确
- [ ] 多 handoff 检测：同一 turn 多个 handoff 只执行第一个
- [ ] 测试：static handoff 场景（A → B，B 继续对话）
- [ ] 测试：dynamic handoff 场景（根据 input 选择目标）
- [ ] 测试：input_filter 裁减上下文
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
