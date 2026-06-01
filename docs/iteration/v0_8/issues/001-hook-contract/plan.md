# 001 · Hook Contract Extension — 实施计划

## 前置条件

- v0.8 PRD 审阅完成（当前分支）
- `cargo test --workspace` 全绿（基线确认）

---

## 步骤

### 步骤 1：`hook/mod.rs` — 扩展 HookAction 和 ToolHookContext

**文件**：`crates/agent-runtime-core/src/hook/mod.rs`

1. `HookAction` 枚举新增 `Reject(String)` 变体，放在 `Skip` 之后
2. `ToolHookContext` 新增 `pub tool_output: Option<serde_json::Value>` 字段，默认 `None`

### 步骤 2：`hook/runner.rs` — 更新 run_before_tool 和 run_after_tool

**文件**：`crates/agent-runtime-core/src/hook/runner.rs`

**run_before_tool**：在 match arm 中新增：
```rust
Ok(HookAction::Reject(reason)) => return HookAction::Reject(reason),
```

**run_after_tool**：
- 函数签名不变（`ctx` 已是 `&mut ToolHookContext`）
- 新增 arm：`after_tool` 中返回 `Reject` 时，发出 RuntimeWarning event 并当作 Skip 处理：
  ```rust
  Ok(HookAction::Reject(_)) => {
      // Reject 在 after_tool 无效；记录警告并继续
      let _ = tx.try_send(RuntimeEvent::RuntimeWarning {
          message: "HookAction::Reject returned from after_tool; treated as Skip".into(),
      });
  }
  ```

### 步骤 3：`run/actor.rs` — 重排 before_tool / approval 顺序 + 处理 Reject + tool_output 替换

**文件**：`crates/agent-runtime-core/src/run/actor.rs`

**步骤 3a：重排执行顺序**

当前顺序（行号近似）：
```
:626  approval check (if tool.metadata().requires_approval { ... })
:680  budget(max_tool_calls) check
:717  before_tool hooks（match run_before_tool { ... }）
:740+ execute tool
```

重排为：
```
1. before_tool hooks（match run_before_tool）→ 得到 effective_input = tool_hook_ctx.tool_input
2. approval check（使用 effective_input 构建 ApprovalRequested / Granted / Denied 事件）
3. budget(max_tool_calls) check
4. execute tool（使用 effective_input）
```

把整个 approval block（`:626`–`:673` 附近）移到 before_tool match（`:717`+）之后。注意：
- before_tool 的 `Continue` arm 之后，提取 `let effective_input = tool_hook_ctx.tool_input;`
- approval block 中所有引用 `tool_call.input` / `tool_call.clone()` 处，改用携带 `effective_input` 的 tool_call（clone tool_call 后替换其 input 字段，或构造事件时直接用 effective_input）
- 工具执行处原本就用 before_tool 后的 input，重排后保持一致

**步骤 3b：before_tool 的 Reject 处理**（在 before_tool match 中新增 arm）：

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

**after_tool 的 tool_output 填充 + 替换**：

各 ToolOutput 分支在调用 `run_after_tool` 前，将输出值填入 ctx：

```rust
let output_value = /* 当前分支的 model_output 或 text content */;
let mut tool_out_ctx = crate::hook::ToolHookContext {
    run_id,
    tool_name: tool_call.name.clone(),
    tool_input: tool_input.clone(),
    tool_metadata: tool_meta.clone(),
    tool_output: Some(output_value.clone()),   // ← 新增
};
```

`run_after_tool` 返回后，若 `tool_out_ctx.tool_output` != `Some(output_value)`（用 hook 修改了），替换 tool_results 最后一项 content：

```rust
if let Some(modified) = tool_out_ctx.tool_output {
    if modified != output_value {
        if let Some(last) = tool_results.last_mut() {
            if let ContentBlock::ToolResult { content, .. } = last {
                *content = modified;
            }
        }
    }
}
```

提取 helper `invoke_after_tool_hooks(...)` 消除各分支重复，入参：`hooks, run_id, tool_call, tool_input, tool_meta, output_value, event_tx`，返回 `(HookAction, Option<Value>)`（action + 最终 output）。

### 步骤 4：单元测试

在 `crates/agent-runtime-core/src/run/tests.rs` 或 `hook/mod.rs` 的 `#[cfg(test)]` 中新增：

1. `before_tool_reject_skips_execution_and_injects_reason`：FakeModelAdapter 触发 tool call，注册返回 `Reject("blocked")` 的 hook，验证 tool_results 含 `{"error": "blocked"}`，run 继续完成（不 abort）
2. `after_tool_hook_modifies_output`：注册 after_tool hook 修改 `ctx.tool_output`，验证最终 tool_result content 被替换
3. `after_tool_reject_treated_as_skip_with_warning`：注册 after_tool 返回 `Reject` 的 hook，验证发出 RuntimeWarning，run 继续
4. `before_tool_runs_before_approval`：注册一个 before_tool hook 修改 `ctx.tool_input`，且工具 `requires_approval: true`；验证 `ApprovalRequested` 事件携带的是修改后的 input（证明顺序正确）
5. `before_tool_reject_skips_approval`：工具 `requires_approval: true` + before_tool 返回 Reject；验证**没有** ApprovalRequested 事件发出，直接得到 reject 结果

### 步骤 5：确认基线不退化

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
