# 004 实现路线

## 步骤

1. **创建 adapter 骨架**
   - 创建 `crates/agent-runtime-providers/src/deepseek.rs`
   - 参照 openai.rs 的结构（同为 OpenAI-compat 协议）：`DeepSeekAdapter` struct + `DeepSeekConfig` + `from_config()` + `ModelAdapter` impl
   - `lib.rs` 添加 `pub mod deepseek;` 和 re-export
   - 与 OpenAI 的区别主要在 `build_request_body()` 和 `complete()` 的参数配置

2. **实现请求构建**
   - `build_request_body()` 基本复用 OpenAI 的消息序列化逻辑（system → "system", user → "user", tool → "tool", assistant → tool_calls）
   - 关键差异——ThinkingLevel 映射：
     - Off → `"thinking": {"type": "disabled"}`，不发 `reasoning_effort`
     - Minimal–High → `"thinking": {"type": "enabled"}` + `"reasoning_effort": "high"`
     - XHigh–Max → `"thinking": {"type": "enabled"}` + `"reasoning_effort": "max"`
     - `thinking` 是 top-level JSON 字段，直接 `body["thinking"] = ...`
   - Sampling 抑制：thinking enabled 时不写 `temperature` / `top_p` 到 body
   - include_thinking false：在 `complete()` 入口处处理——Coerce 时改 thinking 为 Off 并记录 adjustment，Strict 时直接返回 error
   - Reasoning replay：序列化 assistant 消息时，如果含 `ContentBlock::ToolUse`，必须在消息 JSON 中添加 canonical `reasoning` 字段（可按兼容策略附带 `reasoning_content`）

3. **调用 SSE 解析器**
   - `complete()` 内部：构建 HTTP 请求 → `client.post()` → 获取 byte stream → 调用 `sse::parse_openai_sse_stream(stream, tx.as_ref(), Some("reasoning"), None)`，并兼容 `reasoning_content` alias
   - 解析器返回 `(content, usage, stop_reason)` → 组装 `ModelResponse`
   - DeepSeek cache tokens 映射：从 usage 的 `prompt_cache_hit_tokens` 提取到 `TokenUsage::cache_read_tokens`

4. **写测试（11 个）**
   - URL / env var 测试简单
   - Thinking 映射测试：构造 `RequestOptions` 验证生成的 request body JSON
   - Reasoning replay 测试：构造含 ToolUse 的 assistant message，验证序列化输出含 canonical `reasoning`（兼容模式下允许附带 `reasoning_content`）
   - SSE 集成测试：`serve_sse_once` 模拟 DeepSeek 响应（覆盖 `reasoning` 与 alias `reasoning_content`）

## 要读的现有代码

- `crates/agent-runtime-providers/src/openai.rs` — 完成后作为参考（同协议族）
- `crates/agent-runtime-providers/src/sse.rs` — 理解 `reasoning_field` 参数如何工作

## 关键决策

- Assistant 消息序列化逻辑可以考虑抽取为 `fn serialize_openai_compat_messages()` 共享给 OpenAI / DeepSeek / OpenRouter——但 issue 004 先内联实现，issue 006 再决定是否抽取
- `prompt_cache_hit_tokens` 的提取位置：在 `parse_openai_sse_stream` 返回的 `TokenUsage` 之外，还是让 SSE 解析器理解 provider-specific usage 字段？建议解析器只提取标准 `usage.prompt_tokens` / `completion_tokens`，provider-specific 字段由 adapter 从 raw usage JSON 二次提取
