# 003 · Agent-as-Tool + Old Path Cleanup

## 背景

当前 sub-agent 有两条并行路径做几乎一样的事：

1. `ToolOutput::AgentDelegate` + `execute_agent_delegate()`（`run/sub_agent.rs`）— trait 驱动
2. `__sub_agent_request` 魔法 JSON 字段 + `execute_sub_agent_request()`（如果存在）— 值检测驱动

两条路径都是 "call agent, get result" 语义（Agent-as-Tool），各自维护一套 spawn + 事件转发 + 预算继承 + 审批路由逻辑。

本 issue 用统一的 `AgentAsTool` 实现替代两条旧路径，对标 OpenAI SDK `agent.as_tool()`。

设计文档：[sub-agent-handoff-vs-agent-as-tool.md](../../../research/sub-agent-handoff-vs-agent-as-tool.md) §3.2

## 目标

提供 `AgentConfig::as_tool()` API，将 agent 配置转换为普通 `Tool`——模型看到的是 FunctionTool，不知道里面跑了 agent。同时删除所有旧 sub-agent 路径。

## 范围

### 新增：`AgentAsTool`

新文件 `tool/agent_as_tool.rs`：

```rust
pub struct AgentAsTool {
    config: AgentConfig,
    tool_name: String,
    tool_description: String,
    input_schema: Option<JsonSchema>,
    output_extractor: Option<Arc<dyn Fn(Value) -> Value + Send + Sync>>,
}

#[async_trait]
impl Tool for AgentAsTool {
    fn name(&self) -> &str { &self.tool_name }
    fn description(&self) -> &str { &self.tool_description }
    fn input_schema(&self) -> &JsonSchema { /* 默认：{ "input": "string" } */ }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        // 1. 构建子 run 的 input prompt
        // 2. 调用 AgentRun::start()（同步等待，不 spawn tokio task）
        // 3. 转发子 run 事件作为 SubAgentEvent
        // 4. 提取输出（output_extractor 或默认取 final_output）
        // 5. 返回 ToolOutput::Immediate(result)
    }
}
```

关键设计决策：
- **不 spawn tokio task**：子 run 在当前 task 中同步执行。避免审批路由和 child tracking 的复杂性。**但父 agent 的事件流不会静默**——子 run 的每个 `RuntimeEvent` 实时通过 `SubAgentEvent` wrapper 向上传播到父 agent 的 event channel，消费者看到连续的事件流（只是 wrapped），而不是长时间的空白
- **超时保护**：Agent-as-Tool 继承 `ToolMetadata.timeout`。如果 timeout 被设置，子 run 在 `tokio::time::timeout` 内执行，超时返回 `ToolError::Timeout`。如果 timeout 未设置，使用 `AgentConfig.budget` 的 step/token/cost 上限作为隐式保护
- **结构化输入**：可选 `input_schema` 提供 JSON Schema，自动构建 prompt
- **预算继承**：始终继承父 run 剩余预算——Agent-as-Tool 是工具调用，语义上与普通 tool 一致，tool 不应有独立于调用者的预算

### 新增：`AgentConfig::as_tool()` builder

在 `run/config.rs` 中新增：

```rust
impl AgentConfig {
    pub fn as_tool(&self, name: &str, description: &str) -> Arc<dyn Tool> {
        Arc::new(AgentAsTool {
            config: self.clone(),
            tool_name: name.to_string(),
            tool_description: description.to_string(),
            input_schema: None,
            output_extractor: None,
        })
    }
}
```

### 删除：旧 sub-agent 路径

| 删除目标 | 文件 | 说明 |
|----------|------|------|
| `AgentDelegate` struct | `tool/mod.rs:52-68` | 被 `AgentAsTool` 替代 |
| `ToolOutput::AgentDelegate` variant | `tool/mod.rs:41` | 不再需要 |
| `execute_agent_delegate()` | `run/sub_agent.rs:18+` | 逻辑合入 `AgentAsTool::execute()` |
| `AgentTool` struct | `tool/agent.rs` 整个文件 | 被 `AgentConfig::as_tool()` 替代 |
| `pub mod agent;` | `tool/mod.rs:3` | 删除旧模块 |
| `AgentDelegate` 处理分支 | `run/loop_.rs:505-520` | 不再需要 |
| `AgentDelegate` 相关 import | `run/loop_.rs:25`, `run/tests.rs:11` | 清理 |
| `__sub_agent_request` 魔法字段检测 | 如果存在于 `run/loop_.rs` 或 `run/tool_exec.rs` | 彻底删除 |

### 修改：`ToolOutput` enum

```rust
pub enum ToolOutput {
    Immediate(Value),
    Structured { model_output: Value, details: Value },
    // AgentDelegate 删除
    AsyncJob(JobHandle),
    Handoff(HandoffResult),  // 004 新增，本 issue 先加 variant 占位
}
```

`Handoff` variant 在本 issue 中先声明但不实现处理逻辑（004 的事）。

### 修改：测试

`run/tests.rs` 中使用 `AgentDelegate` 的测试（约 L1976-L2120）需要迁移为使用 `AgentAsTool`：
- `DelegatingTool` → 改为返回 `ToolOutput::Immediate`，通过 `AgentConfig::as_tool()` 注册子 agent
- 审批路由测试保持覆盖

## 不在范围内

- Handoff 实现（004）
- AgentRun 重构为 actor（005）
- 子 run 的独立预算模式（v0.7 spec 提到 Handoff 时继承/独立预算，但 Agent-as-Tool 始终继承父 run 剩余预算）

## 依赖

- 002（Hook Framework）：`AgentAsTool::execute()` 内部的子 run 需要触发 hook（`before_tool` / `after_tool` 由外层 run loop 调用，子 run 内部有自己的 hook 链）

## 验收标准

- [ ] `AgentConfig::as_tool()` 可用，返回 `Arc<dyn Tool>`
- [ ] `AgentAsTool::execute()` 同步执行子 run，不 spawn tokio task
- [ ] 子 run 事件通过 `SubAgentEvent` 正确向上传播
- [ ] 子 run 继承父 run 剩余预算
- [ ] ToolMetadata.timeout 被尊重——超时时子 run 终止并返回 ToolError::Timeout
- [ ] `crates/` 中无 `AgentDelegate` 字符串（import、struct、variant 全部清除）
- [ ] `crates/` 中无 `__sub_agent_request` 字符串
- [ ] `tool/agent.rs` 文件删除
- [ ] 现有 sub-agent 相关测试迁移为使用 `AgentAsTool`，覆盖不降低
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
