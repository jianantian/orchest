# 006 · Factory 函数与 Telemetry

## 背景

四个 adapter 就位后，需要提供统一的构造入口和便利函数。`create_adapter` 工厂让调用方不需要 import 各 adapter 的 config 类型。`stream_chat()` / `chat()` 封装常见用法。Telemetry helpers 确保所有 adapter 的 tracing span 和 metric name 一致。

## 目标

在 `lib.rs` 中实现 `create_adapter` 工厂函数和 `stream_chat()` / `chat()` 便利函数，在 `telemetry.rs` 中实现 span helpers。

## 验收标准

### Factory 函数

- [ ] `create_adapter(model: &str, api_key: Option<String>) -> Result<Box<dyn ModelAdapter>, ModelError>`
- [ ] 路由规则：按 model string 的第一个 `/` 分段
  - `"anthropic/..."` → `AnthropicAdapter`
  - `"openai/..."` → `OpenAiAdapter`
  - `"deepseek/..."` → `DeepSeekAdapter`
  - `"openrouter/..."` → `OpenRouterAdapter`（后面所有内容作为 model name）
- [ ] 无 `/` 的 model string → `ModelError { code: "invalid_model" }`
- [ ] 未知 provider → `ModelError { code: "unknown_provider" }`，message 列出支持的 provider
- [ ] 各 adapter 的 `max_tokens` 默认 4096

### 便利函数

- [ ] `stream_chat()` 签名：

```rust
pub fn stream_chat(
    adapter: &dyn ModelAdapter,
    messages: &[Message],
    tools: &[ToolDef],
    options: &RequestOptions,
) -> (impl Future<Output = Result<ModelResponse, ModelError>> + '_, mpsc::Receiver<StreamEvent>)
```

- [ ] 内部创建 bounded channel（capacity 64），调用 `adapter.complete(tx: Some)`
- [ ] 返回 (future, receiver) pair

- [ ] `chat()` 签名：

```rust
pub async fn chat(
    adapter: &dyn ModelAdapter,
    messages: &[Message],
    tools: &[ToolDef],
    options: &RequestOptions,
) -> Result<ModelResponse, ModelError>
```

- [ ] 内部调用 `adapter.complete(tx: None)`

### Telemetry（`telemetry.rs`）

- [ ] `crates/agent-runtime-providers/src/telemetry.rs` 文件就位
- [ ] `lib.rs` 中添加 `pub mod telemetry;`
- [ ] Provider-side span helpers：
  - `model_complete_span(provider, model, streaming)` → tracing span name `model.complete`
  - span 包含 `provider`、`model`、`streaming` 属性，不包含 prompt 内容
- [ ] Provider-side metric name constants：
  - `METRIC_REQUEST_DURATION` → `model.request.duration`
  - `METRIC_FIRST_TOKEN_LATENCY` → `model.stream.first_token_latency`
  - `METRIC_STREAM_DURATION` → `model.stream.duration`
  - `METRIC_TOKENS_INPUT` → `model.tokens.input`
  - `METRIC_TOKENS_OUTPUT` → `model.tokens.output`
  - `METRIC_USAGE_MISSING` → `model.usage.missing`
- [ ] Metric labels：`provider` / `model_family` / `status`（低基数，不含 run_id 或 raw error text）
- [ ] Helpers 不安装 subscriber 或 exporter——纯定义，调用方控制 backend

### 导出

- [ ] `lib.rs` re-export 完整列表：所有 types + 全部 adapter + config + factory + helpers
- [ ] 对外公开：`create_adapter`、`stream_chat`、`chat`、各 Adapter/Config 类型、所有 types
- [ ] `pub(crate)` 不公开：`sse` 模块

### 测试

- [ ] `factory::routes_by_provider`：anthropic / openai / deepseek / openrouter 各创建成功（需要设置 env var 或传 api_key）
- [ ] `factory::rejects_unknown_provider`：`"gemini/..."` → error
- [ ] `factory::rejects_no_slash`：`"claude-sonnet-4"` → error
- [ ] `factory::openrouter_preserves_full_model`：`"openrouter/anthropic/claude-sonnet-4"` → model_name() == `"anthropic/claude-sonnet-4"`
- [ ] `helpers::stream_chat_returns_pair`：stream_chat 返回 (future, receiver)
- [ ] `helpers::chat_returns_response`：chat 返回 ModelResponse
- [ ] `telemetry::model_complete_span_has_canonical_fields`：span 包含 provider/model/streaming 属性
- [ ] `telemetry::provider_request_metrics_are_low_cardinality`：metric labels 不含高基数字段

## 依赖

- Issue 002：Anthropic adapter
- Issue 003：OpenAI adapter + sse.rs
- Issue 004：DeepSeek adapter
- Issue 005：OpenRouter adapter
