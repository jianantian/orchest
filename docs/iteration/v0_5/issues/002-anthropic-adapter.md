# 002 · Anthropic Adapter

## 背景

Anthropic 是 Orchest 的主要 provider，已有 570 行实现在 `core/src/model/anthropic.rs`。但现有实现缺少 thinking level 映射、cache control、capabilities 报告、extended error 保留等。本 issue 迁移并扩展该 adapter。

Anthropic 使用自己的 SSE 协议（`event: type` + `data: json`），与 OpenAI-compat 格式不同，不共享 `sse.rs` 解析器。

## 目标

在 providers crate 中实现完整的 `AnthropicAdapter`，支持 adaptive / enabled thinking 双模式、cache control、capabilities 报告、extended error 保留。

## 验收标准

### 结构

- [ ] `crates/agent-runtime-providers/src/anthropic.rs` 文件就位
- [ ] `lib.rs` 中添加 `pub mod anthropic;` 并 re-export `AnthropicAdapter` / `AnthropicConfig`
- [ ] `AnthropicAdapter` 实现 `ModelAdapter` trait 的全部四个方法

### AnthropicConfig

- [ ] `AnthropicConfig { model, max_tokens, api_key, api_url }` 结构体公开
- [ ] `AnthropicAdapter::from_config(config) -> Result<Self, ModelError>`
- [ ] API key fallback 顺序：config.api_key → `ANTHROPIC_API_KEY` → `ANTHROPIC_AUTH_TOKEN`（不再 fallback 到 `OPENROUTER_API_KEY`，那是 OpenRouter adapter 的事）
- [ ] API URL fallback：config.api_url → `ANTHROPIC_API_URL` → `ANTHROPIC_BASE_URL` → 默认 `https://api.anthropic.com/v1/messages`
- [ ] URL 自动归一化到 `/v1/messages`
- [ ] 空 URL 返回 `ModelError { code: "invalid_api_url" }`

### ModelAdapter trait 方法

- [ ] `provider_name()` → `"anthropic"`
- [ ] `model_name()` → 构造时传入的 model string
- [ ] `capabilities()` → 返回 `ModelCapabilities`，对已知模型（claude-sonnet-4 / claude-opus-4 等）使用 static table，未知模型使用 `CapabilitySource::Assumed` conservative fallback
- [ ] `complete()` 签名匹配 spec——`Option<mpsc::Sender<StreamEvent>>`

### 请求构建

- [ ] System role → Anthropic top-level `system` 参数（不是 message role）
- [ ] `ContentBlock::Thinking` 在 assistant 消息中序列化为 Anthropic `thinking` content block（含 `signature` 字段）
- [ ] `ContentBlock::ToolUse` → `tool_use` content block
- [ ] `ContentBlock::ToolResult` → user turn 中的 `tool_result` content block
- [ ] `ToolDef` → `tools` 数组中的 `{ name, description, input_schema }` 对象

### ThinkingLevel 映射

- [ ] 支持 adaptive 和 enabled 双模式，根据 model capabilities 选择
- [ ] Adaptive mode（新模型）：ThinkingLevel → `output_config.effort`，映射表见 spec（Off→disabled, Minimal/Low→low, Medium→medium, High→high, XHigh→xhigh, Max→max）
- [ ] Enabled mode（老模型 fallback）：ThinkingLevel → `thinking.budget_tokens`，映射表见 spec（Minimal→1024, Low→4096, Medium→10240, High→32768, XHigh→65536, Max→max allowed）
- [ ] `RequestOptions::thinking_budget_tokens` 仅在 enabled mode 生效——override level 默认 budget
- [ ] Adaptive mode 下如果传了 `thinking_budget_tokens`，Coerce 记录 `OptionAdjustment { option: "thinking_budget_tokens", reason: "unsupported_in_adaptive_thinking" }`；Strict 返回错误

### include_thinking 映射

- [ ] `include_thinking: true` → `thinking.display: "summarized"`
- [ ] `include_thinking: false` → `thinking.display: "omitted"`

### CachePolicy 映射

- [ ] `CachePolicy::Auto` → top-level `cache_control: { type: "ephemeral" }`
- [ ] `CachePolicy::Long` → top-level `cache_control: { type: "ephemeral", ttl: "1h" }`
- [ ] `CachePolicy::None` → 不发 cache_control
- [ ] 如需 beta header，自动添加 `anthropic-beta` 值

### max_tokens 覆盖

- [ ] `RequestOptions::max_tokens` 有值时 override adapter 构造时的 `config.max_tokens`
- [ ] `RequestOptions::max_tokens` 为 None 时使用 adapter 的配置默认值

### temperature / top_p

- [ ] `RequestOptions::temperature` 有值时加入请求 body
- [ ] `RequestOptions::top_p` 有值时加入请求 body

### SSE 解析（Anthropic 自有协议）

- [ ] 解析 `message_start`：提取 `input_tokens`
- [ ] 解析 `content_block_start` type=thinking → emit `StreamEvent::ThinkingStart`
- [ ] 解析 `content_block_delta` type=thinking_delta → emit `StreamEvent::Thinking { delta }`
- [ ] 解析 `content_block_stop` for thinking block → emit `StreamEvent::ThinkingEnd { signature, provider_details: None }`，从 thinking block 的 `signature` 字段提取签名
- [ ] 解析 `content_block_start` type=text → 不 emit 事件，开始累积
- [ ] 解析 `content_block_delta` type=text_delta → emit `StreamEvent::Text { delta }`
- [ ] 解析 `content_block_start` type=tool_use → emit `StreamEvent::ToolUseStart { id, name }`
- [ ] 解析 `content_block_delta` type=input_json_delta → emit `StreamEvent::ToolUseArgsChunk { id, delta }`
- [ ] 解析 `content_block_stop` for tool_use → emit `StreamEvent::ToolUseEnd { id }`
- [ ] 解析 `message_delta`：提取 stop_reason 和 output_tokens
- [ ] 流结束时 emit `StreamEvent::Done { usage }`
- [ ] SSE 流中断（连接断开且未收到 `message_stop`）→ 返回 `ModelError { code: "stream_interrupted" }`，不 emit 含默认 usage 的 Done
- [ ] Provider 成功完成但未报告 usage → `TokenUsage::default()` + `OptionAdjustment { option: "usage", reason: "usage_not_reported" }`
- [ ] `StreamEvent::Done { usage }` 和 `ModelResponse.usage` 必须一致
- [ ] `tx` 为 None 时跳过所有 event emit，仍然解析并构建 `ModelResponse`

### ContentBlock::Thinking 构建

- [ ] Thinking block 解析后构建 `ContentBlock::Thinking { text, signature, provider_details: None }` 加入 `ModelResponse.content`
- [ ] Signature 必须保留——从 Anthropic 的 `content_block_stop` 或 `content_block_start` 中提取

### Stop reason 映射

- [ ] `end_turn` → EndTurn
- [ ] `tool_use` → ToolUse
- [ ] `max_tokens` → MaxTokens
- [ ] `stop_sequence` → StopSequence
- [ ] `pause_turn` / `compaction` → Pause
- [ ] `refusal` → Refusal
- [ ] `model_context_window_exceeded` → ContextWindowExceeded
- [ ] 未知 reason → `Other(raw_reason)`

### Error 保留

- [ ] HTTP 非 2xx 时，`ModelError` 保留 `status`（HTTP status code）、`upstream_body`（原始响应 body）
- [ ] `provider` 设为 `"anthropic"`
- [ ] 如果 upstream body 是 JSON 且含 `error.type` / `error.message`，提取到 `upstream_code` / `upstream_message`
- [ ] SSE 中的 malformed JSON → `ModelError { code: "invalid_json" }`，`upstream_body` 含原始 data 行

### TokenUsage 映射

- [ ] `input_tokens` / `output_tokens` 从 `message_start` + `message_delta` 合并
- [ ] `cache_read_input_tokens` → `TokenUsage::cache_read_tokens`
- [ ] `cache_creation_input_tokens` → `TokenUsage::cache_write_tokens`

### 测试

- [ ] `uses_default_api_url`：从现有 core 测试迁移
- [ ] `uses_custom_api_url`：从现有 core 测试迁移
- [ ] `appends_messages_endpoint`：从现有 core 测试迁移
- [ ] `rejects_empty_api_url`：从现有 core 测试迁移
- [ ] `stream_thinking_boundaries`：从现有 core 测试迁移，扩展验证 ThinkingEnd 含 signature
- [ ] `stream_rejects_malformed_sse`：从现有 core 测试迁移，扩展验证 upstream_body
- [ ] `stream_thinking_to_content_block`：thinking block → ContentBlock::Thinking，验证 text + signature
- [ ] `thinking_level_maps_to_budget`：enabled mode 下 ThinkingLevel → budget_tokens
- [ ] `thinking_budget_override`：thinking_budget_tokens override level default
- [ ] `thinking_budget_ignored_in_adaptive_reports_adjustment`：adaptive mode + budget_tokens → OptionAdjustment
- [ ] `adaptive_uses_output_config_effort`：adaptive mode ThinkingLevel → output_config.effort
- [ ] `include_thinking_false_maps_to_omitted`：include_thinking=false → display: "omitted"
- [ ] `cache_policy_auto_adds_top_level_cache_control`：Auto → { type: "ephemeral" }
- [ ] `cache_policy_long_sets_1h_ttl`：Long → { ttl: "1h" }
- [ ] `cache_usage_mapped_to_token_usage`：cache_read/write_input_tokens → TokenUsage
- [ ] `temperature_forwarded`：temperature → API body
- [ ] `adaptive_vs_enabled_mode`：根据 model 选择不同模式
- [ ] `tool_use_start_and_end_emitted`：tool_use block → ToolUseStart + ToolUseArgsChunk + ToolUseEnd 事件
- [ ] `parallel_tool_uses_end_each`：多个并行 tool use 各自 emit ToolUseEnd
- [ ] `tx_none_skips_events`：complete(tx=None) 不 emit 事件但返回正确 ModelResponse
- [ ] `max_tokens_override`：RequestOptions::max_tokens 有值时 override config default
- [ ] `stream_interrupted_returns_error`：SSE 流中断 → ModelError { code: "stream_interrupted" }
- [ ] `missing_usage_reports_adjustment`：成功完成但无 usage → adjustment + default usage
- [ ] `done_usage_matches_model_response`：Done event 和 ModelResponse 的 usage 一致
- [ ] 所有 SSE 测试使用共享的 `test_util::serve_sse_once` 模式（bind 127.0.0.1:0）
- [ ] `cargo test -p agent-runtime-providers` 全绿

## 依赖

- Issue 001：公共类型
