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

### ModelHookContext 新增 response 载体（after_model 输出可见 + 可改）

当前 `after_model` 的 `ModelHookContext` 只携带 `messages`（发给模型的**输入** `call_messages`），不含模型的**响应**；且对 `model_ctx` 的修改在 `break r` 后被丢弃（loop 用原始 `r`）。结果是 `after_model` hook 既看不到也改不了模型输出——002 的 `OutputGuardrail`（无论"检查输出后 Abort"还是"Replace 输出"）都无法实现。

对称于 `before_model`（其 `ctx.messages` 已被 loop 在 line 431 读回作为 `call_messages`），v0.8 给 `ModelHookContext` 增加响应载体：

```rust
// crates/agent-runtime-core/src/hook/mod.rs
pub struct ModelHookContext {
    pub run_id: crate::run::RunId,
    pub messages: Vec<crate::model::Message>,
    pub model_spec: crate::model::ModelSpec,
    pub response: Option<Vec<crate::model::ContentBlock>>,  // 新增
}
```

- `before_model` 调用时：`response = None`（响应尚不存在）
- `after_model` 调用时：`response = Some(r.content.clone())`（模型输出）；hook 可修改此值
- run loop 在 `after_model` hook 链完成后读回 `model_ctx.response`，若为 `Some(modified)`，用 `modified` 替换 `r.content`，**再** append 到 `state.messages`（line 592）

这样 `after_model` hook（含 OutputGuardrail）既能审查模型输出、也能改写它，且改写真正落到对话历史。`messages` 字段在 after_model 时仍为输入上下文（只读参考），`response` 字段是输出。

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

### 执行顺序重排：`before_tool` 必须先于 approval

当前 `actor.rs` 工具调用顺序为：

```
① approval check（:626）→ ② budget(max_tool_calls) check → ③ before_tool hooks（:717）→ ④ execute
```

这个顺序有一个**正确性缺陷**：`before_tool` 通过 `&mut ctx.tool_input` 修改输入（002 的 `ToolInputGuardrail::Modify` 即基于此），但审批发生在修改之前——用户审批的是**原始**输入，实际执行的是**修改后**输入。审批失去意义，且是安全隐患。

001 将顺序重排为：

```
① before_tool hooks → ② approval check → ③ budget(max_tool_calls) check → ④ execute
```

重排后的语义：
- `before_tool` 返回 `Skip` / `Reject` / `Abort` → 不进入审批（跳过/拒绝/终止的工具无需审批）
- `before_tool` 返回 `Continue` → 用 **处理后的** `tool_hook_ctx.tool_input` 构建后续审批事件和执行输入
- 审批事件 `ApprovalRequested` 携带的 tool_call 反映 before_tool 修改后的最终输入

实现要点：在 before_tool 处理完后，令 `let effective_input = tool_hook_ctx.tool_input;`，审批的 `ApprovalRequested` 事件、`ApprovalGranted/Denied` 事件、以及最终工具执行都使用 `effective_input`（而非原始 `tool_call.input`）。

> 这是 003（ApprovalMode）的前置：003 只替换审批判定条件，依赖本 issue 已把 before_tool 移到 approval 之前。

### Reject 与 budget 计数

`before_tool` 返回 `Reject` 时调用 `state.budget.record_tool_call()`，**与 Skip 一致**。理由：被拒绝的调用仍是模型发起的一次工具调用尝试，计入 `max_tool_calls` 预算可防止模型在 guardrail 拒绝后无限重试同一工具耗尽循环。Abort 则不计数（直接终止 run）。

## 验收标准

- [ ] `HookAction::Reject(String)` 变体存在，编译通过
- [ ] `before_tool` 返回 `Reject(reason)` 时，tool 不执行，`{"error": reason}` 作为 tool result 推入 tool_results，run loop 继续（不 abort），且 `budget.record_tool_call()` 被调用
- [ ] `after_tool` / `before_compact` 返回 `Reject` 时，runner 当作 `Skip` 处理并发出 RuntimeWarning event（Reject 仅在 before_tool 有效）
- [ ] `ToolHookContext.tool_output` 字段存在：before_tool 时为 `None`，after_tool 时为实际输出值
- [ ] `after_tool` hook 修改 `ctx.tool_output` 后，tool_results 中对应 content 被替换
- [ ] `ModelHookContext.response` 字段存在：before_model 时为 `None`，after_model 时为 `Some(模型输出 content)`
- [ ] `after_model` hook 修改 `ctx.response` 后，append 到 state.messages 的 assistant content 为修改后的值（回流生效）
- [ ] **执行顺序**：`before_tool` 在 approval check 之前执行
- [ ] **审批针对最终输入**：before_tool 修改 `tool_input` 后，approval 事件和工具执行都使用修改后的输入
- [ ] before_tool 返回 Skip / Reject / Abort 时不进入审批流程
- [ ] 现有使用 `HookAction::Skip` / `HookAction::Continue` / `HookAction::Abort` 的代码不受影响
- [ ] `LoopDetectionHook`（唯一现有 Hook 实现）不需要修改（默认实现覆盖）
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

## 注意事项

- `ToolHookContext` 中 `tool_output` 的修改检测：不要用 `ptr_eq` 或 dirty flag，直接在 `run_after_tool` 返回后读取 `ctx.tool_output`，若为 `Some(v)`，始终用 `v` 替换 tool_result（after_tool 阶段原本就有输出，None 只在 before_tool 时出现）
- `after_tool` 在多处 `ToolOutput` 分支各自调用（Text / Structured / AsyncJob 等），每处都需要做 ctx 填充 + 替换逻辑；用 helper 函数避免重复
- 本 issue 不引入新公共 API 除类型变更，无 binding crate（py/node）改动
