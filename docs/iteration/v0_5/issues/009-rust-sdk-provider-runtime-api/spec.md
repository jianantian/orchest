# 009 · Rust SDK Provider Runtime API

## 背景

v0.5 的 001–007 建立了独立 `agent-runtime-providers` crate，并让 core 通过 re-export 使用 provider 类型。但这只解决了 provider adapter 的归一化和 core 编译迁移，还没有定义 Rust SDK 面向应用开发者的 provider runtime API。

如果直接让 Python / Node.js 绑定层各自补 `api_key`、`api_url`、model prefix、`RequestOptions` 和默认值规则，就会把 provider 选择逻辑复制到多个语言 SDK。更合理的设计是：先在 Rust/core 层形成稳定的 SDK contract，再让 Python / Node.js 做薄映射。

## 目标

定义并实现 Rust SDK 层的统一 provider runtime API，使 agent runtime 可以从一个稳定的配置结构创建 provider adapter、传递 `RequestOptions`、保留旧 Anthropic shorthand，并输出稳定的 event / usage serialization contract。

## 验收标准

### ProviderRuntimeConfig

- [ ] 新增统一配置类型，名称固定为 `ProviderRuntimeConfig`
- [ ] 字段至少包含：
  - `model: String`
  - `api_key: Option<String>`
  - `api_key_env: Option<String>`
  - `api_url: Option<String>`
  - `max_tokens: Option<u32>`
- [ ] `model` 支持 canonical provider string：`anthropic/...`、`openai/...`、`deepseek/...`、`openrouter/...`
- [ ] 未带 provider prefix 的 model string 作为 backward-compatible Anthropic shorthand，内部规范化为 `anthropic/<model>`
- [ ] normalization helper 可单测，且不依赖 Python / Node.js 绑定层
- [ ] API key resolution 优先级固定：`api_key` 显式值 > `api_key_env` 指向的 env var > provider 默认 env var
- [ ] 空字符串 API key 视为 missing / invalid，返回稳定 `ModelError { code: "missing_api_key" }` 或 `ModelError { code: "invalid_api_key" }`
- [ ] provider 默认 env var 固定为：`ANTHROPIC_API_KEY`、`OPENAI_API_KEY`、`DEEPSEEK_API_KEY`、`OPENROUTER_API_KEY`

### Unified Factory

- [ ] 新增或扩展 factory，使其接收统一 config，而不是只接收 `(model, api_key)`
- [ ] factory 负责 provider routing、api key/env fallback、api_url override、max_tokens default
- [ ] provider-specific adapter config 只在 factory 内部构造
- [ ] SDK / core 调用方不需要 import `AnthropicConfig`、`OpenAiConfig`、`DeepSeekConfig`、`OpenRouterConfig`
- [ ] 未知 provider 返回稳定 `ModelError { code: "unknown_provider" }`
- [ ] 无效 model string 返回稳定 `ModelError { code: "invalid_model" }`
- [ ] 默认 `max_tokens` 为 4096，除非 config 显式覆盖

### AgentConfig Integration

- [ ] `AgentConfig` 的 provider construction canonical path 是 `ProviderRuntimeConfig`
- [ ] 现有 `ModelSpec` 仅作为 backward-compatible input，必须无损转换为 `ProviderRuntimeConfig` 后再进入 factory
- [ ] core / SDK 代码不得绕过 `ProviderRuntimeConfig` 直接构造 provider-specific adapter config
- [ ] `AgentConfig` 能携带 `RequestOptions`
- [ ] run loop 调用 `ModelAdapter::complete()` 时使用 `AgentConfig.request_options`，不再强制使用 `RequestOptions::default()`
- [ ] 未设置 request options 时仍使用 `RequestOptions::default()`
- [ ] `RequestOptions.max_tokens` 与 provider config `max_tokens` 的优先级明确且可测试：
  - provider config `max_tokens` 是 adapter 默认输出上限
  - `RequestOptions.max_tokens` 是单次 request override
  - 两者都为空时使用 4096

### Serialization Contract

- [ ] `TokenUsage` serialization 稳定包含：`input_tokens`、`output_tokens`、`reasoning_tokens`、`cache_read_tokens`、`cache_write_tokens`、`details`
- [ ] `StreamEvent` serialization 覆盖 text / thinking / tool-use / done 事件
- [ ] `RuntimeEvent::ModelStreamChunk` 顶层 wire event 名保持不变；内部 `delta` 使用 re-export 后的 `StreamEvent`
- [ ] `RuntimeEvent::ModelCallCompleted` 携带 `option_adjustments: Vec<OptionAdjustment>`；为空时默认为 empty vec 并可在序列化中省略
- [ ] Direct provider helpers (`chat()` / `stream_chat()`) 继续通过 `ModelResponse.option_adjustments` 返回 adjustments
- [ ] SDK event consumers 可通过 `model_call_completed.option_adjustments` 读取非空 adjustments

### Rust Examples

- [ ] 增加 Rust standalone example：通过 canonical `deepseek/...` model string + `RequestOptions` 创建 agent 或 provider call
- [ ] 增加 Rust standalone example：通过 `openrouter/anthropic/...` model string 保留完整 routed model name
- [ ] examples 不需要真实 provider API key 才能编译

### 测试

- [ ] `provider_config::normalizes_legacy_anthropic_model`
- [ ] `provider_config::preserves_canonical_provider_model`
- [ ] `provider_config::openrouter_preserves_nested_model_name`
- [ ] `provider_config::api_key_precedence_explicit_then_env`
- [ ] `provider_config::api_key_empty_string_rejected`
- [ ] `provider_config::api_url_override_reaches_adapter_config`
- [ ] `provider_config::max_tokens_default_and_override`
- [ ] `agent_config::request_options_reach_model_complete`
- [ ] `runtime_event::model_call_completed_includes_option_adjustments`
- [ ] `serialization::token_usage_contains_extended_fields`
- [ ] `serialization::stream_event_thinking_end_preserves_signature_and_details`
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

## 注意

- Rust SDK contract 是 Python / Node.js SDK 的上游设计，不把语言绑定限制带入这里。
- 不允许 Python / Node.js 绑定层各自实现 provider routing 或 provider-specific config construction。
- 不要求发布 crates.io；本 issue 只稳定 workspace 内 public API。
- `ProviderRuntimeConfig` 是唯一 canonical Rust provider config；`ModelSpec` 旧字段只能通过 conversion/backward compatibility 维护。

## 依赖

- Issue 008：Provider Runtime Hotfix
- Issue 006：Factory 函数与 Telemetry
- Issue 007：Core 迁移与 workspace 验证
