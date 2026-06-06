# 001 实现路线

## 要读的现有代码

- `crates/agent-runtime-providers/Cargo.toml` — 依赖风格参考（edition, lints, dev-dependencies）
- `crates/agent-runtime-providers/src/lib.rs` — `NormalizedProviderModel`, `normalize_provider_model()` 的结构，API key resolution 模式
- `crates/agent-runtime-providers/src/types.rs` — 类型 derive 风格、serde 属性惯例
- `crates/agent-runtime-aigc-providers/src/types.rs` — AIGC 类型风格参考
- `crates/agent-runtime-aigc-providers/src/gateway.rs` — gateway struct 骨架参考
- workspace `Cargo.toml` — members 列表格式

## 步骤

### 1. 创建 crate 骨架

- `crates/agent-runtime-asr-providers/Cargo.toml`：
  - edition = "2021"，与 workspace 一致
  - dependencies: `tokio` (sync), `serde` (derive), `serde_json`, `async-trait`, `thiserror`, `bytes` (serde), `uuid` (v4, serde), `tracing`, `metrics`
  - dev-dependencies: `tokio` (full)
  - 不加 `agent-runtime-model` 或任何 workspace 内部 crate
  - feature flags: `volcengine = []`, `aliyun = []`，default 包含两者
  - `[lints] workspace = true`
- 在根 `Cargo.toml` 的 `[workspace] members` 加入 `"crates/agent-runtime-asr-providers"`
- 创建所有 `src/*.rs` 和 `src/providers/*.rs` 为空/minimal module 文件
- `src/lib.rs` 只声明 pub mod 和 re-export
- 运行 `cargo check -p agent-runtime-asr-providers` 确认空 crate 编译

### 2. 定义基础类型 (`types.rs`)

从 PRD Key Types 逐个定义，按依赖顺序：

1. **Primitive types** — `Language(pub String)`, `NetworkRegion(pub String)`, `AudioFormat` enum (`#[non_exhaustive]`, variants: Pcm, Wav, Opus, Mp3, Ogg, Flac)
2. **Audio types** — `AudioInput`, `StreamingAudioFormat`, `AudioChunk`, `AudioChunkBoundary`, `AudioTimelineMode`
3. **Endpointing types** — `EndpointingMode`, `EndpointingOptions`
4. **Options/Request types** — `FinalResultScope`, `TranscribeOptions`, `CompatibilityPolicy`, `OptionAdjustment`, `TranscribeRequest`, `StreamingTranscribeRequest`
5. **Result types** — `WordTimestamp`, `SpeakerSegment`, `AsrUsage`, `AsrTelemetry` (struct shell), `TranscribeResult`, `AsrFinalReason`, `AsrFinalOutput`
6. **Stream event types** — `TranscriptStability`, `TranscriptUpdateKind`, `AsrStreamEvent`
7. **Capability types** — `SampleRateSupport`, `ChannelSupport`, `AudioInputCapability`, `CapabilitySource`, `ConnectionReuse`, `AsrModelCapabilities`
8. **Routing types** — `AsrRoute`, `AsrGatewayConfig`, `AsrRouter` (struct shell), `AsrGateway` (struct shell)

Derive 惯例：
- 数据 struct: `Debug, Clone, Serialize, Deserialize`
- enum: 加 `PartialEq, Eq`
- 含 `serde_json::Value` 的 struct 只 `PartialEq` 不 `Eq`
- `AsrModelCapabilities` 含 `Value` 字段 → 不 derive `Eq`

### 3. 定义 error 类型 (`error.rs`)

- `AsrErrorCode` enum — 16 个 variants，derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`
- `AsrError` struct — 8 字段，derive `Debug, Clone, Serialize, Deserialize`
- impl `std::fmt::Display` 和 `std::error::Error`
- 便捷构造方法: `AsrError::new(code, message)`, `AsrError::unsupported_operation()`

### 4. 定义 stream 类型 (`streaming.rs`)

- `AsrAudioSink` — 持有 `mpsc::Sender<AudioChunk>`，methods 签名 only（body 留 002/003）
- `AsrEventStream` — 持有 `mpsc::Receiver<AsrStreamEvent>`，methods 签名 only
- `AsrStream` — 持有 `AsrAudioSink` + `AsrEventStream`，`split(self)` / `flush_and_wait_final(&mut self)` / `end_and_wait_final(&mut self)` 签名 only
- 这一步只定义 struct + method 签名 + `todo!()` body，真正的实现是 003

### 5. 定义 trait (`traits.rs`)

- `AsrProvider` trait with `#[async_trait]`
- 4 info methods + `transcribe()` + `start_stream()`

### 6. 定义 config (`config.rs`)

- `AsrProviderRuntimeConfig` struct
- `NormalizedAsrProviderModel<'a>` struct
- `normalize_asr_provider_model()` — 参照 `agent-runtime-providers` 的 `normalize_provider_model()` 但**不 fallback bare model**：无 `/` → 返回 `AsrError { code: InvalidRequest }`
- `create_asr_provider_from_config()` 签名 + `todo!()`

### 7. 定义 observability (`observability.rs`)

- `AsrTelemetry` struct 完整字段定义（填充逻辑留 004）

### 8. 定义 routing (`routing.rs`)

- `AsrGateway`, `AsrRouter` struct fields
- `AsrGateway::transcribe()` / `start_stream()` 签名 + `todo!()`

### 9. Provider stubs (`providers/`)

- `providers/mod.rs` — conditional `#[cfg(feature = "volcengine")] pub mod volcengine;` + `#[cfg(feature = "aliyun")] pub mod aliyun;`
- `providers/volcengine.rs` — empty struct `VolcengineAsrAdapter` + `AsrProvider` impl with `todo!()`
- `providers/aliyun.rs` — empty struct `AliyunAsrAdapter` + `AsrProvider` impl with `todo!()`

### 10. 验证

```bash
cargo check -p agent-runtime-asr-providers
cargo check -p agent-runtime-asr-providers --no-default-features
cargo clippy -p agent-runtime-asr-providers -- -D warnings
cargo fmt --check
```

确认无 workspace 内部依赖 (`grep "agent-runtime" crates/agent-runtime-asr-providers/Cargo.toml` 应为空)。

## 关键决策

- `Language` / `NetworkRegion` 用 newtype String 而非 enum：BCP-47 tag 和 region identifier 都是开放集合，enum 会频繁增加 variants
- `AudioFormat` 用 `#[non_exhaustive]` enum：首期 6 个 variant 够用，后续加 variant 不破 semver
- `AsrStream` 的 `flush_and_wait_final(&mut self)` 需要同时访问 sink 和 events — 内部实现可能需要 `AsrAudioSink` 和 `AsrEventStream` 各自通过 `Arc`/clone 方式共享，或者 `AsrStream` 持有原始 channel 而非组合 `AsrAudioSink`/`AsrEventStream`。001 只定义签名 + `todo!()`，具体内部设计留 003
- channel capacity 选择留 003，001 的 `AsrAudioSink::new()` 可以暂用 `mpsc::channel(64)` 作为占位
