# 002 · Hook Framework — 实施计划

## 前置条件

- `cargo test --workspace` 全绿（跑一次确认基线）

---

## 步骤

### 步骤 1：新建 `src/hook/mod.rs` — Hook trait + 所有 Context 类型

新建目录 `crates/agent-runtime-core/src/hook/`，创建 `mod.rs`：

```rust
use std::sync::Arc;
use async_trait::async_trait;
use crate::run::RunId;

pub enum HookAction {
    Continue,
    Skip,
    Abort(String),
}

pub enum ModelHookAction {
    Continue,
    Abort(String),
}

pub struct RunHookContext {
    pub run_id: RunId,
    pub agent_name: String,   // 取自 config.system_prompt 前 60 字符
    pub step: u32,
}

pub struct ModelHookContext {
    pub run_id: RunId,
    pub messages: Vec<crate::model::Message>,  // 可修改
    pub model_spec: crate::run::config::ModelSpec,
}

pub struct ToolHookContext {
    pub run_id: RunId,
    pub tool_name: String,
    pub tool_input: serde_json::Value,          // 可修改
    pub tool_metadata: crate::tool::ToolMetadata,
}

pub struct HandoffHookContext {
    pub run_id: RunId,
    pub previous_agent: String,
    pub new_agent: String,
    pub handoff_input: serde_json::Value,
}

pub struct CompactHookContext {
    pub run_id: RunId,
    pub messages: Vec<crate::model::Message>,   // 可修改
    pub token_count: u32,
}

#[async_trait]
pub trait Hook: Send + Sync {
    async fn on_run_start(&self, _ctx: &mut RunHookContext) {}
    async fn on_run_end(&self, _ctx: &RunHookContext) {}
    async fn on_run_error(&self, _ctx: &RunHookContext, _error: &str) {}
    async fn before_model(&self, _ctx: &mut ModelHookContext) -> ModelHookAction {
        ModelHookAction::Continue
    }
    async fn after_model(&self, _ctx: &mut ModelHookContext) -> HookAction {
        HookAction::Continue
    }
    async fn before_tool(&self, _ctx: &mut ToolHookContext) -> HookAction {
        HookAction::Continue
    }
    async fn after_tool(&self, _ctx: &mut ToolHookContext, _output: &crate::tool::ToolOutput)
        -> HookAction { HookAction::Continue }
    async fn on_handoff(&self, _ctx: &HandoffHookContext) {}
    async fn before_compact(&self, _ctx: &mut CompactHookContext) -> HookAction {
        HookAction::Continue
    }
}
```

在 `src/lib.rs` 中注册：`pub mod hook;`

---

### 步骤 2：添加 hook runner 辅助函数 `src/hook/runner.rs`

新建 `crates/agent-runtime-core/src/hook/runner.rs`，添加链式调用+panic 捕获：

```rust
use std::sync::Arc;
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;
use tokio::sync::mpsc;
use crate::events::RuntimeEvent;
use super::{Hook, HookAction, ModelHookAction, ModelHookContext, RunHookContext,
            ToolHookContext, CompactHookContext};
use crate::run::RunId;
use crate::run::helpers::emit;

// 每个 hook 调用用 AssertUnwindSafe + catch_unwind 包裹
// panic 转为 HookPanicked 事件，run 继续

pub(crate) async fn run_before_model(
    hooks: &[Arc<dyn Hook>],
    ctx: &mut ModelHookContext,
    run_id: RunId,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> ModelHookAction {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.before_model(ctx))
            .catch_unwind().await;
        match result {
            Ok(ModelHookAction::Abort(reason)) => return ModelHookAction::Abort(reason),
            Ok(ModelHookAction::Continue) => {}
            Err(panic) => {
                let msg = format!("{:?}", panic);
                emit(tx, RuntimeEvent::HookPanicked {
                    hook_name: "before_model".to_string(),
                    message: msg,
                }).await;
            }
        }
    }
    ModelHookAction::Continue
}

pub(crate) async fn run_after_model(
    hooks: &[Arc<dyn Hook>],
    ctx: &mut ModelHookContext,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> HookAction {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.after_model(ctx)).catch_unwind().await;
        match result {
            Ok(HookAction::Abort(reason)) => return HookAction::Abort(reason),
            Ok(HookAction::Skip) | Ok(HookAction::Continue) => {}
            Err(panic) => {
                emit(tx, RuntimeEvent::HookPanicked {
                    hook_name: "after_model".to_string(),
                    message: format!("{:?}", panic),
                }).await;
            }
        }
    }
    HookAction::Continue
}

// 同模式：run_before_tool, run_after_tool, run_before_compact, run_on_run_start,
// run_on_run_end, run_on_run_error（各自对应 Hook trait 方法）
```

在 `hook/mod.rs` 中添加：`pub(crate) mod runner;`

---

### 步骤 3：在 `events.rs` 中添加 `HookPanicked` variant

文件 `crates/agent-runtime-core/src/events.rs`，在 `RunAborted` 之前追加：

```rust
HookPanicked {
    hook_name: String,
    message: String,
},
```

---

### 步骤 4：修改 `run/config.rs` — 添加 hooks 字段和 builder

文件 `crates/agent-runtime-core/src/run/config.rs`：

**4.1** `AgentConfig` struct（L38–46）新增字段（`#[serde(skip)]`，因为 trait object 不可序列化）：

```rust
#[serde(skip)]
pub hooks: Vec<std::sync::Arc<dyn crate::hook::Hook>>,
```

**4.2** 在 `impl AgentConfig` block 新增：

```rust
pub fn with_hook(mut self, hook: std::sync::Arc<dyn crate::hook::Hook>) -> Self {
    self.hooks.push(hook);
    self
}
```

---

### 步骤 5：在 `run/loop_.rs` 中插入 hook 调用点

文件 `crates/agent-runtime-core/src/run/loop_.rs`：

**5.1** `on_run_start`：在 `emit(&tx, RuntimeEvent::RunStarted { run_id }).await;`（L69）之后：

```rust
let mut run_hook_ctx = crate::hook::RunHookContext {
    run_id, agent_name: config.system_prompt[..config.system_prompt.len().min(60)].to_string(), step: 0
};
crate::hook::runner::run_on_run_start(&config.hooks, &mut run_hook_ctx, &tx).await;
```

**5.2** `before_model`：在 `emit(&tx, RuntimeEvent::ModelCallStarted { step }).await;`（L201）之后：

```rust
let mut model_ctx = crate::hook::ModelHookContext {
    run_id, messages: messages.clone(), model_spec: config.model.spec.clone()
};
match crate::hook::runner::run_before_model(&config.hooks, &mut model_ctx, run_id, &tx).await {
    crate::hook::ModelHookAction::Abort(reason) => {
        emit(&tx, RuntimeEvent::RunFailed { error: reason }).await;
        return;
    }
    crate::hook::ModelHookAction::Continue => {}
}
// 同步修改的 messages 应用回去：
messages = model_ctx.messages;
```

**5.3** `after_model`：在 `budget.record_model_call(&response.usage);`（L237）之后：

```rust
let mut model_ctx = crate::hook::ModelHookContext { ... };
if let crate::hook::HookAction::Abort(reason) =
    crate::hook::runner::run_after_model(&config.hooks, &mut model_ctx, &tx).await
{
    emit(&tx, RuntimeEvent::RunFailed { error: reason }).await;
    return;
}
```

**5.4** `before_tool`：在 `ToolCallStarted` emit（L398–406）之后，`tool.execute()`（L424）之前：

```rust
let mut tool_ctx = crate::hook::ToolHookContext {
    run_id, tool_name: tool_call.name.clone(),
    tool_input: tool_call.input.clone(),
    tool_metadata: tool.metadata().clone(),
};
match crate::hook::runner::run_before_tool(&config.hooks, &mut tool_ctx, &tx).await {
    crate::hook::HookAction::Skip => {
        tool_results.push(ContentBlock::ToolResult {
            tool_use_id: tool_call.id.clone(),
            content: json!("tool call skipped by hook"),
        });
        budget.record_tool_call();
        continue;
    }
    crate::hook::HookAction::Abort(reason) => {
        emit(&tx, RuntimeEvent::RunFailed { error: reason }).await;
        return;
    }
    crate::hook::HookAction::Continue => {}
}
let tool_call_input = tool_ctx.tool_input; // 应用 hook 可能修改的 input
let execute_fut = tool.execute(tool_call_input, &ctx);
```

**5.5** `after_tool`：需要在每个 `Ok(ToolOutput::*)` arm 内独立调用，不能统一放在循环底部——因为各分支的异步操作（`poll_async_job` 对 `AsyncJob` 分支可能阻塞数秒）和 `output` 值的生命周期不同。

在 `Ok(ToolOutput::Immediate(value))` arm 末尾（`tool_results.push(...)` 之后）：

```rust
let mut tool_out_ctx = crate::hook::ToolHookContext {
    run_id, tool_name: tool_call.name.clone(),
    tool_input: tool_call.input.clone(),
    tool_metadata: tool.metadata().clone(),
};
if let crate::hook::HookAction::Abort(reason) =
    crate::hook::runner::run_after_tool(
        &config.hooks, &mut tool_out_ctx,
        &crate::tool::ToolOutput::Immediate(value.clone()), &tx
    ).await
{
    emit(&tx, RuntimeEvent::RunFailed { error: reason }).await;
    return;
}
```

对 `Ok(ToolOutput::AsyncJob(handle))` 和 `Ok(ToolOutput::Structured {...})` arm 做同样处理（各自在 `tool_results.push` 之后，`budget.record_tool_call()` 之前插入）。`Err(e)` arm 不调用 `after_tool`（tool 失败时没有 output）。

**5.6** `on_run_end`：在 `RunCompleted` emit（L281/L286）之前：

```rust
crate::hook::runner::run_on_run_end(&config.hooks, &run_hook_ctx, &tx).await;
emit(&tx, RuntimeEvent::RunCompleted { output }).await;
```

**5.7** `on_run_error`：在各 `RunFailed` emit 之前（只在顶层错误路径，不在 tool 失败路径），统一用宏或 helper 封装。

---

### 步骤 6：在 `run/compaction.rs` 中插入 `before_compact` hook

文件 `crates/agent-runtime-core/src/run/compaction.rs`，`maybe_compact_context` 函数签名（L17）新增 `hooks` 参数：

```rust
pub(crate) async fn maybe_compact_context(
    config: &AgentConfig,
    model: &Arc<dyn ModelAdapter>,
    messages: &mut Vec<Message>,
    tx: &mpsc::Sender<RuntimeEvent>,
    last_compaction_step: &mut Option<u32>,
    step: u32,
    usage: &TokenUsage,
    hooks: &[Arc<dyn crate::hook::Hook>],  // 新增
) {
    // ...在实际压缩前：
    let mut compact_ctx = crate::hook::CompactHookContext {
        run_id: ..., messages: messages.clone(), token_count: usage.input_tokens,
    };
    match crate::hook::runner::run_before_compact(hooks, &mut compact_ctx, tx).await {
        crate::hook::HookAction::Skip => return,
        crate::hook::HookAction::Abort(_) => return,
        crate::hook::HookAction::Continue => {}
    }
    *messages = compact_ctx.messages;
    // 继续原有压缩逻辑...
}
```

更新 `loop_.rs` 中调用 `maybe_compact_context` 的位置（L248）传入 `&config.hooks`。

---

### 步骤 7：编写测试

在 `crates/agent-runtime-core/src/run/tests.rs` 末尾新增：

**测试 A**：注册两个 hook，验证按顺序调用，context 数据正确

**测试 B**：`before_model` hook 返回 `Abort` → run 以 `RunFailed` 终止，后续 hook 不执行

**测试 C**：`before_tool` hook 返回 `Skip` → tool 不执行，向模型返回 skip result

**测试 D**：hook panic → 发出 `HookPanicked` 事件，run 继续执行到完成

---

## 验证

```bash
cargo test -p agent-runtime-core
cargo clippy -p agent-runtime-core -- -D warnings

# 确认 HookPanicked 事件存在
grep -rn "HookPanicked" crates/ --include="*.rs"

# 确认 hooks 字段在 AgentConfig
grep -n "hooks" crates/agent-runtime-core/src/run/config.rs

# 确认 Hook trait 定义完整（9 个方法）
grep -n "async fn" crates/agent-runtime-core/src/hook/mod.rs
```

---

## 关键决策

- **创建 `src/hook/` 目录而非单文件 `src/hook.rs`**：007（Loop Detection）需要 `hook/loop_detection.rs`，提前建目录避免后续重组
- **panic 捕获用 `futures_util::FutureExt::catch_unwind` + `AssertUnwindSafe`**：`futures-util = "0.3"` 已在 Cargo.toml（L14），可直接使用；避免引入额外依赖
- **`before_model` hook 修改后的 messages 写回**：`model_ctx.messages` 赋值回 `messages` 局部变量，确保修改生效
- **`before_tool` 修改后的 input 应用**：用 `tool_ctx.tool_input` 覆盖 `tool_call.input`；ToolContext 已有 `tool_call_id`，hook 修改后的 input 直接传给 `tool.execute()`
