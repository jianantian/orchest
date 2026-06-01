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
- `check` 传入只读 ctx；guardrail 从 `ctx.response`（001 已填 `Some(模型输出)`）读取输出做决策
- `Replace(blocks)` → `ctx.response = Some(blocks)`（001 的 after_model 回流逻辑负责落到 state.messages）；`Allow` → Continue；`Abort` 透传
- 不需要 tx：无 message-ordering 兜底逻辑（response 一定存在），实现简洁

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
6. `chained_tool_input_guardrails_short_circuit`：注册两个 ToolInputGuardrail，第一个 Reject；验证第二个的 `check` 未被调用（用 AtomicBool 标记），且 tool_result 是第一个的 reason

注：步骤 2-4 各 guardrail 的 `check` 签名是只读 `&ctx`（步骤 2 的 adapter 在 Hook impl 内有 `&mut`，但传给 `check` 时自动 reborrow 成 `&`）。测试中的 fake guardrail 实现 `check(&self, ctx: &XxxHookContext)`。

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
