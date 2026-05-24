# 004 · DeepSeek Adapter

## 背景

DeepSeek 使用 OpenAI-compatible 协议，但 thinking mode 实现方式不同：通过 top-level `thinking` 字段（不是 `extra_body`）控制，`reasoning_content` 在 delta 中流式返回。另有两个关键差异：thinking 启用时忽略 sampling 参数、tool-call turns 必须 replay `reasoning_content`。

## 目标

在 providers crate 中实现 `DeepSeekAdapter`，复用 `sse.rs`，正确处理 thinking mode 和 reasoning replay。

## 验收标准

### 结构

- [ ] `crates/agent-runtime-providers/src/deepseek.rs` 文件就位
- [ ] `lib.rs` 中 re-export `DeepSeekAdapter` / `DeepSeekConfig`

### DeepSeekConfig

- [ ] `DeepSeekConfig { model, max_tokens, api_key, api_url }`
- [ ] Model 默认值：`"deepseek-chat"`
- [ ] API key fallback：config.api_key → `DEEPSEEK_API_KEY`
- [ ] API URL 默认：`https://api.deepseek.com`
- [ ] URL 自动归一化到 `/v1/chat/completions`

### ModelAdapter trait 方法

- [ ] `provider_name()` → `"deepseek"`
- [ ] `model_name()` → 构造时传入的 model string
- [ ] `capabilities()` → `ReasoningCapability { supported: true, budget_tokens: false, output_exclusion: false, replay_metadata_required: true }`
- [ ] `complete()` 使用 `sse.rs` 的 `parse_openai_sse_stream`，reasoning_field = `Some("reasoning_content")`，reasoning_details_field = None

### ThinkingLevel 映射

- [ ] Off → top-level `thinking: { "type": "disabled" }`；omit `reasoning_effort`
- [ ] Minimal / Low / Medium / High → `thinking: { "type": "enabled" }` + `reasoning_effort: "high"`
- [ ] XHigh / Max → `thinking: { "type": "enabled" }` + `reasoning_effort: "max"`
- [ ] `thinking_budget_tokens` 忽略——Coerce 记录 OptionAdjustment
- [ ] `thinking` 是 top-level 请求字段，**不是** `extra_body`（因为是直接构建 HTTP JSON，不走 OpenAI SDK）
- [ ] `RequestOptions::max_tokens` 有值时 override config default

### Sampling 参数抑制

- [ ] Thinking 启用时（level != Off），从请求 body 中 **omit** `temperature` 和 `top_p`
- [ ] Thinking 未启用时，正常发送 temperature / top_p

### include_thinking 映射

- [ ] DeepSeek 不支持纯 output exclusion
- [ ] `include_thinking: false` + thinking enabled → Coerce 模式：禁用 thinking 并记录 `OptionAdjustment { option: "include_thinking", reason: "output_exclusion_unsupported_disables_reasoning" }`
- [ ] `include_thinking: false` + thinking enabled → Strict 模式：返回 `ModelError { code: "unsupported_reasoning_output_exclusion" }`

### Reasoning replay

- [ ] Assistant 消息含 `ContentBlock::ToolUse` 时，序列化必须包含 `reasoning_content`（从 `ContentBlock::Thinking.text` 或 `provider_details` 提取）
- [ ] Assistant 消息不含 tool call 时，`reasoning_content` 可省略
- [ ] **不可**盲目 strip 所有 thinking blocks——必须检查是否有 tool call

### Prompt caching

- [ ] DeepSeek 缓存全自动，`CachePolicy` 被忽略（不发额外参数）
- [ ] `prompt_cache_hit_tokens` → `TokenUsage::cache_read_tokens`
- [ ] `prompt_cache_miss_tokens` 可记录到 `TokenUsage::details`
- [ ] `cache_write_tokens` 始终为 0

### Stop reason 映射

- [ ] `stop` → EndTurn, `tool_calls` → ToolUse, `length` → MaxTokens
- [ ] `content_filter` → ContentFilter
- [ ] `insufficient_system_resource` → Interrupted

### 测试

- [ ] `default_api_url`：默认 URL 正确
- [ ] `env_var_fallback`：DEEPSEEK_API_KEY 环境变量 fallback
- [ ] `reasoning_content_maps_to_thinking`：`reasoning_content` → StreamEvent::Thinking + ContentBlock::Thinking
- [ ] `thinking_off_disables_reasoning`：Off → thinking.type disabled，omit reasoning_effort
- [ ] `thinking_levels_map_to_high_and_max`：Minimal/Low/Medium/High → high; XHigh/Max → max
- [ ] `thinking_is_top_level_not_extra_body`：验证 thinking 字段在请求 body top-level
- [ ] `omits_sampling_when_thinking_enabled`：thinking enabled → 不发 temperature/top_p
- [ ] `include_thinking_false_reports_or_errors`：Coerce → adjustment, Strict → error
- [ ] `replays_reasoning_for_tool_call_turns`：assistant + tool_call → reasoning_content 保留
- [ ] `omits_reasoning_for_non_tool_call_turns`：assistant without tool_call → 可省略
- [ ] `cache_hit_tokens_reported`：prompt_cache_hit_tokens → TokenUsage

## 依赖

- Issue 001：公共类型
- Issue 003：`sse.rs` 共享解析器
