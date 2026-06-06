# 002 实现路线

## 要读的现有代码

- `crates/agent-runtime-providers/src/lib.rs` — `create_adapter_from_config()`, `normalize_provider_model()`, `resolve_api_key()` 完整逻辑——ASR 版本应平行实现
- `crates/agent-runtime-providers/src/registry.rs` — `ProviderRegistry` / `ProviderFactory` trait 模式
- `crates/agent-runtime-aigc-providers/src/gateway.rs` — AIGC gateway 结构，参考 gateway 的 trait dispatch 方式
- `crates/agent-runtime-aigc-providers/src/lib.rs` — `create_image_provider_from_config()` 的 match-on-provider 模式

## 步骤

### 1. 实现 `normalize_asr_provider_model()` (`config.rs`)

- 把 001 的 `todo!()` 替换为实现
- 关键差异：**不 fallback bare model**。无 `/` → `AsrError { code: InvalidRequest, message: "ASR model must use 'provider/model' format" }`
- 空 provider 或空 model → `AsrError { code: InvalidRequest }`
- 正常 case: split_once('/'), 返回 `NormalizedAsrProviderModel { provider, model }`
- 写测试: valid `"volcengine/bigmodel_async"`, valid `"aliyun/fun-asr-realtime"`, reject `"bigmodel_async"`, reject `""`, reject `"volcengine/"`, reject `"/bigmodel_async"`

### 2. 实现 `create_asr_provider_from_config()` (`config.rs`)

- normalize model → match provider name → 构造 adapter
- API key resolution: 参照 `agent-runtime-providers` 的 `resolve_api_key()` 三级逻辑
  - explicit `api_key` → non_empty check
  - explicit `api_key_env` → env var lookup, 缺失/空 → error (不 fallback)
  - provider default env var: `"volcengine"` → `VOLCENGINE_ACCESS_KEY`, `"aliyun"` → `DASHSCOPE_API_KEY` (或其他，查 vendor docs)
- 返回 `Arc<dyn AsrProvider>`
- 写 API key resolution 测试（模式同 LLM provider tests）

### 3. 实现 route config 解析 (`config.rs`)

Route config 的解析/校验放 `config.rs`（与 `AsrGatewayConfig` 同模块），`routing.rs` 只使用 parsed routes。

- 添加 `toml = "0.8"` 依赖（workspace 当前没有 `toml` crate）
- `AsrRouteConfig` — TOML 反序列化 struct:
  ```rust
  #[derive(Deserialize)]
  struct AsrRouteConfig {
      routes: Vec<AsrRouteEntry>,
  }
  #[derive(Deserialize)]
  struct AsrRouteEntry {
      model: String,
      priority: u8,
      languages: Vec<String>,
      regions: Option<Vec<String>>,
      max_latency_ms: Option<u64>,
      max_cost_micros_per_minute: Option<u64>,
  }
  ```
- 解析时 normalize 每个 entry 的 model，invalid → error
- 添加 `toml` crate 依赖
- 测试: valid TOML → `Vec<AsrRoute>`, invalid model string → error, empty routes → empty vec

### 4. 实现 `AsrRouter` 确定性路由 (`routing.rs`)

- `AsrRouter::new(providers, routes)` — 注册 provider instances + route entries
- `AsrRouter::select(request) -> Result<Arc<dyn AsrProvider>, AsrError>`:
  1. request has model → normalize → lookup in providers HashMap → not found → `NoMatchingProvider`
  2. request omits model → no routes loaded → `NoMatchingProvider`
  3. request omits model → filter routes by capability compatibility
  4. Filter by language (route.languages ∩ request.options.language)
  5. Filter by region (if route has regions)
  6. Apply latency/cost constraints
  7. Sort: lowest priority first, tie-break by normalized model string (lexicographic)
  8. Return first match
- `provider_options` 非空 + model 为 None → `InvalidRequest` (在 gateway 层 check)

### 5. 实现 compatibility validation (`compatibility.rs`)

拆为独立模块 `src/compatibility.rs`，`lib.rs` 加 `pub mod compatibility;`。这是本 issue 最重的部分，200+ 行代码，放 `routing.rs` 会过重。

对照 `AsrModelCapabilities` 逐字段校验：

**Strict mode:**
- `streaming_inputs` — match `StreamingAudioFormat` against capabilities: format, sample_rate_hz (SampleRateSupport), channels (ChannelSupport)
- `audio_timeline_modes` — request timeline ∉ capabilities → `UnsupportedOption`
- `endpointing_modes` — request mode (非 ProviderDefault) ∉ capabilities → `UnsupportedOption`；ProviderDefault 始终通过
- `silence_timeout` — 只在 AcousticSilence mode + capability 支持时生效，否则 → `UnsupportedOption`
- `word_timestamps` — capabilities.word_timestamps=false → `UnsupportedOption`
- `speaker_diarization` — capabilities.speaker_diarization=false → `UnsupportedOption`
- `hot_words` — capabilities.hot_words=false → `UnsupportedOption`
- `context_prompt` — capabilities.context_prompt=false → `UnsupportedOption`
- `provider_options` keys — ∉ `provider_option_keys` → `UnsupportedOption`

**Coerce mode:**
- 同样逐字段检查，不支持时 record `OptionAdjustment` 而非 error
- 可以选择另一个 provider route（通过 router filter），但不 transcode、不强制 auto-detect

### 6. 实现 `AsrGateway` (`routing.rs`)

- `AsrGateway::new(router, config)` — 如果 `config.route_config_path` 有值，加载并解析 route config
- `AsrGateway::transcribe(request)`:
  1. Validate: `provider_options` 非空 + model 为 None → `InvalidRequest`
  2. Generate trace_id if None
  3. Router select provider
  4. Compatibility validation
  5. Delegate to `provider.transcribe(request)`
- `AsrGateway::start_stream(request)`:
  1. 同上 validate + select + compatibility
  2. Delegate to `provider.start_stream(request)`
  3. 注入 trace_id 到 stream events（实际注入机制可能在 003 实现）

### 7. 实现 fake provider (`tests/fake_provider.rs`)

- `FakeAsrProvider` impl `AsrProvider`:
  - `capabilities()` 返回可配置的 `AsrModelCapabilities`
  - `transcribe()` 返回 `UnsupportedOperation`
  - `start_stream()` 返回一个 pre-populated `AsrStream`（在 002 阶段可先返回 `todo!()` 或简单 channel pair，003 会完善）
- 可配置: 支持/不支持的 features，用于 compatibility 测试

### 8. 写 router + compatibility 测试 (`tests/router.rs`)

- 路由 determinism: 两个 provider 同 priority → stable model string sort
- 路由 priority: 低 priority 优先
- 路由 language filter: 匹配/不匹配
- no_matching_provider: 无 model + 无 route config
- invalid_request: non-empty provider_options + no model
- bare model rejection
- Strict compatibility: 每种 unsupported option 一个 test case
- Coerce compatibility: 记录 OptionAdjustment

### 9. 验证

```bash
cargo test -p agent-runtime-asr-providers
cargo clippy -p agent-runtime-asr-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- Compatibility validation 放独立 `compatibility.rs`，不塞 `routing.rs`
- Route config 解析放 `config.rs`（与 `AsrGatewayConfig` 同模块），`routing.rs` 只负责路由选择逻辑
- `toml = "0.8"` 新增依赖，workspace 当前没有
- `AsrGateway` 持有 route config 是启动时一次加载，不支持热更新（PRD 明确 v0.9.1 scope）
- Fake provider 在 002 阶段的 `start_stream()` 实现会比较粗糙，003 会扩展为完整的 streaming fake
