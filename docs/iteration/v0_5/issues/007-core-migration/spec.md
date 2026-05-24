# 007 · Core 迁移与 workspace 验证

## 背景

providers crate 完成后，core 需要从"自己定义类型 + 自己实现 adapter"切换到"从 providers re-export 类型 + 依赖 providers 的 adapter"。这是改动面最广的 issue，但大部分是 import path 替换。

关键约束：**不改变 core 的公共 API surface**——SDK 代码通过 core re-export 使用类型，import path 只从 `core::model::anthropic::AnthropicAdapter` 缩短为 `core::model::AnthropicAdapter`，不需要直接依赖 providers crate。

## 目标

完成 core 到 providers 的迁移，删除 core 中的旧 model adapter 实现，workspace 全绿。

## 验收标准

### Core Cargo.toml

- [ ] 添加 `agent-runtime-providers = { path = "../agent-runtime-providers" }` 依赖
- [ ] 保留现有的 tracing / metrics 依赖（如果 core 还没有，本 issue 添加）

### model/mod.rs 重写

- [ ] 替换现有内容为 re-exports：

```rust
pub use agent_runtime_providers::{
    ModelAdapter, Message, Role, ContentBlock,
    ToolDef, JsonSchema, ModelResponse, ModelError,
    StreamEvent, RequestOptions,
    ThinkingLevel, CachePolicy, CompatibilityPolicy,
    CapabilitySource, ModelCapabilities, ReasoningCapability, CacheCapability,
    OptionAdjustment,
    TokenUsage, StopReason, ModelSpec,
    AnthropicAdapter, AnthropicConfig,
    OpenAiAdapter, OpenAiConfig,
    DeepSeekAdapter, DeepSeekConfig,
    OpenRouterAdapter, OpenRouterConfig,
    create_adapter, stream_chat, chat,
};

/// Backward-compatible alias.
pub type ModelStreamChunk = StreamEvent;
```

- [ ] 删除 `pub mod anthropic;` 和 `pub mod openai;`

### 删除旧文件

- [ ] 删除 `crates/agent-runtime-core/src/model/anthropic.rs`
- [ ] 删除 `crates/agent-runtime-core/src/model/openai.rs`

### Import path 更新

- [ ] `src/run.rs`：更新 `use crate::model::*` 引用
  - `ModelStreamChunk` → 可保持使用 alias，或改为 `StreamEvent`
  - `StopReason` 新增的变体不影响现有 match——现有 match 只用 EndTurn / ToolUse / MaxTokens，其他 fallback 到默认分支
  - 确认 `run.rs` 中的 `model.stream()` 调用适配新的 `model.complete()` 签名（`Option<tx>` + `RequestOptions` 参数）
- [ ] `src/events.rs`：`ModelStreamChunk` 类型可通过 alias 保持，或改为 `StreamEvent`
  - `RuntimeEvent::ModelStreamChunk { delta }` 变体名可暂时保留（wire format 不变），内部类型用 alias
  - `RuntimeEvent::ModelCallCompleted { tokens: TokenUsage }` 需要适配新的 TokenUsage（多了字段，但向后兼容——新字段有 default）
- [ ] `src/tool/mod.rs`：`ToolDef` 改为从 `crate::model::ToolDef` 引入（通过 re-export）
  - 如果 core 的 `tool` 模块有自己的 `ToolDef` 定义，删除并改为 use
- [ ] `src/budget.rs`：如果引用 `TokenUsage`，更新 import path

### Run loop 适配

- [ ] `model.stream(messages, tools, tx)` 改为 `model.complete(messages, tools, &options, Some(tx))`
- [ ] 构造 `RequestOptions::default()`（或从 AgentConfig 派生）作为 `complete()` 参数
- [ ] `model.call(messages, tools)` 改为 `model.complete(messages, tools, &options, None)` 或使用 `chat()` helper

### Backward compatibility

- [ ] `pub type ModelStreamChunk = StreamEvent;` 确保 core 外部引用 `ModelStreamChunk` 的代码不 break
- [ ] `ToolDef` 通过 re-export 可在 `crate::model::ToolDef` 和 `crate::tool::ToolDef` 两个路径使用——如果 tool mod 之前有自己的 ToolDef，改为 re-export providers 的版本
- [ ] core 内部的 `MockModelProvider`（如果有）需要实现新的 `ModelAdapter` trait 签名（`complete` + `provider_name` + `model_name` + `capabilities`）

### SDK 更新

- [ ] `agent-runtime-py` 和 `agent-runtime-node`：import path 从 `core::model::anthropic::AnthropicAdapter` 变为 `core::model::AnthropicAdapter`（子模块删除了）
- [ ] 如果 SDK 直接 import `ModelStreamChunk`，通过 alias 兼容；不需要改动

### 测试

- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] 现有 core 测试（`e2e_validation.rs`、`v03_runtime.rs` 等）不改动仍能通过
- [ ] 现有 core 的 model-specific 测试（SSE parsing / URL normalization）已在 providers crate 覆盖，core 中的对应测试可删除
- [ ] `runtime_contract::core_never_branches_on_provider`：验证 core 测试只使用 ModelAdapter / Message / ContentBlock / StreamEvent，不引用 provider-specific 类型

## 注意

- 这个 issue 的改动面最广但逻辑最简单——主要是 import path 替换和删除。如果编译报错，大部分是 path 问题
- `run.rs` 中的 `model.stream()` → `model.complete()` 签名变化是唯一需要仔细处理的逻辑变化——需要构造 `RequestOptions` 并传入 `Some(tx)`
- 不要在这个 issue 中引入新 feature——如果发现 core 需要用 providers 的新能力（如 ThinkingLevel），留到后续 issue
- 如果 workspace 中有其他依赖 `core::model::anthropic` 子模块的代码，需要逐一修复 import path

## 依赖

- Issue 006：factory 和 telemetry（确保 providers crate 功能完整）
