# 001 · Hook Contract Extension

## 背景

v0.7 Hook 框架定义了 `HookAction { Continue, Skip, Abort(String) }` 和 `ToolHookContext`，但缺少两个 v0.8 Guardrail 层所需的能力：

1. `before_tool` 目前只能 Continue / Skip（跳过，回传固定字符串）/ Abort（终止 run）。Guardrail 需要"拒绝此次工具调用，并把**用户指定的原因**回传给模型"——这是 `Reject(String)` 语义，与 Skip 不同（Skip 原因固定、语义是内部跳过；Reject 原因由 guardrail 决定、语义是"拒绝 + 告知模型换策略"）。
2. `after_tool` 的 `ToolHookContext` 没有 `tool_output` 字段，`OutputToolGuardrail` 无法读取或修改工具输出。

这两处是对**现有公共类型**的最小扩展，须在 Guardrail Framework（002）前落地，作为 002 的前置条件。

## 目标

以最小改动扩展 Hook 契约，使 `before_tool` 支持带原因的拒绝语义，`after_tool` 支持读取和修改工具输出。

## 范围

### HookAction 新增 Reject 变体

```rust
// crates/agent-runtime-core/src/hook/mod.rs
pub enum HookAction {
    Continue,
    Skip,           // 保持现有语义：跳过工具，回传固定字符串 "tool call skipped by hook"
    Abort(String),
    Reject(String), // 新增：拒绝工具调用，以 reason 作为 tool result 内容回传给模型
}
```

**Reject 与 Skip 的区别：**
- `Skip`：内部流程控制，reason 固定，模型看到的是通用提示
- `Reject`：显式拒绝，reason 是 guardrail 提供的上下文（如 "contains banned keyword: DROP TABLE"），模型据此调整策略

`Reject` 仅在 `before_tool` 路径上有意义；若在 `after_tool` 或 `before_compact` 中返回，runner 当作 `Skip` 处理（记录 warning event）。

### ToolHookContext 新增 tool_output 字段

```rust
// crates/agent-runtime-core/src/hook/mod.rs
pub struct ToolHookContext {
    pub run_id: crate::run::RunId,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_metadata: crate::tool::ToolMetadata,
    pub tool_output: Option<serde_json::Value>,  // 新增
}
```

- `before_tool` 调用时：`tool_output = None`
- `after_tool` 调用时：`tool_output = Some(actual_output)`；hook 可修改此值；run loop 在 hook 链完成后，若 `ctx.tool_output` 与传入值不同（or 任意 hook 写入过），用修改后的值替换 tool_results 中对应条目的 content

### runner.rs 更新

`run_before_tool` 新增 `Reject` arm：

```rust
Ok(HookAction::Reject(reason)) => return HookAction::Reject(reason),
```

`run_after_tool` 接受初始 `tool_output`，填入 ctx，完成后返回最终 `tool_output`（hook 可能修改）：

```rust
pub(crate) async fn run_after_tool(
    hooks: &[Arc<dyn Hook>],
    ctx: &mut ToolHookContext,   // ctx.tool_output 已预填充
    tx: &mpsc::Sender<RuntimeEvent>,
) -> HookAction   // 返回值不变，但 ctx.tool_output 可能被 hook 修改
```

调用方在 `run_after_tool` 返回后检查 `ctx.tool_output` 是否被修改，并据此更新 tool_results。

### actor.rs 更新

在 `before_tool` 调用结果处理中，新增 `Reject` arm：

```rust
crate::hook::HookAction::Reject(reason) => {
    tool_results.push(ContentBlock::ToolResult {
        tool_use_id: tool_call.id.clone(),
        content: json!({"error": reason}),
    });
    state.budget.record_tool_call();
    continue;
}
```

在各 `after_tool` 调用后，若 `tool_out_ctx.tool_output` 被修改，替换已推入 `tool_results` 的最后一项 content。

## 验收标准

- [ ] `HookAction::Reject(String)` 变体存在，编译通过
- [ ] `before_tool` 返回 `Reject(reason)` 时，tool 不执行，`{"error": reason}` 作为 tool result 推入 tool_results，run loop 继续（不 abort）
- [ ] `after_tool` 返回 `Reject` 时，runner 当作 `Skip` 处理并发出 RuntimeWarning event
- [ ] `ToolHookContext.tool_output` 字段存在：before_tool 时为 `None`，after_tool 时为实际输出值
- [ ] `after_tool` hook 修改 `ctx.tool_output` 后，tool_results 中对应 content 被替换
- [ ] 现有使用 `HookAction::Skip` / `HookAction::Continue` / `HookAction::Abort` 的代码不受影响
- [ ] `LoopDetectionHook`（唯一现有 Hook 实现）不需要修改（默认实现覆盖）
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

## 注意事项

- `ToolHookContext` 中 `tool_output` 的修改检测：不要用 `ptr_eq` 或 dirty flag，直接在 `run_after_tool` 返回后读取 `ctx.tool_output`，若为 `Some(v)`，始终用 `v` 替换 tool_result（after_tool 阶段原本就有输出，None 只在 before_tool 时出现）
- `after_tool` 在多处 `ToolOutput` 分支各自调用（Text / Structured / AsyncJob 等），每处都需要做 ctx 填充 + 替换逻辑；用 helper 函数避免重复
- 本 issue 不引入新公共 API 除类型变更，无 binding crate（py/node）改动
