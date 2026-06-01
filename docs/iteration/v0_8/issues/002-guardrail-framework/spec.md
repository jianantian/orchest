# 002 · Guardrail Framework

## 背景

v0.7 Hook 框架提供了原始扩展点，但对"审查并拦截"场景来说接口过于底层：用户需要自行处理 `ModelHookContext` 的消息替换、`ToolHookContext.tool_input` 的修改等细节。Guardrail 是 Hook 的**便利封装层**，为四个常见审查位置提供意图明确的 API。

依赖：001（`HookAction::Reject`、`ToolHookContext.tool_output`、`ModelHookContext.response` 先落地——四层 guardrail 分别依赖这三者中的对应项）。

## 目标

提供四层 Guardrail 便利 API，用户注册 guardrail 后，框架负责将 guardrail 结果适配为 Hook 调用。

## 范围

### Guardrail 模块结构

```
crates/agent-runtime-core/src/guardrail/
├── mod.rs          # 4 个 Guardrail trait + 4 个 Action 类型，声明子模块并 re-export adapter
├── input.rs        # InputGuardrail — before_model hook adapter
├── output.rs       # OutputGuardrail — after_model hook adapter
├── tool_input.rs   # ToolInputGuardrail — before_tool hook adapter
└── tool_output.rs  # ToolOutputGuardrail — after_tool hook adapter
```

### Action 类型（各层独立）

```rust
// guardrail/mod.rs

pub enum InputGuardrailAction {
    Allow,
    Replace(Vec<crate::model::Message>),
    Abort(String),
}

pub enum OutputGuardrailAction {
    Allow,
    Replace(Vec<crate::model::ContentBlock>),  // 替换模型响应 content（写入 ctx.response，由 001 回流到历史）
    Abort(String),
}

pub enum ToolInputGuardrailAction {
    Allow,
    Modify(serde_json::Value),    // 替换 tool_input
    Reject(String),               // 拒绝此次调用，reason 回传给模型
    Abort(String),
}

pub enum ToolOutputGuardrailAction {
    Allow,
    Modify(serde_json::Value),    // 替换 tool_output
    Abort(String),
}
```

### Guardrail Trait

`check` 接受**只读** `&ctx`：guardrail 只负责"审查 + 表达意图"，所有上下文修改由 adapter 根据返回值统一执行。这消除了"直接改 ctx"和"返回 Modify/Replace"两条修改路径并存导致的歧义——guardrail 读 ctx 决策，adapter 写 ctx 落地，职责单一。

```rust
#[async_trait]
pub trait InputGuardrail: Send + Sync {
    async fn check(&self, ctx: &crate::hook::ModelHookContext) -> InputGuardrailAction;
}

#[async_trait]
pub trait OutputGuardrail: Send + Sync {
    async fn check(&self, ctx: &crate::hook::ModelHookContext) -> OutputGuardrailAction;
}

#[async_trait]
pub trait ToolInputGuardrail: Send + Sync {
    async fn check(&self, ctx: &crate::hook::ToolHookContext) -> ToolInputGuardrailAction;
}

#[async_trait]
pub trait ToolOutputGuardrail: Send + Sync {
    async fn check(&self, ctx: &crate::hook::ToolHookContext) -> ToolOutputGuardrailAction;
}
```

### Hook Adapter（每层各一个内部类型）

各层有一个实现了 `Hook` trait 的 adapter struct，将 Guardrail 调用结果翻译为 HookAction / 上下文修改：

```rust
// guardrail/input.rs
pub(crate) struct InputGuardrailHook(pub Arc<dyn InputGuardrail>);

#[async_trait]
impl Hook for InputGuardrailHook {
    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction {
        match self.0.check(ctx).await {
            InputGuardrailAction::Allow => ModelHookAction::Continue,
            InputGuardrailAction::Replace(msgs) => {
                ctx.messages = msgs;           // 直接修改 ctx（&mut 已有）
                ModelHookAction::Continue
            }
            InputGuardrailAction::Abort(r) => ModelHookAction::Abort(r),
        }
    }
}
```

类似地：
- `OutputGuardrailHook` → `after_model`，`check` 读 `ctx.response`（001 已填模型输出），`Replace(blocks)` 时写 `ctx.response = Some(blocks)`（001 负责回流到 state.messages）
- `ToolInputGuardrailHook` → `before_tool`，`Modify(v)` 写 `ctx.tool_input`，`Reject(r)` 返回 `HookAction::Reject(r)`
- `ToolOutputGuardrailHook` → `after_tool`，`check` 读 `ctx.tool_output`，`Modify(v)` 写 `ctx.tool_output`

### AgentConfig 注册 API

```rust
// run/config.rs
impl AgentConfig {
    pub fn with_input_guardrail(mut self, g: Arc<dyn InputGuardrail>) -> Self;
    pub fn with_output_guardrail(mut self, g: Arc<dyn OutputGuardrail>) -> Self;
    pub fn with_tool_input_guardrail(mut self, g: Arc<dyn ToolInputGuardrail>) -> Self;
    pub fn with_tool_output_guardrail(mut self, g: Arc<dyn ToolOutputGuardrail>) -> Self;
}
```

每个方法将对应 adapter 包装为 `Arc<dyn Hook>` 并 push 到 `self.hooks`。Guardrail 与手写 Hook 共用同一个 `hooks` 列表，执行顺序为注册顺序。

**不提供 `AgentConfigBuilder` 方法**（builder 已经有 `with_hook`，guardrail 通过 `with_hook(Arc::new(InputGuardrailHook(g)))` 或链式调用 `config.with_input_guardrail(g)` 添加）。

### Core 不内置任何具体 Guardrail

框架提供 trait + adapter，不提供任何具体实现（极简 Core 原则）。Examples 目录提供示例实现（keyword filter、length limit 等），作为使用参考。

## 验收标准

- [ ] `guardrail/` 模块存在，4 个 trait + 4 个 Action 枚举 + 4 个 adapter 实现完整
- [ ] `InputGuardrail::Replace` 正确替换发给模型的消息列表
- [ ] `OutputGuardrail::Replace` 正确替换模型输出（最后一条 assistant message content）
- [ ] `ToolInputGuardrail::Modify` 正确替换工具调用输入（工具以修改后的 input 执行）
- [ ] `ToolInputGuardrail::Reject(reason)` 工具不执行，reason 作为 tool result 内容回传给模型
- [ ] `ToolOutputGuardrail::Modify` 正确替换工具输出（模型看到修改后的结果）
- [ ] 各层 `Abort` 正确终止 run（与 Hook::Abort 行为一致）
- [ ] `guardrail::check` 签名为只读 `&ctx`，guardrail 不能直接修改 ctx，仅通过返回值表达意图
- [ ] `AgentConfig::with_input_guardrail` 等 4 个注册方法可用
- [ ] Guardrail adapter 实现 `Hook` trait，与手写 Hook 共用 hooks 列表
- [ ] **链式执行**：同层注册多个 guardrail 时按注册顺序执行；前一个返回 Reject/Abort 时，后续同层 guardrail 不再执行（短路，由 hook runner 的现有链式语义保证）
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

## 注意事项

- `OutputGuardrail` 依赖 001 的 `ModelHookContext.response`：`check` 从 `ctx.response`（after_model 时为 `Some`）读模型输出，`Replace(blocks)` 写回 `ctx.response`，由 001 的 after_model 回流逻辑落到 state.messages。adapter 无需自己 append 历史
- `ToolOutputGuardrailHook` 依赖 001 的 `ToolHookContext.tool_output`，`InputGuardrailHook` 用既有的 `ModelHookContext.messages`（before_model 已支持回流）——必须 001 先合入
- 各 adapter 的 panic recovery 由 `hook/runner.rs` 统一 `catch_unwind` 处理，无需每个 adapter 自己处理
