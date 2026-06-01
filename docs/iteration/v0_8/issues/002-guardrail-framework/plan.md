# 002 · Guardrail Framework — 实施计划

## 前置条件

- 001（Hook Contract Extension）已合入 main

---

## 步骤

### 步骤 1：新建 `src/guardrail/mod.rs` — Action 类型 + trait 声明

```
crates/agent-runtime-core/src/guardrail/mod.rs
```

内容：
- 4 个 Action 枚举（InputGuardrailAction / OutputGuardrailAction / ToolInputGuardrailAction / ToolOutputGuardrailAction）
- 4 个 `#[async_trait]` trait（InputGuardrail / OutputGuardrail / ToolInputGuardrail / ToolOutputGuardrail）
- `pub mod input; pub mod output; pub mod tool_input; pub mod tool_output;`
- Re-export 各 adapter：`pub(crate) use input::InputGuardrailHook; ...`

### 步骤 2：新建各层 adapter 文件

**`guardrail/input.rs`** — `InputGuardrailHook`：
- 实现 `Hook::before_model`
- `Replace(msgs)` → `ctx.messages = msgs`; `Allow`/`Continue`; `Abort` 透传

**`guardrail/output.rs`** — `OutputGuardrailHook`：
- 实现 `Hook::after_model`
- `Replace(v)` → 找 ctx.messages 最后一条 assistant message，替换其 content；若最后一条不是 assistant，`try_send` RuntimeWarning 并 Continue
- 需要 `mpsc::Sender<RuntimeEvent>` 用于警告；通过 `after_model(&self, ctx, tx)` 签名传入（当前签名为 `after_model(&self, ctx: &mut ModelHookContext) -> HookAction`，没有 tx）—— **注意**：当前 `Hook::after_model` 没有 tx 参数，发不了 event。选择：(a) 静默忽略警告，(b) 改 `after_model` 签名。选 (a)，保持签名不变，警告仅记录到 log（tracing::warn!）

**`guardrail/tool_input.rs`** — `ToolInputGuardrailHook`：
- 实现 `Hook::before_tool`
- `Modify(v)` → `ctx.tool_input = v`; `Reject(r)` → `HookAction::Reject(r)`; `Abort` 透传

**`guardrail/tool_output.rs`** — `ToolOutputGuardrailHook`：
- 实现 `Hook::after_tool`
- `Modify(v)` → `ctx.tool_output = Some(v)`（001 已有此字段）; `Abort` 透传

### 步骤 3：`src/lib.rs` — 声明 guardrail 模块并 re-export 公共类型

```rust
pub mod guardrail;
pub use guardrail::{
    InputGuardrail, InputGuardrailAction,
    OutputGuardrail, OutputGuardrailAction,
    ToolInputGuardrail, ToolInputGuardrailAction,
    ToolOutputGuardrail, ToolOutputGuardrailAction,
};
```

### 步骤 4：`run/config.rs` — AgentConfig 注册方法

在 `impl AgentConfig` 中新增 4 个方法，每个将对应 adapter 包装为 `Arc<dyn Hook>` 并 push 到 `self.hooks`：

```rust
pub fn with_input_guardrail(mut self, g: Arc<dyn crate::guardrail::InputGuardrail>) -> Self {
    self.hooks.push(Arc::new(crate::guardrail::InputGuardrailHook(g)));
    self
}
// 类似地 with_output_guardrail / with_tool_input_guardrail / with_tool_output_guardrail
```

### 步骤 5：单元测试

新建 `crates/agent-runtime-core/src/guardrail/tests.rs`（或在各文件内 `#[cfg(test)]`）：

1. `input_guardrail_replace_changes_messages`：FakeModelAdapter 记录收到的 messages，注册 Replace guardrail，验证 model 收到替换后的 messages
2. `tool_input_guardrail_reject_returns_reason_to_model`：注册 Reject guardrail，验证 tool 不执行，tool_result content 含 reason 字符串
3. `tool_input_guardrail_modify_changes_input`：注册 Modify guardrail，FakeTool 记录收到的 input，验证 input 被替换
4. `tool_output_guardrail_modify_changes_result`：注册 Modify guardrail，验证 tool_result content 被替换
5. `guardrail_abort_terminates_run`：任意层返回 Abort，验证 run 以 RunFailed 结束

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
