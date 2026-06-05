# 001 · Crate scaffold + public types

GitHub: [#109](https://github.com/jianantian/orchest/issues/109)

## 背景

v0.9.1 ASR Provider Gateway 的基础 issue。建立 crate 结构，定义所有下游 issue 依赖的公共类型。

## 契约

### 输入

- PRD: `docs/iteration/v0_9_1/prd.md`
- 无 workspace 内部依赖

### 输出

- `crates/agent-runtime-asr-providers` 加入 workspace
- 模块布局: `lib.rs`, `traits.rs`, `types.rs`, `streaming.rs`, `routing.rs`, `observability.rs`, `error.rs`, `config.rs`, `providers/{mod,volcengine,aliyun}.rs`

## 影响范围

### 新增文件

- `crates/agent-runtime-asr-providers/Cargo.toml`
- `crates/agent-runtime-asr-providers/src/lib.rs`
- `crates/agent-runtime-asr-providers/src/traits.rs` — `AsrProvider` trait
- `crates/agent-runtime-asr-providers/src/types.rs` — request/result/audio/stream/capability types
- `crates/agent-runtime-asr-providers/src/streaming.rs` — `AsrStream`, `AsrAudioSink`, `AsrEventStream`
- `crates/agent-runtime-asr-providers/src/routing.rs` — `AsrRouter`, `AsrRoute`, `AsrGateway` (structs only)
- `crates/agent-runtime-asr-providers/src/observability.rs` — `AsrTelemetry`
- `crates/agent-runtime-asr-providers/src/error.rs` — `AsrError`, `AsrErrorCode`
- `crates/agent-runtime-asr-providers/src/config.rs` — `AsrProviderRuntimeConfig`, `NormalizedAsrProviderModel`, `normalize_asr_provider_model()`
- `crates/agent-runtime-asr-providers/src/providers/mod.rs`
- `crates/agent-runtime-asr-providers/src/providers/volcengine.rs` (stub)
- `crates/agent-runtime-asr-providers/src/providers/aliyun.rs` (stub)

### 修改文件

- workspace `Cargo.toml` — 新增 member

## 类型清单

### Core Trait

- `AsrProvider` trait: `provider_name()`, `model_name()`, `capabilities()`, `supported_languages()`, `transcribe()` (reserved, returns `UnsupportedOperation`), `start_stream()`

### Request Types

- `TranscribeRequest` — `model: Option<String>`, `audio: AudioInput`, `options: TranscribeOptions`, `compatibility: CompatibilityPolicy`, `provider_options: Value`
- `StreamingTranscribeRequest` — `model: Option<String>`, `format: StreamingAudioFormat`, `timeline: AudioTimelineMode`, `options: TranscribeOptions`, `compatibility: CompatibilityPolicy`, `provider_options: Value`
- `TranscribeOptions` — `language`, `hot_words`, `context_prompt`, `code_switching`, `punctuate`, `interim_results`, `endpointing`, `word_timestamps`, `speaker_diarization`, `final_result_scope`, `flush_timeout`, `trace_id`
- `EndpointingOptions` — `mode: EndpointingMode`, `silence_timeout: Option<Duration>`
- `EndpointingMode` — `ProviderDefault`, `AcousticSilence`, `Semantic`, `NaturalSegmenting`, `ProviderDisabled`
- `AudioTimelineMode` — `ContinuousRealtime`, `SparseSpeechOnly`
- `FinalResultScope` — `Segment`, `Stream`

### Primitive Types

PRD 使用但未定义的基础类型，需在 001 中确定表示方式：

- `Language` — BCP-47 language tag (e.g. `"zh-CN"`, `"en"`)。建议定义为 newtype `pub struct Language(pub String)` 以保持类型安全，同时允许任意 BCP-47 值。
- `NetworkRegion` — 网络区域标识 (e.g. `"cn"`, `"us"`)。建议同样 newtype `pub struct NetworkRegion(pub String)`。
- `AudioFormat` — 音频编码格式 enum。首期至少需要: `Pcm`, `Wav`, `Opus`, `Mp3`, `Ogg`, `Flac`。按 PRD 的 `#[non_exhaustive]` 风格预留扩展。

### Audio Types

- `AudioInput` — `Bytes`, `File`, `Url`
- `StreamingAudioFormat` — `Pcm16 { sample_rate_hz, channels }`, `Encoded { format }`
- `AudioChunk` — `data: Bytes`, `timestamp_ms: Option<u64>`, `boundary: AudioChunkBoundary`
- `AudioChunkBoundary` — `None`, `Flush`, `End`

### Result Types

- `TranscribeResult` — `text`, `language`, `confidence`, `words`, `speakers`, `audio_duration_ms`, `processing_latency_ms`, `usage`, `option_adjustments`, `telemetry`
- `AsrFinalOutput` — `trace_id`, `segment_id`, `reason: AsrFinalReason`, `result: TranscribeResult`
- `AsrFinalReason` — `CallerFlush`, `CallerEnd`, `ProviderEndpoint`, `Timeout`
- `AsrUsage` — `audio_duration_ms`, `billable_duration_ms`, `input_bytes`, `transcript_chars`, `cost_estimate_micros`
- `WordTimestamp`, `SpeakerSegment`

### Stream Types

- `AsrStream` — `input: AsrAudioSink`, `events: AsrEventStream`; methods: `split(self)`, `flush_and_wait_final()`, `end_and_wait_final()`
- `AsrAudioSink` — `send_audio()`, `flush_segment()`, `end_stream()`
- `AsrEventStream` — `next()`, `next_final()`
- `AsrStreamEvent` — `RouteSelected`, `Started`, `TranscriptUpdate`, `EndOfSpeech`, `AsrFinal`, `Error`
- `TranscriptStability` — `Provisional`, `Committed`
- `TranscriptUpdateKind` — `Snapshot`, `Append`

### Capability Types

- `AsrModelCapabilities` — `model`, `languages`, `streaming`, `batch`, `streaming_inputs`, `batch_inputs`, `audio_timeline_modes`, `interim_results`, `endpointing_modes`, `segment_flush`, `multi_segment_streaming`, `connection_reuse`, `word_timestamps`, `speaker_diarization`, `confidence`, `code_switching`, `hot_words`, `context_prompt`, `provider_option_keys`, `max_duration_ms`, `default_flush_timeout_ms`, `source`, `diagnostic_metadata`
- `AudioInputCapability` — `format`, `sample_rates_hz`, `channels`, `max_duration_ms`, `max_bytes`
- `SampleRateSupport` — `Any`, `Exact(Vec<u32>)`, `Range { min, max }`
- `ChannelSupport` — `Any`, `Exact(Vec<u16>)`
- `ConnectionReuse` — `NotReusable`, `ReusableAfterTerminalFinal`, `ReusableAfterProviderTaskFinished`
- `CapabilitySource`

### Compatibility Types

- `CompatibilityPolicy` — `Coerce`, `Strict`
- `OptionAdjustment` — `option`, `requested`, `applied`, `reason`

### Error Types

- `AsrError` — `message`, `code`, `model`, `status`, `upstream_code`, `upstream_message`, `upstream_body`, `diagnostic_metadata`
- `AsrErrorCode` — `MissingApiKey`, `InvalidApiKey`, `UnknownProvider`, `UnknownModel`, `NoMatchingProvider`, `UnsupportedOperation`, `UnsupportedOption`, `UnsupportedLanguage`, `UnsupportedAudioFormat`, `InvalidAudio`, `InvalidRequest`, `ProviderHttpError`, `ProviderStreamError`, `ProviderTaskFailed`, `Timeout`, `Cancelled`

### Config Types

- `AsrProviderRuntimeConfig` — `model`, `api_key`, `api_key_env`, `api_url`, `region`, `timeout`, `provider_options`
- `NormalizedAsrProviderModel<'a>` — `provider: &'a str`, `model: &'a str`
- `normalize_asr_provider_model()` — rejects bare model strings without `"provider/"` prefix

### Observability Types

- `AsrTelemetry` — `trace_id`, `model`, `language`, `audio_duration_ms`, `latency_first_update_ms`, `latency_final_ms`, `update_rollback_count`, `confidence_avg`, `cost_estimate_micros`, `network_region`, `upstream_status`, `option_adjustment_count`

## 约束

- `anyhow` 不允许使用
- `AsrStream` 的 `split(self)` 和 owned convenience methods 互斥，类型签名需要体现
- `EndpointingMode::ProviderDefault` 隐式可用，不需要出现在 `AsrModelCapabilities.endpointing_modes`
- v0.9.1 adapters 可以从 `transcribe()` 返回 `UnsupportedOperation`
- `TranscribeOptions` 保持 provider-neutral；provider-native 参数放 `provider_options`

## 验收标准

- [ ] `crates/agent-runtime-asr-providers` exists and is part of the workspace
- [ ] Crate has zero workspace-internal dependencies
- [ ] `AsrProvider` trait supports streaming transcription and reserves a one-shot `transcribe()` signature
- [ ] v0.9.1 adapters may return `AsrErrorCode::UnsupportedOperation` from `transcribe()`
- [ ] Provider-neutral types exist for audio input, options, result, stream handle, stream event, usage, telemetry, routing, compatibility and errors
- [ ] `TranscribeRequest` and `StreamingTranscribeRequest` expose `model: Option<String>` and `provider_options: Value`
- [ ] `TranscribeOptions` remains provider-neutral
- [ ] `AsrModelCapabilities` exists with `streaming_inputs`, `batch_inputs`, `audio_timeline_modes`, `endpointing_modes`, `segment_flush`, `multi_segment_streaming`, `connection_reuse`, `default_flush_timeout_ms`
- [ ] `AsrErrorCode` covers all specified variants including `Cancelled`
- [ ] `normalize_asr_provider_model()` rejects bare model strings without provider prefix
- [ ] No changes required in `agent-runtime-core`
- [ ] `cargo check -p agent-runtime-asr-providers` passes
- [ ] `cargo clippy -p agent-runtime-asr-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## 依赖

无 — 基础 issue。
