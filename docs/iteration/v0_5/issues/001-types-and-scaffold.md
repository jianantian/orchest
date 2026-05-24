# 001 · Crate 骨架与公共类型

## 背景

所有 adapter 都依赖同一套公共类型。先把 crate 骨架和类型定义稳定下来，后续 adapter issue 才能并行推进。

类型系统是从 core 的简化版本（`ModelStreamChunk`、3 字段 `ModelError`、2 字段 `TokenUsage`）扩展为 spec 中的完整归一化类型。这不是简单搬运——几乎每个类型都有新增字段或变体。

## 目标

创建 `agent-runtime-providers` crate 骨架，定义全部公共类型，编译通过，零业务逻辑。

## 验收标准

### Crate 骨架

- [ ] `crates/agent-runtime-providers/Cargo.toml` 就位，依赖 tokio（sync）、serde、serde_json、async-trait、reqwest（json + stream）、futures-util、thiserror、tracing、metrics
- [ ] **零 workspace 内部依赖**——`Cargo.toml` 不含任何 `path = "../..."` 条目
- [ ] 加入根 `Cargo.toml` 的 `[workspace] members`
- [ ] `src/lib.rs` + `src/types.rs` 文件就位
- [ ] `cargo check -p agent-runtime-providers` 通过

### ModelAdapter trait

- [ ] `ModelAdapter` trait 定义 `provider_name()`、`model_name()`、`capabilities()`、`complete()` 四个方法
- [ ] `complete()` 签名：`async fn complete(&self, messages: &[Message], tools: &[ToolDef], options: &RequestOptions, tx: Option<mpsc::Sender<StreamEvent>>) -> Result<ModelResponse, ModelError>`
- [ ] trait bound: `Send + Sync`

### 消息类型

- [ ] `Message { role: Role, content: Vec<ContentBlock> }`
- [ ] `Role` enum：System / User / Assistant / Tool
- [ ] `ContentBlock` enum：`Text(String)` / `Thinking { text, signature, provider_details }` / `ToolUse { id, name, input }` / `ToolResult { tool_use_id, content }`
- [ ] `ContentBlock::Thinking` 三个字段均为 `Option`

### 配置类型

- [ ] `ThinkingLevel` enum：Off / Minimal / Low / Medium(default) / High / XHigh / Max
- [ ] `CachePolicy` enum：None / Auto(default) / Long
- [ ] `CompatibilityPolicy` enum：Coerce(default) / Strict
- [ ] `CapabilitySource` enum：Static(default) / ProviderMetadata / Assumed
- [ ] `RequestOptions` 结构体：thinking / thinking_budget_tokens / include_thinking / compatibility_policy / max_tokens / temperature / top_p / cache_policy
- [ ] `RequestOptions::default()` 符合 spec（thinking=Medium, include_thinking=true, compatibility_policy=Coerce, cache_policy=Auto, 其余 None）

### 能力元数据

- [ ] `ModelCapabilities { streaming, tool_use, parallel_tool_use, reasoning, prompt_cache, max_output_tokens, context_window_size, source }`
- [ ] `ReasoningCapability { supported, efforts, budget_tokens, output_exclusion, replay_metadata_required }`
- [ ] `CacheCapability { supported, explicit_breakpoints, long_ttl }`
- [ ] `OptionAdjustment { option, requested, applied, reason }`——所有字段为 String / Value

### Streaming 事件

- [ ] `StreamEvent` enum：Text / ThinkingStart / Thinking / ThinkingEnd { signature, provider_details } / ToolUseStart { id, name } / ToolUseArgsChunk { id, delta } / ToolUseEnd { id } / Done { usage }
- [ ] 与 core 的 `ModelStreamChunk` 对比：新增 `ThinkingEnd.signature`、`ThinkingEnd.provider_details`、`ToolUseStart`、`ToolUseEnd`；重命名 `ToolCallArgsChunk` → `ToolUseArgsChunk`

### 响应类型

- [ ] `ModelResponse { content, usage, stop_reason, option_adjustments }`
- [ ] `option_adjustments` 使用 `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
- [ ] `TokenUsage { input_tokens, output_tokens, reasoning_tokens, cache_read_tokens, cache_write_tokens, details }`——`details` 使用 `#[serde(default, skip_serializing_if = "HashMap::is_empty")]`
- [ ] `StopReason` enum：EndTurn / ToolUse / MaxTokens / StopSequence / ContentFilter / Refusal / ContextWindowExceeded / Pause / Interrupted / Other(String)

### 错误类型

- [ ] `ModelError { message, code, provider, status, upstream_code, upstream_message, upstream_body }`——实现 `thiserror::Error`
- [ ] `ModelError::internal(message, code)` 便利构造器：只设 message + code，其余 None

### 其他类型

- [ ] `ToolDef { name, description, input_schema }`，`pub type JsonSchema = serde_json::Value`
- [ ] `ModelSpec { provider, model, api_key_env, api_url, max_tokens, context_window_size }`

### 导出

- [ ] `lib.rs` 通过 `pub mod types; pub use types::*;` 导出全部类型
- [ ] 后续 adapter 模块（anthropic / openai / deepseek / openrouter / sse / telemetry）在此 issue 中 **不** 创建

### 测试

- [ ] `ThinkingLevel::default()` == Medium
- [ ] `CachePolicy::default()` == Auto
- [ ] `CompatibilityPolicy::default()` == Coerce
- [ ] `RequestOptions::default()` 各字段符合预期
- [ ] `ModelError::internal()` 构造器正确
- [ ] `ModelError` 实现 `Display`（输出 message 字段）
- [ ] `TokenUsage::default()` 所有数值字段为 0，details 为空
- [ ] `ContentBlock::Thinking` serde 往返
- [ ] `Message` serde 往返
- [ ] `StreamEvent::Done` serde 往返
- [ ] `StreamEvent::ThinkingEnd` 含 signature 的 serde 往返
- [ ] `ModelResponse` 空 `option_adjustments` 时 JSON 不含该字段
- [ ] `ModelResponse` 含 `option_adjustments` 时 JSON 包含该字段
- [ ] `StopReason::Other(String)` serde 保留自定义字符串
- [ ] `ModelCapabilities::default()` 各 bool 为 false
- [ ] `ToolDef` serde 往返
- [ ] `ModelSpec` 可选字段缺失时反序列化正常
- [ ] `cargo test -p agent-runtime-providers` 全绿
- [ ] `cargo clippy -p agent-runtime-providers -- -D warnings` 全绿

## 注意

- 类型定义以 spec 的 "Public types" 章节为 source of truth，但 spec 是设计意图——Rust 编译器说了算。如果某处 derive 不通过，修复后记录差异
- `ToolDef` 从 core 的 `tool` 模块拆出，在 providers 中独立定义。core 后续通过 re-export 使用（issue 007）
- 不要在此 issue 引入任何 adapter 逻辑或 HTTP 调用——这个 issue 纯粹是类型 + 编译
