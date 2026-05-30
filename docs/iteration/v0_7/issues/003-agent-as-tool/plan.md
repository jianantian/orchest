# 003 实现路线

## 依赖

- 002（Hook Framework）完成后合入，或并行开发（本 issue 不调用 hook，hook 由外层 run loop 的 before_tool/after_tool 处理，不阻塞本 issue）
- `cargo test --workspace` 基线绿（跑一次确认）

---

## 步骤

### 步骤 1：扩展 `ToolContext`，加入 `approval_bus`

文件 `crates/agent-runtime-core/src/tool/mod.rs`，`ToolContext` 结构体（L98-105）新增字段：

```rust
pub approval_bus: Option<crate::run::handle::ApprovalBus>,
```

同时在 `run/loop_.rs:410-419` 的 `ToolContext` 构建处补充该字段：

```rust
approval_bus: Some(approval_bus.clone()),
```

`ApprovalBus` 实现 `Clone`（`Arc<Mutex<...>>`），无开销。

### 步骤 2：新建 `tool/agent_as_tool.rs`

新文件 `crates/agent-runtime-core/src/tool/agent_as_tool.rs`。

结构体定义：

```rust
pub struct AgentAsTool {
    pub(crate) config: crate::run::config::AgentConfig,
    pub(crate) model: std::sync::Arc<dyn crate::model::ModelAdapter>,
    pub(crate) registry: crate::tool::registry::ToolRegistry,
    pub(crate) tool_name: String,
    pub(crate) tool_description: String,
    pub(crate) input_schema: Option<crate::tool::JsonSchema>,
    pub(crate) output_extractor: Option<std::sync::Arc<dyn Fn(serde_json::Value) -> serde_json::Value + Send + Sync>>,
    pub(crate) metadata: crate::tool::ToolMetadata,
}
```

`Tool` impl：
- `name()` → `&self.tool_name`
- `description()` → `&self.tool_description`
- `input_schema()` → 返回 `self.input_schema.as_ref()` 或一个默认 schema（`{"type":"object","properties":{"input":{"type":"string"}},"required":["input"]}`）
- `output_schema()` → `None`
- `metadata()` → `&self.metadata`
- `execute(input, ctx)` → 核心逻辑

`execute` 逻辑（搬运自 `run/sub_agent.rs:18-154`，适配 `ToolContext` 接口）：

1. 从 `ctx.event_tx` 拿到父 run 的 sender；`ctx.run_id` 取 `parent_run_id`
2. run_depth 检查（参考 `sub_agent.rs:28-43`）：if `ctx.run_depth >= 3`，emit `SubAgentFailed`，返回含 error 的 `ToolOutput::Immediate`
3. 构建 `child_config`：clone self.config，通过 `SubAgentRuntime::cap_budget()` 继承预算，设置 `run_depth = ctx.run_depth + 1`
4. 从 `input["input"].as_str()` 提取 input string，fallback 到 `input.to_string()`
5. 从 `ctx.approval_bus` 取 `approval_bus`（unwrap_or_default）
6. `CancellationToken::new()` 创建子 token（独立取消，后续可改为子 token）
7. 调用 `AgentRun::start_with_bus_and_token(child_config, input_str, model.clone(), registry.clone(), approval_bus, cancel_token)` 得到 `(handle, mut child_rx)`，**不 spawn**，在当前 task 直接 await
8. emit `SubAgentStarted`（参考 `sub_agent.rs:61-72`）
9. 事件转发循环（参考 `sub_agent.rs:74-118`）：while loop 收 child_rx，统计 child_usage，匹配 RunCompleted/RunFailed，每个事件 wrap 成 `SubAgentEvent` 向上 emit
10. `handle.wait().await`
11. emit `SubAgentCompleted` 或 `SubAgentFailed`（参考 `sub_agent.rs:121-150`）
12. 对 output 应用 `output_extractor`（如有），返回 `Ok(ToolOutput::Immediate(model_output))`

注：子 run token 消耗通过 SubAgentEvent wrapper 中的 ModelCallCompleted 向上传递，父 run 消费端自行处理，与现有 execute_agent_delegate 一致。

### 步骤 3：在 `tool/mod.rs` 中注册新模块，删除 `pub mod agent`

文件 `crates/agent-runtime-core/src/tool/mod.rs`：

- L3 `pub mod agent;` → **删除**
- 添加 `pub mod agent_as_tool;`

### 步骤 4：修改 `ToolOutput` enum — 删除 `AgentDelegate`，添加 `Handoff` 占位

文件 `crates/agent-runtime-core/src/tool/mod.rs:38-43`：

```rust
pub enum ToolOutput {
    Immediate(Value),
    Structured { model_output: Value, details: Value },
    // AgentDelegate 已删除
    AsyncJob(JobHandle),
    Handoff(Value),  // 占位，004 中替换为 Handoff(HandoffResult)
}
```

同时删除 `AgentDelegate` struct（L52-68）和对应的 `impl std::fmt::Debug`（L60-68）。

### 步骤 5：在 `run/config.rs` 中添加 `AgentConfig::as_tool()`

文件 `crates/agent-runtime-core/src/run/config.rs`，在 `impl AgentConfig` block 中新增：

```rust
pub fn as_tool(
    &self,
    name: &str,
    description: &str,
    model: std::sync::Arc<dyn crate::model::ModelAdapter>,
    registry: crate::tool::registry::ToolRegistry,
) -> std::sync::Arc<dyn crate::tool::Tool> {
    std::sync::Arc::new(crate::tool::agent_as_tool::AgentAsTool {
        config: self.clone(),
        model,
        registry,
        tool_name: name.to_string(),
        tool_description: description.to_string(),
        input_schema: None,
        output_extractor: None,
        metadata: crate::tool::ToolMetadata {
            side_effect: false,
            requires_approval: false,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: crate::tool::ToolSource::InProcess,
        },
    })
}
```

注：`AgentConfig` 不含 `Arc<dyn ModelAdapter>`（ModelConfig 只含 ModelSpec，不含 adapter 实例），所以 `as_tool()` 接收 model + registry 参数，与 `AgentRun::start()` 的调用方式一致。

### 步骤 6：修改 `run/loop_.rs`，删除 `AgentDelegate` 处理分支

文件 `crates/agent-runtime-core/src/run/loop_.rs`：

1. L25 `use super::sub_agent::execute_agent_delegate;` → **删除**
2. L505-530 `Ok(ToolOutput::AgentDelegate(delegate)) => { ... }` 整个 arm → **删除**
3. 新增 `Ok(ToolOutput::Handoff(_)) => { /* TODO: 004 */ continue; }` arm，防止非穷尽匹配编译报错

### 步骤 7：删除 `run/sub_agent.rs`

`execute_agent_delegate()` 逻辑已迁入 `AgentAsTool::execute()`，删除整个文件。

同时在 `run/mod.rs` 中删除 `pub(crate) mod sub_agent;`。

确认无其他引用：

```bash
grep -rn "sub_agent\|execute_agent_delegate" crates/ --include="*.rs"
```

### 步骤 8：删除 `tool/agent.rs`

整体删除，已在步骤 3 删了 `pub mod agent;`。

### 步骤 9：迁移 `run/tests.rs` 中的 sub-agent 测试

文件 `crates/agent-runtime-core/src/run/tests.rs`：

1. 删除 `AgentDelegate,` import（约 L11）
2. 约 L1976-L2125 的 `SpawnSubTool` struct：其 `execute` 目前返回 `ToolOutput::AgentDelegate(...)`
   - 删除 `SpawnSubTool`，在测试 setup 中直接用 `child_config.as_tool("spawn_sub", "...", child_model, child_registry)` 注册进父 registry
   - `SubAgentApprovalModel` 等辅助结构保留，调整注册方式即可

### 步骤 10：迁移 `tests/v03_runtime.rs` 中的 sub-agent 测试

文件 `crates/agent-runtime-core/tests/v03_runtime.rs`：

1. 删除 `use agent_runtime_core::tool::agent::AgentTool;`（约 L12）
2. 删除 `AgentDelegate,` import（约 L15）
3. 约 L178-235 `SpawnChildTool` → 改用 `child_config.as_tool("spawn_child", "...", child_model, ToolRegistry::new())`
4. 约 L376-419 `agent_tool_runs_child_agent_with_isolated_context` 中的 `AgentTool::new(...)` → 同上替换

### 步骤 11：清理 `tool/mod.rs` 中已无用的 imports

确认 `use crate::model::ModelAdapter`、`use crate::run::AgentConfig`、`use crate::tool::registry::ToolRegistry` 是否仍被 mod.rs 本身使用；若仅由已删除的 `AgentDelegate` 使用则删除。

---

## 验证

```bash
cargo build --workspace 2>&1 | head -50

cargo test --workspace 2>&1 | tail -30

cargo clippy --workspace -- -D warnings 2>&1 | head -50

# 确认 AgentDelegate 彻底清除
grep -rn "AgentDelegate" crates/ --include="*.rs"
# 期望：零结果

# 确认 execute_agent_delegate 清除
grep -rn "execute_agent_delegate" crates/ --include="*.rs"
# 期望：零结果

# 确认 __sub_agent_request 不存在
grep -rn "__sub_agent_request" crates/ --include="*.rs"
# 期望：零结果
```

---

## 关键决策

**`as_tool()` 需要调用方传 model + registry**

`AgentConfig`（`config.rs:38-46`）只含 `ModelConfig`（含 `ModelSpec`，不含 adapter 实例），不含 `ToolRegistry`。Config 是可序列化的，adapter 是运行时对象。因此 `as_tool()` 签名接收 `Arc<dyn ModelAdapter>` 和 `ToolRegistry`，与 `AgentRun::start()` 调用方式一致。

**approval_bus 传入 ToolContext**

`execute_agent_delegate` 通过参数接收 `approval_bus`。迁移到 `Tool::execute()` 后通过 `ToolContext.approval_bus` 字段传递，与现有 `event_tx: Option<...>` 风格一致，最小侵入。
