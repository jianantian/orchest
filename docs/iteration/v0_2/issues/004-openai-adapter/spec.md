# 004 · OpenAI Model Adapter

## 背景

v0.1 只支持 Anthropic Claude。v0.2 增加 OpenAI adapter，验证 `ModelAdapter` trait 的跨 provider 可移植性，并让用户能在不改代码的情况下切换模型。

## 目标

实现 `OpenAiAdapter`，通过与 Anthropic adapter 相同的 trait 接口支持 OpenAI GPT 系列模型，包含 streaming。

## 验收标准

- [ ] `OpenAiAdapter` 实现 `ModelAdapter` trait
- [ ] 支持 streaming（`stream: true`），逐 chunk 发出 `ModelStreamChunk::Text { delta }`
- [ ] 支持 tool call（function calling），正确解析 OpenAI 的 `tool_calls` 响应格式，**由 adapter 负责将原始响应 normalize 成统一的 `Vec<ToolCall>`；run loop 不做任何 provider-specific 分支**
- [ ] `ModelSpec` 支持 `"openai/gpt-4o"`、`"openai/gpt-4o-mini"` 格式，adapter 自动提取 model id 部分
- [ ] API key 从 `OPENAI_API_KEY` 环境变量读取，也支持构造时传入
- [ ] `TokenUsage` 从 OpenAI 响应的 `usage` 字段正确映射（`prompt_tokens` → `input_tokens`，`completion_tokens` → `output_tokens`）
- [ ] 通过与 Anthropic adapter 相同的集成测试套件（smoke test：basic call、tool call、streaming）
- [ ] `ModelStreamChunk::Thinking` 在 OpenAI adapter 中不发出（OpenAI 无 extended thinking）

## 说明

`openai` crate 或直接用 `reqwest` 调用 API 均可。建议直接 `reqwest`，避免引入 openai-rs 的版本依赖问题。OpenAI API base URL 支持覆盖（用于测试 mock server 或 Azure OpenAI）。
