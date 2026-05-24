# 008 · Provider Runtime Hotfix

## 背景

对 v0.5 issue 001–007 的实现做 code review 后，发现 provider runtime 仍有几个 release-blocking contract gap。`agent-runtime-providers` 自身测试通过，但 workspace 级验证失败，并且 DeepSeek / OpenRouter / OpenAI 的部分 provider-specific contract 与外部文档或 v0.5 issue spec 不一致。

这些问题应该在 Python / Node.js SDK parity 之前修复。否则语言绑定会建立在错误的 Rust provider contract 上。

## 目标

修复 v0.5 provider runtime 的已知 contract gap，使 001–007 的实现回到可依赖状态，并为后续 Rust SDK API 与 Python / Node.js parity 打稳基础。

## Review Findings

### Workspace 验证失败

- `cargo test --workspace` 当前失败在 `tool::mcp::tests::http_tools_call_does_not_retry`
- 失败信息：`tools/call should be sent exactly once, not retried (got 0)`
- 虽然失败点在 core MCP 测试，但 v0.5 issue 007 的验收要求 workspace 全绿，因此这是 release blocker

### DeepSeek cache usage 映射不完整

- v0.5 issue 004 要求 `prompt_cache_hit_tokens` → `TokenUsage::cache_read_tokens`
- 当前实现只处理 OpenAI-style `usage.prompt_tokens_details.cached_tokens`
- DeepSeek-specific `prompt_cache_miss_tokens` 也没有进入 `TokenUsage::details`

### OpenRouter reasoning replay 字段错误

- OpenRouter 文档要求 replay 使用 `message.reasoning` 或完整 `message.reasoning_details`
- 当前实现单 reasoning block 时写 `reasoning` / `reasoning_details`，但多 block 时写非标准字段 `reasoning_blocks`
- 这会破坏 tool-call continuation 下的 reasoning preservation

### OpenAI reasoning capability 判断过窄

- 当前 `supports_reasoning()` 只按 `o1` / `o3` / `o4` 前缀判断
- 对实际支持 reasoning 但不匹配该前缀的模型，`ThinkingLevel` 会被静默忽略且没有 `OptionAdjustment`
- 需要改成显式 capability/config 规则，或至少在不能确认支持时记录 adjustment / strict error

### RequestOptions 仍无法从 runtime config 进入 run loop

- core run loop 主模型调用仍使用 `RequestOptions::default()`
- context compaction 调用也使用 `RequestOptions::default()`
- 这不属于 issue 007 的原始范围，但必须在 Rust SDK Provider Runtime API 前明确记录为直接前置依赖

## 验收标准

### Workspace tests

- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `cargo fmt --check` 全绿

### MCP regression

- [ ] `tool::mcp::tests::http_tools_call_does_not_retry` 稳定通过
- [ ] 测试能区分“未发送请求”和“发送一次但不 retry”
- [ ] side-effectful `tools/call` 失败后不会自动重试

### DeepSeek usage mapping

- [ ] SSE usage 中 `prompt_cache_hit_tokens` 映射到 `TokenUsage::cache_read_tokens`
- [ ] SSE usage 中 `prompt_cache_miss_tokens` 记录到 `TokenUsage::details["prompt_cache_miss_tokens"]`
- [ ] 继续支持 OpenAI-style `prompt_tokens_details.cached_tokens`
- [ ] 新增测试使用 DeepSeek 官方字段，而不是只用 OpenAI-compatible nested usage 字段

### OpenRouter reasoning replay

- [ ] Assistant tool-call continuation replay 不再发送 `reasoning_blocks`
- [ ] 当 `ContentBlock::Thinking.provider_details` 存在时，完整原样写入 `message.reasoning_details`
- [ ] 当只有 plaintext thinking 时，写入 `message.reasoning`
- [ ] 多个 consecutive reasoning details 必须保序、完整回传；不能摘要、重排、过滤
- [ ] 合并规则明确且可测试：如果单个 `provider_details` 是 array，则按原顺序 flatten 到 `reasoning_details`；如果是 object，则作为一个 item append；其他 JSON 类型返回 `ModelError { code: "invalid_reasoning_replay" }`
- [ ] 若无法以 OpenRouter 支持的字段表达 replay 内容，返回可诊断 `ModelError`，不要发送非标准字段
- [ ] 新增测试覆盖多个 reasoning details block + tool call replay

### OpenAI reasoning support behavior

- [ ] `ThinkingLevel` 对 unsupported / unknown reasoning model 不再静默丢弃
- [ ] `CompatibilityPolicy::Coerce` 下记录 `OptionAdjustment`
- [ ] `CompatibilityPolicy::Strict` 下返回稳定 `ModelError`
- [ ] capability 判断规则集中在一个可测试 helper 或 table 中
- [ ] 新增测试覆盖 unknown reasoning request 的 coerce / strict 行为

### RequestOptions handoff

- [ ] 本 issue 不直接实现 `AgentConfig.request_options`
- [ ] 在 issue 009 中将 `AgentConfig.request_options` 列为直接前置 blocker
- [ ] 本 issue 不声称 SDK options 已可进入 runtime

## 注意

- 不要在 Python / Node.js binding 中绕过这些问题；provider/runtime contract 必须先在 Rust 层正确。
- 不要把 OpenRouter replay 修成 provider-specific ad hoc 字段。只使用外部文档支持的 `reasoning` / `reasoning_details`。
- DeepSeek 当前外部文档明确返回 `reasoning_content`；v0.5 内部 canonical 可以继续用 `reasoning`，但 decoding 必须兼容 `reasoning_content`，replay 字段必须与 provider 实际接受行为一致。

## 依赖

- Issue 001–007 的当前实现
- 外部文档：
  - `docs/external/deepseek/thinking_mode.md`
  - `docs/external/openrouter/reasoning.md`
