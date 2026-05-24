# 005 · OpenRouter Adapter

## 背景

OpenRouter 是 OpenAI-compatible 协议的代理层，转发请求到多个底层 provider。有几个特有差异：自定义 HTTP headers（`HTTP-Referer` / `X-OpenRouter-Title`）、统一的 `reasoning` 对象（`reasoning.effort` 和 `reasoning.max_tokens` 互斥）、model 名称包含 provider 前缀（如 `anthropic/claude-sonnet-4`）。

关键约束：`reasoning_details` 必须精确保留，不可重排 / 摘要 / 过滤。

## 目标

在 providers crate 中实现 `OpenRouterAdapter`，复用 `sse.rs`，正确处理 OpenRouter-specific headers、reasoning 对象互斥、reasoning_details 保留。

## 验收标准

### 结构

- [ ] `crates/agent-runtime-providers/src/openrouter.rs` 文件就位
- [ ] `lib.rs` 中 re-export `OpenRouterAdapter` / `OpenRouterConfig`

### OpenRouterConfig

- [ ] `OpenRouterConfig { model, max_tokens, api_key, api_url, app_title, site_url }`
- [ ] API key fallback：config.api_key → `OPENROUTER_API_KEY`
- [ ] API URL 默认：`https://openrouter.ai/api`
- [ ] URL 自动归一化到 `/v1/chat/completions`（注意 OpenRouter 也用 `/v1/chat/completions`）
- [ ] `app_title` fallback → `OPENROUTER_APP_TITLE` env var
- [ ] `site_url` fallback → `OPENROUTER_SITE_URL` env var

### 自定义 headers

- [ ] `HTTP-Referer` header：从 `site_url` 设置
- [ ] `X-OpenRouter-Title` header：从 `app_title` 设置
- [ ] 标准的 `Authorization: Bearer` header

### ModelAdapter trait 方法

- [ ] `provider_name()` → `"openrouter"`
- [ ] `model_name()` → 构造时的完整 model string（含 provider prefix，如 `anthropic/claude-sonnet-4`）
- [ ] `capabilities()` → conservative `Assumed` fallback（OpenRouter 路由多种 model，能力不可静态确定）
- [ ] `complete()` 使用 `sse.rs`，reasoning_field = `Some("reasoning")`（当底层 model 支持时），reasoning_details_field = `Some("reasoning_details")`

### Model 名称透传

- [ ] Model name 不做任何处理，直接传给 OpenRouter API（`anthropic/claude-sonnet-4` → `model: "anthropic/claude-sonnet-4"`）
- [ ] `create_adapter("openrouter/anthropic/claude-sonnet-4", ...)` 中 `openrouter/` 后面的全部内容作为 model name

### ThinkingLevel 映射

- [ ] 使用 OpenRouter 的 `reasoning` 对象
- [ ] `reasoning.effort` 和 `reasoning.max_tokens` 互斥——同时只发一个
- [ ] 当 `thinking_budget_tokens` 有值时：发 `reasoning: { max_tokens: N }`，不发 `effort`
- [ ] 当 `thinking_budget_tokens` 无值时：发 `reasoning: { effort: "<level>" }`，直接映射 ThinkingLevel → none / minimal / low / medium / high / xhigh / max
- [ ] 如果路由的 model 不支持所选形式，行为由 `CompatibilityPolicy` 决定

### include_thinking 映射

- [ ] `include_thinking: true` → `reasoning.exclude: false`（或不发 exclude 字段）
- [ ] `include_thinking: false` → `reasoning.exclude: true`

### reasoning_details 保留

- [ ] OpenRouter 返回的 `reasoning_details` 必须精确保留到 `ContentBlock::Thinking.provider_details`
- [ ] 不可重排、摘要、过滤、重构
- [ ] Replay 时必须原样传回

### Prompt caching

- [ ] Anthropic-backed models：`CachePolicy::Auto` 时添加 top-level `cache_control`
- [ ] 其他 provider 的 model：caching 自动，不发额外参数
- [ ] `prompt_tokens_details.cached_tokens` → `TokenUsage::cache_read_tokens`

### 测试

- [ ] `default_api_url`：默认 URL 正确
- [ ] `sends_custom_headers`：X-OpenRouter-Title / HTTP-Referer 正确发送
- [ ] `model_passthrough`：完整 model path 透传不修改
- [ ] `reasoning_maps_to_thinking`：reasoning content → Thinking events
- [ ] `reasoning_details_preserved`：reasoning_details → provider_details，精确保留
- [ ] `reasoning_object_from_thinking_level`：ThinkingLevel → reasoning.effort
- [ ] `reasoning_effort_and_max_tokens_are_exclusive`：budget_tokens 有值 → 只发 max_tokens，不发 effort
- [ ] `include_thinking_false_sends_exclude`：include_thinking=false → reasoning.exclude=true
- [ ] `env_var_fallback`：OPENROUTER_API_KEY / OPENROUTER_APP_TITLE / OPENROUTER_SITE_URL 环境变量

## 依赖

- Issue 001：公共类型
- Issue 003：`sse.rs` 共享解析器
