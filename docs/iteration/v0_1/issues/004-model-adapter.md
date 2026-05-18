# 004 · ModelAdapter trait 与 Anthropic Streaming Adapter

## 背景

Run loop 通过 `ModelAdapter` trait 与模型交互，抹平不同 provider 的差异。v0.1 只实现 Anthropic Claude adapter，但 trait 设计要能支持后续扩展。

## 目标

定义 `ModelAdapter` trait，实现 `AnthropicAdapter`，支持 streaming 调用并正确发出 `ModelStreamChunk`。

## 验收标准

- [ ] `ModelAdapter` trait 定义 `call()` 和 `stream()` 两个方法
- [ ] `stream()` 接受 `mpsc::Sender<ModelStreamChunk>` 参数，返回完整 `ModelResponse`
- [ ] `call()` 是 `stream()` 的 wrapper，收集完整 response 后返回（不单独实现）
- [ ] `AnthropicAdapter` 使用 `anthropic` crate（或 `reqwest` 直接调用 API）实现 streaming
- [ ] streaming 时逐 chunk 发出 `ModelStreamChunk::Text { delta }`
- [ ] tool call 参数流式时发出 `ModelStreamChunk::ToolCallArgsChunk { id, delta }`
- [ ] extended thinking 时发出 `ModelStreamChunk::Thinking { delta }`
- [ ] 流结束时发出 `ModelStreamChunk::Done { usage: TokenUsage }`
- [ ] `ModelResponse` 包含完整的 tool calls（从完整 response 一次性解析，不从 streaming chunk 增量解析）
- [ ] `ModelSpec` 支持指定 model id 字符串（如 `"claude-3-5-sonnet-20241022"`）
- [ ] API key 从 `ANTHROPIC_API_KEY` 环境变量读取，也支持构造时传入

## 说明

loop 内部只调用 `stream()`。`call()` 保留是为了测试和 utility 场景。

`ModelResponse` 结构：
```rust
pub struct ModelResponse {
    pub content: Vec<ContentBlock>,   // text blocks + tool_use blocks
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
}

pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}
```
