# 005 实现路线

## 步骤

1. **创建 adapter 骨架**
   - 创建 `crates/agent-runtime-providers/src/openrouter.rs`
   - 结构参照 DeepSeek adapter（同为 OpenAI-compat + 额外字段）
   - `OpenRouterConfig` 比其他 adapter 多两个字段：`app_title` / `site_url`
   - `from_config()` 中处理 3 个 env var resolution：`OPENROUTER_API_KEY` / `OPENROUTER_APP_TITLE` / `OPENROUTER_SITE_URL`
   - `lib.rs` 添加 `pub mod openrouter;` 和 re-export

2. **实现自定义 headers 和请求构建**
   - HTTP 请求额外添加 `HTTP-Referer` 和 `X-OpenRouter-Title` headers（从 config 中读取）
   - ThinkingLevel 映射到 `reasoning` 对象——关键是互斥逻辑：
     - `thinking_budget_tokens` 有值 → `reasoning: { max_tokens: N }`，不发 effort
     - 否则 → `reasoning: { effort: "<level>" }`，ThinkingLevel 直接映射字符串
   - include_thinking → `reasoning.exclude` 字段
   - Model name 透传：不做任何处理

3. **reasoning_details 保留**
   - 调用 `sse::parse_openai_sse_stream(stream, tx, Some("reasoning"), Some("reasoning_details"))`，并在解码层兼容 `reasoning_content` alias
   - SSE 解析器会把 `reasoning_details` 存入 `ContentBlock::Thinking.provider_details`
   - Tool-call continuation 时保序透传完整 consecutive reasoning blocks；若缺失或篡改，返回可诊断错误（或 warning event）
   - `capabilities()` 返回 `CapabilitySource::Assumed`（OpenRouter 路由多种 model，能力不可静态确定）

4. **写测试（9 个）**
   - URL / env var / headers 测试：用 `serve_sse_once` 验证请求 headers
   - Model passthrough 测试：验证 `model_name()` 返回完整路径
   - Reasoning 相关测试：验证 request body 中 `reasoning` 对象的 effort / max_tokens 互斥
   - reasoning_details 保留测试：mock SSE 响应含 `reasoning_details` 字段，验证 `ContentBlock::Thinking.provider_details` 精确保留

## 要读的现有代码

- `crates/agent-runtime-providers/src/deepseek.rs` — 完成后作为结构参考
- `crates/agent-runtime-providers/src/sse.rs` — 理解 `reasoning_details_field` 参数如何工作

## 关键决策

- OpenRouter reasoning 字段 canonical 使用 `"reasoning"`；`"reasoning_content"` 仅作为兼容 alias（解码层处理，不作为主字段）
- Prompt caching 对 Anthropic-backed models 的特殊处理：可以通过检查 `model` 字符串是否以 `anthropic/` 开头来判断
