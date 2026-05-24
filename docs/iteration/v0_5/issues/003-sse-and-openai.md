# 003 · SSE 共享解析器与 OpenAI Adapter

## 背景

OpenAI、DeepSeek、OpenRouter 都使用相同的 SSE 行格式（`data: {...}` lines + `data: [DONE]`），区别在于 thinking/reasoning 字段名和 provider-specific 细节。抽取共享解析器避免三份重复代码。

OpenAI adapter 已有 543 行实现在 `core/src/model/openai.rs`，需要迁移并扩展。

## 目标

实现 `sse.rs` 共享 SSE 解析器，迁移并扩展 OpenAI adapter。

## 验收标准

### 共享 SSE 解析器（`sse.rs`）

- [ ] `crates/agent-runtime-providers/src/sse.rs` 文件就位
- [ ] `lib.rs` 中添加 `pub(crate) mod sse;`（不对外公开）
- [ ] 核心函数签名：

```rust
pub(crate) async fn parse_openai_sse_stream(
    stream: impl Stream<Item = Result<Bytes, reqwest::Error>>,
    tx: Option<&mpsc::Sender<StreamEvent>>,
    reasoning_field: Option<&str>,
    reasoning_details_field: Option<&str>,
) -> Result<(Vec<ContentBlock>, TokenUsage, StopReason), ModelError>
```

- [ ] 处理 TCP chunk 边界的行缓冲（SSE 事件可能跨 chunk）
- [ ] 从 `choices[0].delta.content` 提取 text → emit `StreamEvent::Text`
- [ ] 从 `choices[0].delta.tool_calls` 累积 tool calls → emit `ToolUseStart` / `ToolUseArgsChunk` / `ToolUseEnd`
- [ ] 当 `reasoning_field` 非 None 时，从 `choices[0].delta.{field}` 提取 thinking → emit `ThinkingStart` / `Thinking` / `ThinkingEnd`
- [ ] 当 `reasoning_details_field` 非 None 时，保留 provider-native reasoning details → `ContentBlock::Thinking.provider_details`
- [ ] 从最后一个 chunk 的 `usage` 提取 token usage
- [ ] Stop reason 映射：`stop` → EndTurn, `tool_calls` → ToolUse, `length` → MaxTokens, `content_filter` → ContentFilter, 其他 → `Other(raw)`
- [ ] `data: [DONE]` 和空行跳过
- [ ] Malformed JSON → `ModelError { code: "invalid_json" }` 含原始 data 行
- [ ] 流中断（连接断开且未收到 `data: [DONE]`）→ 返回 `ModelError { code: "stream_interrupted" }`
- [ ] 成功完成但 usage 为空 → `TokenUsage::default()` + `OptionAdjustment { option: "usage", reason: "usage_not_reported" }`
- [ ] `tx` 为 None 时跳过所有 event emit

### OpenAI Adapter

- [ ] `crates/agent-runtime-providers/src/openai.rs` 文件就位
- [ ] `lib.rs` 中 re-export `OpenAiAdapter` / `OpenAiConfig`

**OpenAiConfig：**

- [ ] `OpenAiConfig { model, max_tokens, api_key, api_url }`
- [ ] API key fallback：config.api_key → `OPENAI_API_KEY`
- [ ] API URL fallback：config.api_url → `OPENAI_API_URL` → `OPENAI_BASE_URL` → 默认 `https://api.openai.com/v1/chat/completions`
- [ ] URL 自动归一化到 `/v1/chat/completions`
- [ ] Model string 自动 strip `openai/` prefix

**ModelAdapter trait 方法：**

- [ ] `provider_name()` → `"openai"`
- [ ] `model_name()` → stripped model string
- [ ] `capabilities()` → static table for known models，`Assumed` for unknown
- [ ] `complete()` 使用 `sse.rs` 的 `parse_openai_sse_stream`，reasoning_field = None，reasoning_details_field = None

**请求构建：**

- [ ] System role → `role: "system"` 消息（或 `"developer"` 当 model 偏好时）
- [ ] Tool role → `role: "tool"` + `tool_call_id`
- [ ] Assistant ToolUse → `tool_calls` 数组中的 `{ id, type: "function", function: { name, arguments } }`
- [ ] `ContentBlock::Thinking` 在序列化时跳过（OpenAI 不需要 replay）
- [ ] `RequestOptions::max_tokens` 有值时 override config default
- [ ] `stream: true` + `stream_options: { include_usage: true }`

**ThinkingLevel 映射：**

- [ ] ThinkingLevel → `reasoning_effort` 参数
- [ ] Off → `"none"` when supported, omit otherwise
- [ ] Minimal → `"minimal"` when supported, else `"low"`
- [ ] Low → `"low"`, Medium → `"medium"`, High → `"high"`
- [ ] XHigh → `"xhigh"` when supported, else `"high"`
- [ ] Max → `"max"` when supported, else `"xhigh"` or `"high"`
- [ ] `thinking_budget_tokens` 忽略——Coerce 记录 OptionAdjustment，Strict 返回 error

**CachePolicy 映射：**

- [ ] `Auto` → provider default（不发额外参数）
- [ ] `Long` → `prompt_cache_retention: "24h"` when model supports，否则 Coerce 记录 adjustment
- [ ] Cache tokens：`usage.prompt_tokens_details.cached_tokens` → `TokenUsage::cache_read_tokens`
- [ ] `cache_write_tokens` 始终为 0
- [ ] Reasoning tokens：`usage.completion_tokens_details.reasoning_tokens` → `TokenUsage::reasoning_tokens`

**Error 保留：**

- [ ] HTTP 非 2xx → `ModelError { provider: "openai", status, upstream_body }`
- [ ] 解析 upstream JSON error 结构（`error.type` / `error.message`）

### 测试

**SSE 解析器测试：**

- [ ] `parse_buffered_chunks`：验证跨 TCP chunk 边界的正确缓冲
- [ ] `parse_with_reasoning_field`：reasoning_field 传入时正确提取 thinking events
- [ ] `parse_without_reasoning_field`：reasoning_field = None 时不 emit thinking
- [ ] `parse_tool_calls`：多个 tool call 正确累积 + ToolUseStart/ArgsChunk/ToolUseEnd 事件
- [ ] `parse_done_signal`：`data: [DONE]` 被正确跳过
- [ ] `parse_malformed_json`：返回 ModelError
- [ ] `parse_stream_interrupted`：流中断 → ModelError { code: "stream_interrupted" }
- [ ] `parse_missing_usage_reports_adjustment`：成功完成无 usage → adjustment

**OpenAI adapter 测试：**

- [ ] `strips_prefix`：从现有 core 测试迁移
- [ ] `stream_text_and_tool_calls`：从现有 core 测试迁移，扩展验证 ToolUseStart/End
- [ ] `build_request_body`：从现有 core 测试迁移
- [ ] `stream_rejects_invalid_tool_args`：从现有 core 测试迁移
- [ ] `thinking_level_maps_to_reasoning_effort`：ThinkingLevel → reasoning_effort
- [ ] `thinking_off_maps_to_none_when_supported`：Off → none for supported models
- [ ] `cache_policy_long_maps_to_24h_when_supported`：Long → prompt_cache_retention=24h
- [ ] `cache_tokens_reported`：cached_tokens → cache_read_tokens
- [ ] `temperature_forwarded`：temperature → API body
- [ ] `max_tokens_override`：RequestOptions::max_tokens override config default
- [ ] `reasoning_tokens_reported`：completion_tokens_details.reasoning_tokens → TokenUsage
- [ ] `tx_none_skips_events`：complete(tx=None) 返回正确 ModelResponse

## 依赖

- Issue 001：公共类型
