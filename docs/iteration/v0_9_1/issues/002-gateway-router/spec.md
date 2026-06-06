# 002 · Gateway + router

GitHub: [#110](https://github.com/jianantian/orchest/issues/110)

## 背景

实现 `AsrGateway` 和确定性 `AsrRouter`，包括集中路由配置、兼容性校验、fake-provider 测试。

## 契约

### 输入

- 001 定义的所有公共类型
- PRD 路由规则和兼容性策略

### 输出

- `AsrGateway` 可通过 `transcribe()` / `start_stream()` 路由到注册 provider
- 确定性路由选择
- Strict/Coerce 兼容性校验
- Fake provider 用于测试

## 影响范围

### 修改文件

- `crates/agent-runtime-asr-providers/src/routing.rs` — `AsrGateway`, `AsrRouter` 实现逻辑
- `crates/agent-runtime-asr-providers/src/config.rs` — `create_asr_provider_from_config()` factory, route config parsing

### 新增文件

- `crates/agent-runtime-asr-providers/tests/fake_provider.rs`
- `crates/agent-runtime-asr-providers/tests/router.rs`

## 实现要点

### Gateway

- `AsrGateway` 持有 `AsrRouter` + `AsrGatewayConfig`
- `AsrGatewayConfig.route_config_path`: 可选，启动时一次加载
- `transcribe()` / `start_stream()` 代理到选中的 provider
- `trace_id` 为 `None` 时自动生成，注入 `RouteSelected`, `Started` 及后续所有事件

### Router

确定性路由逻辑：

1. 有 `model` → `normalize_asr_provider_model()` → 直接选中
2. 无 `model` → 加载集中路由配置 → 构建候选集
3. 按 capability 过滤
4. 按 language/region 过滤
5. 按 latency/cost 约束过滤
6. 最低 `priority` 胜出，tie-break 按 normalized provider/model 字符串排序

### Route Config

TOML 格式：

```toml
[[routes]]
model = "volcengine/bigmodel_async"
priority = 10
languages = ["zh-CN"]
regions = ["cn"]
max_latency_ms = 800
```

路径由应用配置，crate 只负责解析/校验类型。

### Compatibility Validation (Strict)

- 不支持的 audio format / sample rate / channel → `unsupported_audio_format`
- 不支持的 timeline mode → `unsupported_option`
- 不支持的 endpointing mode → `unsupported_option`（`ProviderDefault` 隐式可用）
- 不支持的 `silence_timeout` → `unsupported_option`
- 不支持的 `speaker_diarization` → `unsupported_option`
- 不支持的 `word_timestamps` → `unsupported_option`
- 不支持的 `hot_words` / `context_prompt` → `unsupported_option`
- 不支持/未知的 `provider_options` keys → `unsupported_option` / `invalid_request`
- sparse timeline + 需要 continuous audio 的 provider → `unsupported_option`

### Compatibility Validation (Coerce)

- 记录 `OptionAdjustment`，不 silent drop
- 可路由到另一个 provider，但不 transcode / 不强制 auto-detect

### API Key Resolution

1. explicit `api_key`
2. explicit `api_key_env`（缺失/空 → error，不 fallback）
3. provider default env var

### 约束

- `provider_options` 非空但无 explicit `model` → `invalid_request`
- 无 `model` 且无 route config → `no_matching_provider`
- bare model string (无 `provider/` 前缀) → `invalid_model`

## 验收标准

- [ ] `AsrGateway` can route requests to registered fake providers based on centralized route config priority and language
- [ ] Automatic routing only runs when request `model` is omitted and a centralized route config file is configured
- [ ] Route config parsing/validation is centralized in the ASR crate
- [ ] Omitting request `model` without configured route config returns `no_matching_provider`
- [ ] Router tie-break behavior is deterministic (tested)
- [ ] Requests with non-empty `provider_options` and no explicit `model` return `invalid_request`
- [ ] ASR model normalization rejects bare model strings without provider prefix
- [ ] Strict compatibility rejects unsupported streaming audio format, sample rate, channel count, timeline, endpointing mode, speaker diarization, word timestamps
- [ ] Strict compatibility rejects unsupported `hot_words` and `context_prompt` with `unsupported_option`
- [ ] Strict compatibility rejects unsupported or unknown `provider_options` keys/values
- [ ] Strict compatibility rejects sparse speech-only timelines for providers requiring continuous realtime audio
- [ ] `EndpointingMode::ProviderDefault` is implicitly available and not required in capabilities
- [ ] `EndpointingOptions.silence_timeout` unsupported use returns `unsupported_option` in strict mode
- [ ] Coerce mode records `OptionAdjustment` for adjusted options
- [ ] API key resolution follows explicit key → explicit env var → provider default env var
- [ ] Trace ID auto-generated when not provided
- [ ] Fake-provider tests exercise routing, compatibility, and error paths
- [ ] `cargo test -p agent-runtime-asr-providers` passes

## 依赖

- 001 Crate scaffold + public types (#109)
