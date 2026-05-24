# 009 实现路线

## 步骤

1. **设计 Rust SDK config**
   - 读取 `agent-runtime-providers` 的 `create_adapter()`、各 adapter config、`ModelSpec`、`AgentConfig`
   - 新增 `ProviderRuntimeConfig`，集中表达 model/api_key/api_key_env/api_url/max_tokens
   - 实现 `normalize_model()`：canonical provider string 原样处理；无 prefix 时映射为 Anthropic shorthand
   - 实现 API key resolution：explicit api_key > api_key_env env var > provider default env var；空字符串报错

2. **扩展统一 factory**
   - 将现有 `create_adapter(model, api_key)` 保留为 convenience wrapper 或标为 legacy helper
   - 新增 config-first factory，例如 `create_adapter_from_config(config)`
   - factory 内部统一处理 provider routing、env fallback、api_url、max_tokens
   - 保证 SDK/core 外部不需要构造 provider-specific config

3. **接入 AgentConfig / run loop**
   - 给 `AgentConfig` 添加 `request_options: RequestOptions`，使用 serde default 保持旧配置反序列化兼容
   - 让 run loop 调用 `complete(messages, tools, &config.request_options, tx)`
   - 明确 `ModelSpec` 到 `ProviderRuntimeConfig` 的转换路径；`ProviderRuntimeConfig` 是唯一进入 factory 的 canonical config

4. **明确 max_tokens 规则**
   - provider config `max_tokens` 设置 adapter 默认值
   - request options `max_tokens` 设置单次请求 override
   - 两者为空时 factory 设置 4096
   - 写单元测试覆盖 default、config override、request override

5. **稳定 serialization**
   - 检查 `TokenUsage`、`StreamEvent`、`ModelResponse`、`RuntimeEvent` 的 serde 输出
   - 给 `RuntimeEvent::ModelCallCompleted` 增加 `option_adjustments`，让 Agent event consumers 能看到 provider option coercions
   - 为 SDK 依赖的 wire shape 写 snapshot-style 或 explicit field tests
   - 保持顶层 `model_stream_chunk` event 名不变

6. **补 Rust examples**
   - 添加 provider runtime config example
   - 添加 OpenRouter nested model name example
   - examples 可以只构造 config / adapter，不要求真实网络调用

7. **验证**
   - `cargo test --workspace`
   - `cargo clippy --workspace -- -D warnings`
   - 如果 core tests 需要更新 `AgentConfig` 构造，使用 default request options，不改变测试语义

## 要读的现有代码

- `crates/agent-runtime-providers/src/lib.rs`
- `crates/agent-runtime-providers/src/types.rs`
- `crates/agent-runtime-core/src/model/mod.rs`
- `crates/agent-runtime-core/src/run.rs`
- `crates/agent-runtime-core/src/events.rs`
- `crates/agent-runtime-core/tests/`

## 关键决策

- Rust config-first factory 是 canonical API；tuple-style `create_adapter(model, api_key)` 只能作为 convenience wrapper。
- Provider routing 只存在一处：providers/core factory。
- `AgentConfig.request_options` 是 runtime 传递 provider behavior 的唯一入口。
- Language SDK parity 必须建立在本 issue 的 Rust contract 上。
