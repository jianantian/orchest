# 010 · Python / Node.js SDK Provider Parity

## 背景

Issue 009 定义 Rust SDK provider runtime API 后，Python 和 Node.js 绑定层应只做语言习惯映射。两端不能各自实现 provider routing、API key/env resolution、api_url override 或 provider-specific config construction。

当前 Python 和 Node.js SDK 都 hardcode Anthropic adapter。v0.5 provider runtime API 就位后，两端需要暴露同等能力：provider/model 选择、request options、extended usage、stream event typing 和 examples。

## 目标

让 Python 和 Node.js SDK 用户以各自语言自然的 API 使用同一套 Orchest provider runtime 能力，并保证两端行为一致。

## 验收标准

### Public API Shape

- [ ] Python `Agent` 构造参数支持：
  - `model: str`
  - `api_key: str | None`
  - `api_key_env: str | None`
  - `api_url: str | None`
  - `max_tokens: int | None`
  - `request_options: dict | None`
- [ ] Node.js `AgentOptions` 支持：
  - `model: string`
  - `apiKey?: string`
  - `apiKeyEnv?: string`
  - `apiUrl?: string`
  - `maxTokens?: number`
  - `requestOptions?: RequestOptions`
- [ ] Python public API 使用 snake_case；Node.js public API 使用 camelCase
- [ ] 两端都支持 canonical provider model string 和 legacy Anthropic shorthand，具体 normalization 委托 Rust SDK contract

### Rust Contract Usage

- [ ] Python binding 通过 Issue 009 的 Rust config-first factory 创建 adapter
- [ ] Node.js binding 通过同一个 Rust config-first factory 创建 adapter
- [ ] 两端都不直接 import provider-specific adapter/config 类型
- [ ] 两端都不复制 provider routing 逻辑
- [ ] 两端都把 public API options 映射到同一个 Rust `ProviderRuntimeConfig` / `RequestOptions`

### RequestOptions Mapping

- [ ] Python `request_options` 支持：
  - `thinking`
  - `thinking_budget_tokens`
  - `include_thinking`
  - `compatibility_policy`
  - `max_tokens`
  - `temperature`
  - `top_p`
  - `cache_policy`
- [ ] Node.js `requestOptions` 支持对应 camelCase 字段：
  - `thinking`
  - `thinkingBudgetTokens`
  - `includeThinking`
  - `compatibilityPolicy`
  - `maxTokens`
  - `temperature`
  - `topP`
  - `cachePolicy`
- [ ] enum 字符串取值一致：thinking=`off|minimal|low|medium|high|xhigh|max`；compatibility=`coerce|strict`；cache=`none|auto|long`
- [ ] 无效 enum string 两端都返回清晰错误
- [ ] 未传 request options 时，两端都使用 Rust default

### Runtime Types

- [ ] Python type hints 覆盖扩展后的 `TokenUsage`
- [ ] Node.js `TokenUsage` interface 覆盖扩展后的 `TokenUsage`
- [ ] Node.js `StreamEvent` union 覆盖 text / thinking / thinking_end / tool_use / done
- [ ] Python type hints 不错误收窄 stream delta；推荐用 `TypedDict` 表达 common event shape
- [ ] `model_call_completed.option_adjustments` 可从两端 runtime event 中访问
- [ ] 若两端后续暴露 direct provider/chat helper，则 direct response 也必须保留 `ModelResponse.option_adjustments`

### Examples

- [ ] Python example 展示 DeepSeek model + thinking options
- [ ] Python example 展示 legacy Anthropic shorthand 仍可用
- [ ] Node.js example 展示 OpenRouter nested model + reasoning options
- [ ] Node.js example 展示 legacy Anthropic shorthand 仍可用

### 测试

- [ ] Python binding 测试：canonical provider model passes through Rust config
- [ ] Python binding 测试：legacy shorthand maps through Rust normalization
- [ ] Python binding 测试：request_options maps to Rust `RequestOptions`
- [ ] Python type stub 检查：extended `TokenUsage`
- [ ] Node binding 测试：canonical provider model passes through Rust config
- [ ] Node binding 测试：legacy shorthand maps through Rust normalization
- [ ] Node binding 测试：requestOptions maps to Rust `RequestOptions`
- [ ] Node type check：`RequestOptions`、`TokenUsage`、`StreamEvent` fields compile
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

## 注意

- 本 issue 不重新设计 Rust provider runtime API；如果发现 Rust contract 不足，先回到 Issue 009 修 contract。
- 两端 API 可以按语言习惯命名，但语义和默认值必须一致。
- 不改变 runtime event 顶层 wire format。

## 依赖

- Issue 008：Provider Runtime Hotfix
- Issue 009：Rust SDK Provider Runtime API
