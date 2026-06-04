# v0.9.3 Spec：TTS Provider Gateway

## 背景

v0.9.1 规划了 `agent-runtime-asr-providers`，负责语音输入侧的 STT/ASR provider 统一封装。语音输出侧需要同级别的 TTS provider gateway，但 TTS 也不应进入 `agent-runtime-core`。

TTS 在实时 voice agent 中通常是渲染/输出层能力：agent 产出文本或未来的 `Spoken` block，Showroom / 宿主应用选择 TTS provider 合成语音。Orchest runtime 不应该管理扬声器、音频播放、WebRTC、SIP、barge-in 或 voice session 生命周期。

本卫星迭代新增 `agent-runtime-tts-providers` crate，提供文本到语音的 provider abstraction、voice catalog、streaming synthesis、audio format normalization、usage、telemetry 和 error contract。第一期实现阿里云和火山引擎两个国内 TTS provider，用它们的 HTTP / WebSocket / 单流式 / 双流式差异校准公共抽象。

参考研究：

- [`docs/research/audio-native-agent-full-architecture.md`](../../research/audio-native-agent-full-architecture.md)
- [`docs/research/audio-native-agent-products-landscape.md`](../../research/audio-native-agent-products-landscape.md)
- [`docs/research/output-model-genui.md`](../../research/output-model-genui.md)

本地供应商资料：

- [`docs/external/aliyun/tts-api-doc.md`](../../external/aliyun/tts-api-doc.md)
- [`docs/external/aliyun/tts-guideline.md`](../../external/aliyun/tts-guideline.md)
- [`docs/external/volceengine/tts.md`](../../external/volceengine/tts.md)

## 目标

1. 新增 `crates/agent-runtime-tts-providers` 卫星 crate
2. 定义 `TtsProvider` trait，统一 batch synthesis、单流式 synthesis 和双流式 synthesis
3. 定义 provider-neutral TTS 类型：text input、voice selection、speech controls、audio format、stream handle、stream event、usage、telemetry、error
4. 提供 `TtsGateway` / `TtsRouter`，支持按语言、voice capability、格式、延迟和成本策略选择 provider/model
5. 支持 voice catalog 查询，区分 system voice、custom cloned voice、designed voice
6. 实现阿里云和火山引擎两个 provider adapter，live tests 通过 env var gated
7. 保持 core runtime 不变：TTS crate 不依赖 `agent-runtime-core`

## 统一原则

- **Capability first**：调用方表达的是合成意图（文本、音色、语言、语速、情绪、输出格式、是否流式），不是某个 provider 的 endpoint 形状。
- **Standalone crate**：`agent-runtime-tts-providers` 不依赖 `agent-runtime-core` 或绑定 crate。它必须可作为独立 TTS library 使用。
- **Two streaming protocols are first-class**：TTS 首要支持两种协议形态：单流式（完整/已提交文本输入，文本事件 + 音频事件流式输出）和双流式（文本 chunk 流入，文本事件 + 音频事件流式输出）。公共 API 不应把单流式 provider 硬塞进双流式形状。
- **No silent semantic loss**：provider 无法满足请求字段时，按 compatibility policy 返回 `unsupported_option` 或记录 `OptionAdjustment`。
- **Voice is a first-class capability**：voice id 不是裸字符串。公共类型必须表达 voice 来源、支持语言、是否自定义、是否支持 instruction / emotion / cloning / design。
- **Provider-specific power remains available**：通用稳定字段进入 typed config；少数 provider 特性通过 `provider_options` 透传。
- **No playback concerns**：crate 只生成音频 bytes/chunks 和 metadata，不播放音频、不管理设备、不做会话编排。
- **Core stays text-in/text-out**：TTS 用于输出渲染，不是 agent loop 的一部分。

## 范围

### Crate 结构

```text
crates/agent-runtime-tts-providers/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── traits.rs
│   ├── types.rs
│   ├── streaming.rs
│   ├── voices.rs
│   ├── routing.rs
│   ├── observability.rs
│   ├── error.rs
│   ├── config.rs
│   └── providers/
│       ├── mod.rs
│       ├── volcengine.rs
│       └── aliyun.rs
└── tests/
    ├── fake_provider.rs
    ├── router.rs
    └── streaming.rs
```

Both provider modules must implement the public TTS contract. Live credential tests are ignored by default and gated by env vars, but fake-provider tests must exercise routing, compatibility, streaming, voice catalog, and telemetry behavior in normal CI.

### Dependency Direction

```text
agent-runtime-tts-providers/  <-- standalone, zero workspace-internal dependencies
  owns: TtsProvider trait + gateway + public TTS types + adapters

agent-runtime-core/           <-- no dependency added in v0.9.3
  owns: run loop, tools, skills, MCP, budget, events
```

`agent-runtime-tts-providers` must not import from `agent-runtime-core`, `agent-runtime-py`, or `agent-runtime-node`. Future output-model integration or tool wrappers belong in core, Showroom, or a separate extension crate.

### Core Trait

```rust
#[async_trait]
pub trait TtsProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capabilities(&self) -> TtsModelCapabilities;

    async fn synthesize(
        &self,
        request: SynthesizeRequest,
    ) -> Result<SynthesizeResult, TtsError>;

    async fn stream_synthesize(
        &self,
        request: StreamSynthesizeRequest,
    ) -> Result<TtsOutputStream, TtsError>;

    async fn start_duplex_stream(
        &self,
        request: DuplexSynthesizeRequest,
    ) -> Result<TtsDuplexStream, TtsError>;

    async fn list_voices(
        &self,
        request: ListVoicesRequest,
    ) -> Result<Vec<VoiceInfo>, TtsError>;
}
```

`list_voices()` may return a static catalog for providers whose voice lists are documented but not queryable. The returned `VoiceInfo.source` must identify whether the catalog came from provider metadata, static tables, or caller-provided config.

### Key Types

```rust
pub struct SynthesizeRequest {
    pub input: TtsInput,
    pub voice: VoiceSelection,
    pub output: AudioOutputConfig,
    pub controls: SpeechControls,
    pub compatibility: CompatibilityPolicy,
    pub trace_id: Option<String>,
    pub provider_options: Value,
}

pub struct StreamSynthesizeRequest {
    pub input: TtsInput,
    pub voice: VoiceSelection,
    pub output: AudioOutputConfig,
    pub controls: SpeechControls,
    pub compatibility: CompatibilityPolicy,
    pub trace_id: Option<String>,
    pub provider_options: Value,
}

pub struct DuplexSynthesizeRequest {
    pub voice: VoiceSelection,
    pub output: AudioOutputConfig,
    pub controls: SpeechControls,
    pub compatibility: CompatibilityPolicy,
    pub trace_id: Option<String>,
    pub provider_options: Value,
}

pub enum TtsInput {
    Text(String),
    Ssml(String),
}

pub struct TextChunk {
    pub text: String,
    pub is_final: bool,
}

pub struct SynthesizeResult {
    pub audio: AudioData,
    pub format: AudioFormat,
    pub duration_ms: Option<u64>,
    pub usage: TtsUsage,
    pub option_adjustments: Vec<OptionAdjustment>,
    pub provider_metadata: Value,
    pub telemetry: TtsTelemetry,
}

pub enum AudioData {
    Bytes(Bytes),
    Url { url: String, expires_at: Option<DateTime<Utc>> },
}

pub struct TtsUsage {
    pub input_chars: u64,
    pub billable_chars: Option<u64>,
    pub audio_duration_ms: Option<u64>,
    pub output_bytes: Option<u64>,
    pub cost_estimate_micros: Option<u64>,
}
```

The gateway does not persist audio assets in v0.9.3. If a provider returns a temporary URL, it is returned as an ingestion source with expiration metadata. Asset storage and public URL signing belong to a later media-common layer or the host application.

### Streaming Contract

```rust
/// 单流式：request already contains the complete or committed text input.
/// The stream emits text/audio output events.
pub struct TtsOutputStream {
    pub events: mpsc::Receiver<TtsStreamEvent>,
}

/// 双流式：caller streams text chunks in and receives text/audio output events.
pub struct TtsDuplexStream {
    pub input: mpsc::Sender<TextChunk>,
    pub events: mpsc::Receiver<TtsStreamEvent>,
}

pub enum TtsStreamEvent {
    RouteSelected { trace_id: String, provider: String, model: String },
    Started { trace_id: String, provider: String, model: String, voice: VoiceInfo },
    /// Text that the gateway/provider has accepted for synthesis and may show
    /// in the UI alongside audio. In single-stream mode this is usually an echo
    /// of the committed input. In duplex mode this follows accepted chunks.
    TextDelta { trace_id: String, text: String, sequence: u64, is_final: bool },
    TextAccepted { trace_id: String, chars: u64 },
    AudioChunk { trace_id: String, data: Bytes, format: AudioFormat, sequence: u64 },
    Completed { trace_id: String, result: SynthesizeResult },
    Error { trace_id: String, error: TtsError, fatal: bool },
}
```

Single-stream and duplex-stream are separate public protocols:

- **Single-stream** (`stream_synthesize`) is for providers that accept a complete text request and stream output. The output stream must include text events and audio chunks so a host app can render synchronized text + speech progress.
- **Duplex-stream** (`start_duplex_stream`) is for providers that accept text chunks over time while streaming audio back. This is the path for feeding LLM text deltas directly into TTS.

For providers that do not support duplex streaming, strict mode must return `unsupported_operation` for `start_duplex_stream`. Coerce mode may degrade duplex to single-stream only after the caller has sent a final text chunk and the gateway records an `OptionAdjustment`; it must not pretend incremental text was synthesized incrementally.

### Voice and Controls

```rust
pub struct VoiceSelection {
    pub voice_id: String,
    pub language: Option<Language>,
    pub gender: Option<VoiceGender>,
    pub provider_options: Value,
}

pub struct VoiceInfo {
    pub voice_id: String,
    pub display_name: Option<String>,
    pub provider: String,
    pub model: String,
    pub languages: Vec<Language>,
    pub kind: VoiceKind,
    pub gender: Option<VoiceGender>,
    pub supports_instruction: bool,
    pub supports_emotion: bool,
    pub supports_ssml: bool,
    pub source: CapabilitySource,
    pub provider_metadata: Value,
}

pub enum VoiceKind {
    System,
    Cloned,
    Designed,
    Custom,
}

pub struct SpeechControls {
    pub speed: Option<f32>,
    pub pitch: Option<f32>,
    pub volume: Option<f32>,
    pub emotion: Option<String>,
    pub style: Option<String>,
    pub instruction: Option<String>,
}
```

Voice cloning and voice design management are out of scope for v0.9.3. This crate may synthesize with an existing custom voice id, but it does not create, enroll, delete, or govern custom voices.

### Audio Output Contract

```rust
pub struct AudioOutputConfig {
    pub format: AudioFormat,
    pub sample_rate_hz: Option<u32>,
    pub bitrate: Option<u32>,
    pub channels: Option<u16>,
}

pub enum AudioFormat {
    Pcm16,
    Wav,
    Mp3,
    Opus,
}
```

The gateway does not perform general-purpose transcoding in v0.9.3. If a provider cannot produce the requested format, strict mode returns `unsupported_audio_format`; coerce mode may select another supported format and record an `OptionAdjustment` only when application policy allows it.

### Gateway and Routing

```rust
pub struct TtsGateway {
    router: TtsRouter,
    config: TtsGatewayConfig,
}

pub struct TtsRouter {
    /// Registered provider instances keyed by normalized "provider/model".
    providers: HashMap<String, Arc<dyn TtsProvider>>,
    routes: Vec<TtsRoute>,
}

pub struct TtsRoute {
    pub languages: Vec<Language>,
    pub voice_kinds: Vec<VoiceKind>,
    pub output_formats: Vec<AudioFormat>,
    pub max_first_audio_latency_ms: Option<u64>,
    pub max_cost_micros_per_1k_chars: Option<u64>,
    pub priority: u8,
    /// Provider/model selector using the same "provider/model" convention as
    /// agent-runtime-providers, for example "volcengine/seed-tts-2.0",
    /// "aliyun/cosyvoice-v3-flash", or "aliyun/qwen3-tts-flash-realtime".
    pub model: String,
}
```

Provider/model naming follows `agent-runtime-providers`: a model string with an explicit prefix is interpreted as `"provider/model"`; the prefix selects the factory, and the suffix is passed to the provider adapter as the provider-native model or service identifier. TTS does not introduce separate request-level `provider` and `model` fields.

Gateway helpers provide the public convenience surface:

```rust
impl TtsGateway {
    pub async fn synthesize(
        &self,
        request: SynthesizeRequest,
    ) -> Result<SynthesizeResult, TtsError>;

    pub async fn stream_synthesize(
        &self,
        request: StreamSynthesizeRequest,
    ) -> Result<TtsOutputStream, TtsError>;

    pub async fn start_duplex_stream(
        &self,
        request: DuplexSynthesizeRequest,
    ) -> Result<TtsDuplexStream, TtsError>;
}
```

If `trace_id` is `None`, the gateway generates one and includes it in `RouteSelected`, `Started`, every audio event, `SynthesizeResult.telemetry`, and all emitted telemetry. Provider adapters should not generate unrelated trace IDs.

The router must be deterministic:

1. Normalize each route's `model` string with `normalize_tts_provider_model()`
2. Filter by capability compatibility
3. Filter by voice/language/output format constraints
4. Apply latency/cost constraints when configured
5. Select lowest `priority`, then stable normalized provider/model string sort as tie-breaker

If no provider matches, return `no_matching_provider` with the rejected constraints included in `provider_metadata`.

### Provider-Specific Protocol Notes

These notes are implementation constraints from the local vendor docs, not extra public API surface:

- **Aliyun CosyVoice** supports WebSocket and HTTP access depending on model/version. v0.9.3 should target a realtime-capable CosyVoice model first and leave room for non-realtime HTTP synthesis.
- **Aliyun Qwen3-TTS** distinguishes HTTP and realtime models by model name. Instruct models support natural-language `instruction`; non-instruct models must reject `SpeechControls.instruction` in strict mode.
- **Aliyun custom voices** include voice cloning and voice design flows. v0.9.3 may synthesize with an existing voice id but does not implement voice enrollment/design management APIs.
- **Volcengine TTS V3** exposes bidirectional WebSocket, unidirectional WebSocket, HTTP chunked, and SSE modes. v0.9.3 should treat bidirectional WebSocket and unidirectional streaming as separate first-class mappings, not as one adapter mode pretending to be the other.
- **Volcengine connections** can support multiple sessions on one WebSocket connection but not concurrent sessions on the same connection. Provider adapters must not expose connection reuse policy as public API.
- **Usage return** is provider-specific. Volcengine exposes usage-token return controls; Aliyun exposes request id / first package delay through SDK-style APIs. Adapters should normalize usage where available and preserve provider details in `provider_metadata`.
- **Text chunking** affects prosody. The gateway should not force sentence splitting; callers may stream LLM text directly. Provider adapters may buffer only when the selected provider protocol requires it and must report first-audio latency.

### Runtime Config and Factory

```rust
pub struct TtsProviderRuntimeConfig {
    /// Provider/model selector using the same "provider/model" convention as
    /// agent-runtime-providers.
    pub model: String,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub api_url: Option<String>,
    pub region: Option<String>,
    pub timeout: Option<Duration>,
    pub provider_options: Value,
}

pub fn create_tts_provider_from_config(
    config: TtsProviderRuntimeConfig,
) -> Result<Arc<dyn TtsProvider>, TtsError>;

pub struct NormalizedTtsProviderModel<'a> {
    pub provider: &'a str,
    pub model: &'a str,
}

pub fn normalize_tts_provider_model(model: &str) -> Result<NormalizedTtsProviderModel<'_>, TtsError>;
```

API key resolution follows the same hierarchy as other provider crates:

1. explicit `api_key`
2. explicit local `api_key_env`
3. provider default environment variable

If `api_key_env` is configured, a missing or empty value is an error and must not fall back to the provider default.

### Compatibility Policy

```rust
pub enum CompatibilityPolicy {
    Coerce,
    Strict,
}

pub struct OptionAdjustment {
    pub option: String,
    pub requested: Value,
    pub applied: Value,
    pub reason: String,
}
```

`Strict` returns errors for unsupported or ambiguous options.

`Coerce` prefers a working request and records changes. Examples:

- Requested `instruction` on a non-instruct model -> strict returns `unsupported_option`; coerce drops it only if policy explicitly allows expression loss.
- Requested `Ssml` on a provider/model without SSML support -> strict returns `unsupported_input`; coerce may convert to plain text only if tags can be safely stripped and adjustment is recorded.
- Requested `AudioFormat::Opus` but selected provider supports only MP3/WAV -> strict returns `unsupported_audio_format`; coerce may select MP3 and record adjustment.
- Requested custom voice kind but route selects a system-only model -> strict returns `unsupported_voice`.

### Capabilities

```rust
pub struct TtsModelCapabilities {
    pub languages: Vec<Language>,
    pub input_kinds: Vec<TtsInputKind>,
    pub output_formats: Vec<AudioFormat>,
    pub stream_output: bool,
    pub batch: bool,
    pub duplex_streaming: bool,
    pub text_delta_output: bool,
    pub voice_kinds: Vec<VoiceKind>,
    pub supports_instruction: bool,
    pub supports_emotion: bool,
    pub supports_ssml: bool,
    pub supports_speed: bool,
    pub supports_pitch: bool,
    pub supports_volume: bool,
    pub max_input_chars: Option<u64>,
    pub source: CapabilitySource,
    pub provider_metadata: Value,
}
```

Capabilities may come from provider metadata, static tables, or conservative assumptions. Strict mode must not rely on assumptions for fields that affect audio semantics.

### Observability

```rust
pub struct TtsTelemetry {
    pub trace_id: String,
    pub provider: String,
    pub model: String,
    pub voice_id: String,
    pub input_chars: u64,
    pub latency_first_audio_ms: Option<u64>,
    pub latency_final_ms: u64,
    pub audio_duration_ms: Option<u64>,
    pub output_bytes: Option<u64>,
    pub cost_estimate_micros: Option<u64>,
    pub provider_status: Option<u16>,
    pub option_adjustment_count: u32,
}
```

The TTS crate emits lifecycle and audio events through `TtsStreamEvent` for streaming calls, and records spans / metrics for both batch and streaming calls. It must not require adding TTS-specific variants to core `RuntimeEvent`. Applications can align ASR -> Orchest -> TTS timelines through `trace_id`.

Spans:

- `tts.gateway.synthesize`
- `tts.gateway.stream`
- `tts.provider.request`
- `tts.provider.stream`
- `tts.router.select`
- `tts.voices.list`

Metrics:

- request duration
- first audio latency
- final synthesis latency
- input chars
- output bytes
- audio duration
- provider error count by code/status
- option adjustment count

Do not log raw audio bytes, API keys, full synthesized text, or temporary provider URLs by default. Text/audio logging is application policy.

### Error Handling

```rust
pub struct TtsError {
    pub message: String,
    pub code: TtsErrorCode,
    pub provider: Option<String>,
    pub status: Option<u16>,
    pub upstream_code: Option<String>,
    pub upstream_message: Option<String>,
    pub upstream_body: Option<Value>,
}

pub enum TtsErrorCode {
    MissingApiKey,
    InvalidApiKey,
    UnknownProvider,
    UnknownModel,
    NoMatchingProvider,
    UnsupportedOperation,
    UnsupportedOption,
    UnsupportedInput,
    UnsupportedVoice,
    UnsupportedLanguage,
    UnsupportedAudioFormat,
    InvalidText,
    InvalidVoice,
    InvalidRequest,
    ProviderHttpError,
    ProviderStreamError,
    ProviderTaskFailed,
    Timeout,
    Cancelled,
}
```

Provider response bodies should be preserved in errors for debugging, with secrets redacted.

### Provider Scope

Initial provider targets:

| Provider | Priority | Notes |
|----------|----------|-------|
| Volcengine | P0 | Domestic realtime TTS baseline; first target model/resource `volcengine/seed-tts-2.0` |
| Aliyun | P0 | Domestic realtime TTS comparison point; first target model `aliyun/cosyvoice-v3-flash` or `aliyun/qwen3-tts-flash-realtime` |

Reference-only providers:

| Provider | Status | Why |
|----------|--------|-----|
| ElevenLabs | Not implemented in v0.9.3 | Interface calibration for voice cloning, emotion/style controls, and global commercial TTS UX |
| Cartesia | Not implemented in v0.9.3 | Interface calibration for low first-audio latency and streaming-first TTS |
| OpenAI TTS | Not implemented in v0.9.3 | Useful later for broad model-provider ecosystem coverage |

Live provider tests should be gated behind env vars and ignored by default. Fake-provider tests must run in normal CI.

### Feature Flags

| Feature | Contents |
|---------|----------|
| `volcengine` | Volcengine adapter and live tests |
| `aliyun` | Aliyun adapter and live tests |

The default feature set should include fake/test-safe infrastructure and the two first-party provider adapters unless a provider requires a heavy optional dependency. WebSocket dependencies for streaming providers should be feature-gated if they are not already required by the default adapters.

## Not in Scope

- Changes to `agent-runtime-core` run loop
- Adding `Spoken`, `Audio`, or TTS-specific events to core `RuntimeEvent`
- Showroom renderer implementation
- Audio playback, device selection, gain control, jitter buffer, WebRTC, SIP, Twilio, telephony
- Continuous voice session orchestration, barge-in, interruption handling
- ASR provider gateway
- Music generation (`fun-music-*`), sound effects, voice conversion as a standalone product
- Voice cloning enrollment, voice design creation, custom voice governance
- Treating output rendering TTS as a Tool
- Treating TTS as a Skill

## Tool Adapter Boundary

v0.9.3 does not implement a `TtsTool` because this crate must remain standalone and must not depend on `agent-runtime-core`.

A future core wrapper or extension crate may expose TTS as a Tool only for agent-initiated audio artifact generation, such as "render this podcast script to an MP3 file." That wrapper must be documented as distinct from realtime spoken response rendering.

Realtime output rendering flow remains:

```text
Agent output text / Spoken block -> Showroom or host app -> TTS Gateway -> audio output
```

not:

```text
Agent output -> model calls speak tool -> audio playback
```

## Dependencies

- `async-trait`
- `serde` / `serde_json`
- `tokio`
- `thiserror`
- `bytes`
- `uuid`
- `tracing`
- `metrics`
- Existing workspace conventions from `agent-runtime-providers`, `agent-runtime-aigc-providers`, and `agent-runtime-asr-providers`

New provider-specific HTTP/WebSocket dependencies must be feature-gated and justified in the implementation PR.

`anyhow` is not allowed in this library crate. Provider adapters should return `TtsError` with stable codes and preserved upstream details.

## Testing Strategy

Unit tests:

- Fake provider implements batch, single-stream, and duplex-stream paths
- Voice catalog returns system/custom voice metadata and capability source
- Router determinism and tie-break behavior
- Compatibility policy strict/coerce behavior
- Capability validation for instruction, SSML, output format, custom voice, single-stream output and duplex streaming
- API key resolution hierarchy
- Error redaction preserves debug payload while removing secrets
- Streaming event ordering: `RouteSelected` -> `Started` -> zero or more `TextDelta` / `TextAccepted` / `AudioChunk` -> `Completed` or fatal `Error`

Integration tests:

- `cargo test -p agent-runtime-tts-providers` runs without live credentials
- Live provider tests are `#[ignore]` and gated by provider-specific env vars
- Volcengine and Aliyun live tests, when env vars are set, synthesize a tiny text fixture and assert non-empty audio bytes/chunks plus telemetry fields

Test fixtures:

- Use tiny text fixtures generated in tests.
- Do not commit provider-generated voice samples unless they are explicitly licensed and tiny enough for repo storage.

## Issue Breakdown

| Issue | Title | Depends on | Scope |
|-------|-------|------------|-------|
| 001 | Crate scaffold + public types | -- | Workspace entry, module layout, `TtsProvider`, request/result/usage/error/capability/voice types |
| 002 | Gateway + router | 001 | `TtsGateway`, deterministic `TtsRouter`, compatibility validation, fake-provider tests |
| 003 | Streaming contracts | 001, 002 | `TtsOutputStream`, `TtsDuplexStream`, stream event ordering, text delta semantics, audio chunk output, backpressure docs |
| 004 | Voice catalog + controls | 001, 002 | `VoiceInfo`, `VoiceSelection`, voice capability validation, instruction/emotion/SSML handling |
| 005 | Observability + trace | 001, 002, 003 | `TtsTelemetry`, `TtsStreamEvent` trace propagation, spans/metrics |
| 006 | Volcengine adapter | 001, 003, 004, 005 | Feature-gated provider implementation, config factory, live ignored test |
| 007 | Aliyun adapter + examples | 001, 003, 004, 005, 006 | Feature-gated provider implementation, config factory, live ignored test, README/example snippets |

## Acceptance Criteria

- [ ] `crates/agent-runtime-tts-providers` exists and is part of the workspace
- [ ] `TtsProvider` trait supports batch synthesis, single-stream synthesis, duplex streaming synthesis, and voice listing
- [ ] Crate has zero workspace-internal dependencies
- [ ] Provider/model selection uses the same `"provider/model"` convention and normalization behavior as `agent-runtime-providers`
- [ ] Provider-neutral types exist for text input, voice selection, speech controls, output format, stream handle, stream event, usage, telemetry, routing, compatibility and errors
- [ ] `TtsModelCapabilities` exists and strict compatibility checks use it
- [ ] `VoiceInfo` distinguishes system, cloned, designed and custom voices
- [ ] Instruction, SSML, speed, pitch, volume, emotion and output format controls map to provider-native fields only when supported; otherwise strict mode returns stable errors
- [ ] `TtsGateway` can route requests to registered fake providers based on route priority, language, voice kind and output format
- [ ] Router tie-break behavior is deterministic
- [ ] Public single-stream API accepts committed text and returns text/audio output events through `TtsOutputStream`
- [ ] Public duplex-stream API lets callers send `TextChunk`s and receive text/audio output events through `TtsDuplexStream`
- [ ] Streaming contract distinguishes route selection, start, text delta, accepted text, audio chunk, completion and fatal/non-fatal errors
- [ ] Strict compatibility rejects duplex streaming for provider/model combinations that only support single-stream output
- [ ] Streaming tests assert stable event ordering and no provider-internal channel drain deadlock
- [ ] Telemetry includes `trace_id` and can be correlated with external ASR and Orchest run traces
- [ ] `TtsError` preserves upstream status/code/message/body with secret redaction
- [ ] API key resolution follows explicit key -> explicit env var -> provider default env var
- [ ] Volcengine adapter is implemented behind a feature flag with fake/offline tests and ignored live tests
- [ ] Aliyun adapter is implemented behind a feature flag with fake/offline tests and ignored live tests
- [ ] Offline provider tests cover Volcengine session lifecycle and Aliyun realtime model capability differences
- [ ] Strict compatibility rejects `instruction`, `SSML`, custom voices or output formats for provider/model combinations that do not support them
- [ ] `cargo test -p agent-runtime-tts-providers --no-default-features` passes
- [ ] `cargo test -p agent-runtime-tts-providers --features volcengine` passes without live credentials
- [ ] `cargo test -p agent-runtime-tts-providers --features aliyun` passes without live credentials
- [ ] PRD documents that `TtsTool` is out of scope for this standalone crate
- [ ] No changes are required in `agent-runtime-core`
- [ ] `cargo test -p agent-runtime-tts-providers` passes
- [ ] `cargo clippy -p agent-runtime-tts-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes
