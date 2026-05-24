# 003 实现路线

## 步骤

1. **从 OpenAI adapter 提取共享 SSE 解析器**
   - 读 `crates/agent-runtime-core/src/model/openai.rs`（543 行），其中 SSE 解析逻辑约占 200 行
   - 创建 `crates/agent-runtime-providers/src/sse.rs`，`lib.rs` 添加 `pub(crate) mod sse;`
   - 提取 `parse_openai_sse_stream()` 函数——核心签名见 spec
   - 参数化差异点：`reasoning_field: Option<&str>` 和 `reasoning_details_field: Option<&str>`
   - OpenAI 调用时：`reasoning_field = None, reasoning_details_field = None`
   - DeepSeek（issue 004）：`reasoning_field = Some("reasoning_content"), reasoning_details_field = None`
   - OpenRouter（issue 005）：`reasoning_field = Some("reasoning_content")` 或 `Some("reasoning")`（需验证），`reasoning_details_field = Some("reasoning_details")`

2. **实现 SSE 行缓冲**
   - 现有 openai.rs 的 `bytes_stream.next().await` 逐 chunk 读取，按 `\n` 分行，处理 `data: ` 前缀
   - 关键：TCP chunk 可能在行中间断开——需要维护一个 `line_buffer: String`，遇到 `\n` 时才处理完整行
   - `data: [DONE]` → 跳过
   - 空行 → 跳过
   - `data: {...}` → `serde_json::from_str::<Value>` 解析

3. **实现事件提取和累积**
   - Text：`choices[0].delta.content` → 累积到 `Vec<ContentBlock>` + emit `StreamEvent::Text`
   - Tool calls：`choices[0].delta.tool_calls` → 按 index 累积，emit `ToolUseStart` / `ToolUseArgsChunk`；`finish_reason == "tool_calls"` 时 emit 每个 tool 的 `ToolUseEnd`
   - Reasoning：当 `reasoning_field` 有值时，`choices[0].delta.{field}` → emit `ThinkingStart` / `Thinking` / `ThinkingEnd`
   - Usage：从最后一个 chunk 的 `usage` 对象提取
   - Stop reason：`choices[0].finish_reason` 映射（含 deprecated `function_call`）

4. **迁移 OpenAI adapter**
   - 创建 `crates/agent-runtime-providers/src/openai.rs`
   - 从 core openai.rs 复制，修改 import，适配新 trait 签名（同 issue 002 的模式）
   - `build_request_body()` 扩展：加入 `reasoning_effort`（ThinkingLevel 映射）、`stream_options`、`prompt_cache_retention`、`max_tokens` override、`temperature` / `top_p`
   - `complete()` 方法内调用 `sse::parse_openai_sse_stream()` 替换内联解析逻辑
   - 添加 `provider_name()` / `model_name()` / `capabilities()`
   - `lib.rs` 添加 `pub mod openai;` 和 re-export

5. **写测试**
   - SSE 解析器测试（8 个）：独立测试 `parse_openai_sse_stream` 函数，构造 `futures_util::stream::iter()` 模拟字节流
   - OpenAI adapter 测试（12 个）：迁移现有 core 测试 + 新增 thinking/cache/temperature 测试
   - 使用 `serve_sse_once` 模式做 adapter 集成测试

## 要读的现有代码

- `crates/agent-runtime-core/src/model/openai.rs` — 完整 543 行，迁移起点
- 重点关注 `parse_sse_line()` / `handle_chunk()` 相关逻辑（如果有）和 tool_calls 累积逻辑

## 关键决策

- SSE 解析器返回 `(Vec<ContentBlock>, TokenUsage, StopReason)` 而不是 `ModelResponse`——OptionAdjustment 由调用方（各 adapter）负责填充
- Tool call 累积用 `HashMap<u32, (String, String, String)>`（index → id, name, args_buffer），finish 时按 index 排序 emit ToolUseEnd
