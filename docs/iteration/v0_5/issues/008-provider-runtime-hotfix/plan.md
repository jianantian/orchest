# 008 实现路线

## 步骤

1. **复现 workspace 失败**
   - 跑 `cargo test --workspace`
   - 单独跑 `cargo test -p agent-runtime-core tool::mcp::tests::http_tools_call_does_not_retry -- --nocapture`
   - 修复测试或实现，使测试能稳定证明 `tools/call` 发送一次且不 retry

2. **修 DeepSeek usage parsing**
   - 扩展 OpenAI-compatible SSE parser 返回 raw usage，或提供 provider hook 读取 usage JSON
   - 在 DeepSeek adapter 中读取：
     - `usage.prompt_cache_hit_tokens`
     - `usage.prompt_cache_miss_tokens`
   - 写测试使用 DeepSeek 官方字段
   - 保留 `prompt_tokens_details.cached_tokens` 兼容逻辑

3. **修 OpenRouter reasoning replay**
   - 删除 `reasoning_blocks` 输出路径
   - 设计 `ContentBlock::Thinking.provider_details` 到 OpenRouter `reasoning_details` 的唯一合法序列化规则
   - 多个 consecutive reasoning detail blocks 合并为一个 `reasoning_details` array，保持原始顺序
   - Flatten 规则：provider_details 为 array 时按顺序展开；为 object 时作为单个 item append；其他类型返回 `invalid_reasoning_replay`
   - 如果 block 同时有 plaintext 和 provider_details，优先回传 provider_details；plaintext 仅用于无 provider_details 的模型
   - 无法合法表达时返回 `ModelError { code: "invalid_reasoning_replay" }`

4. **修 OpenAI reasoning support behavior**
   - 把支持判断抽到 helper/table
   - 对 unsupported / unknown model + requested thinking：
     - Coerce：不发 `reasoning_effort`，记录 `OptionAdjustment`
     - Strict：返回 `ModelError`
   - 覆盖 `thinking_budget_tokens` 与 `thinking` request 的组合

5. **确认 RequestOptions handoff 边界**
   - 读取 issue 009 Rust SDK Provider Runtime API
   - 确认 `AgentConfig.request_options` 由 issue 009 实现
   - 本 issue 只记录该 blocker，不在本 issue 中宣称 SDK options 已可进入 runtime

6. **验证**
   - `cargo test -p agent-runtime-providers`
   - `cargo test --workspace`
   - `cargo clippy --workspace -- -D warnings`
   - `cargo fmt --check`

## 要读的现有代码

- `crates/agent-runtime-core/src/tool/mcp.rs`
- `crates/agent-runtime-core/src/run.rs`
- `crates/agent-runtime-providers/src/sse.rs`
- `crates/agent-runtime-providers/src/deepseek.rs`
- `crates/agent-runtime-providers/src/openrouter.rs`
- `crates/agent-runtime-providers/src/openai.rs`
- `docs/external/deepseek/thinking_mode.md`
- `docs/external/openrouter/reasoning.md`

## 关键决策

- Workspace 全绿是 v0.5 provider runtime 的 release gate。
- Provider-specific usage/replay behavior belongs in adapter code, not SDK bindings.
- Non-standard provider request fields are not acceptable unless documented by the provider.
- RequestOptions must not be marketed as SDK-configurable until the run loop can actually receive it.
