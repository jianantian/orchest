# v0.9.3 PRD：TTS Provider Gateway

## 背景

v0.9.1 规划了 `agent-runtime-asr-providers`，负责语音输入侧的 STT/ASR provider 统一封装。语音输出侧需要同级别的 TTS provider gateway，但 TTS 也不应进入 `agent-runtime-core`。

TTS 在实时 voice agent 中通常是渲染/输出层能力：agent 产出文本或未来的 `Spoken` block，Showroom / 宿主应用选择 TTS provider 合成语音。Orchest runtime 不应该管理扬声器、音频播放、WebRTC、SIP、barge-in 或 voice session 生命周期。

本卫星迭代新增 `agent-runtime-tts-providers` crate，提供文本到语音的 provider abstraction、voice catalog、streaming synthesis、audio format normalization、usage、telemetry 和 error contract。第一期实现阿里云和火山引擎两个国内 TTS provider，用它们的 HTTP / WebSocket / 单流式 / 双流式差异校准公共抽象。

参考研究：

- [`docs/research/audio-agent/audio-native-agent-full-architecture.md`](../../research/audio-agent/audio-native-agent-full-architecture.md)
- [`docs/research/audio-agent/audio-native-agent-products-landscape.md`](../../research/audio-agent/audio-native-agent-products-landscape.md)
- [`docs/research/output-model/output-model-genui.md`](../../research/output-model/output-model-genui.md)

本地供应商资料：

- [`docs/external/aliyun/tts-api-doc.md`](../../external/aliyun/tts-api-doc.md)
- [`docs/external/aliyun/tts-guideline.md`](../../external/aliyun/tts-guideline.md)
- [`docs/external/volceengine/tts.md`](../../external/volceengine/tts.md)

## 目标

1. 新增 `crates/agent-runtime-tts-providers` 卫星 crate
2. 定义 `TtsProvider` trait，统一 batch synthesis、单流式 synthesis 和双流式 synthesis
3. 定义 provider-neutral TTS 类型：provider/model selector、text input、voice selection、speech controls、audio format、stream handle、stream event、usage、telemetry、error
4. 提供 `TtsGateway` / `TtsRouter`，支持按语言、voice capability、格式、延迟和成本策略选择 provider/model
5. 支持 gateway/provider voice catalog 查询，区分 system voice、custom cloned voice、designed voice
6. 实现阿里云和火山引擎两个 provider adapter，live tests 通过 env var gated
7. 保持 core runtime 不变：TTS crate 不依赖 `agent-runtime-core`

## 统一原则

- **Capability first**：调用方表达的是合成意图（文本、音色、语言、语速、情绪、输出格式、是否流式），不是某个 provider 的 endpoint 形状。
- **Standalone crate**：`agent-runtime-tts-providers` 不依赖 `agent-runtime-core` 或绑定 crate。它必须可作为独立 TTS library 使用。
- **Two streaming protocols are first-class**：TTS 首要支持两种协议形态：单流式（完整/已提交文本输入，文本事件 + 音频事件流式输出）和双流式（文本 chunk 流入，文本事件 + 音频事件流式输出）。公共 API 不应把单流式 provider 硬塞进双流式形状，也不在 v0.9.3 做隐式 buffered downgrade。
- **No silent semantic loss**：provider 无法满足请求字段时，按 compatibility policy 返回 `unsupported_option` 或记录 `OptionAdjustment`。
- **Voice is a first-class capability**：voice id 不是裸字符串。公共类型必须表达 voice 来源、voice kind、支持语言、是否自定义、是否支持 instruction / emotion / cloning / design。
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

Provider adapters are model-specific instances. When a request with `model: Some(...)` reaches a direct provider adapter, the normalized provider/model must match that adapter's `provider_name()` and `model_name()`; otherwise the adapter returns `unknown_model` or `invalid_request` instead of silently synthesizing with a different model. The gateway is responsible for routing and for clearing or validating the selector before dispatch.

### Key Types

```rust
pub struct SynthesizeRequest {
    /// Optional explicit provider/model selector, for example
    /// "volcengine/seed-tts-2.0" or "aliyun/qwen3-tts-flash-realtime".
    /// If None, the gateway router selects a provider/model.
    pub model: Option<String>,
    pub input: TtsInput,
    pub voice: VoiceSelection,
    pub output: AudioOutputConfig,
    pub controls: SpeechControls,
    pub compatibility: CompatibilityPolicy,
    pub trace_id: Option<String>,
    pub provider_options: Value,
}

pub struct StreamSynthesizeRequest {
    /// Optional explicit provider/model selector. If present, routing is limited
    /// to this provider/model and compatibility validation still applies.
    pub model: Option<String>,
    pub input: TtsInput,
    pub voice: VoiceSelection,
    pub output: AudioOutputConfig,
    pub controls: SpeechControls,
    pub compatibility: CompatibilityPolicy,
    pub trace_id: Option<String>,
    pub provider_options: Value,
}

pub struct DuplexSynthesizeRequest {
    /// Optional explicit provider/model selector. If present, routing is limited
    /// to this provider/model and compatibility validation still applies.
    pub model: Option<String>,
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

pub enum TtsInputKind {
    Text,
    Ssml,
}

/// BCP-47 language tag, for example "zh-CN", "en-US", "ja-JP".
/// A string newtype avoids hard-coding provider language catalogs in the SDK.
pub struct Language(pub String);

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

pub struct TtsStreamSummary {
    pub format: AudioFormat,
    pub duration_ms: Option<u64>,
    pub usage: TtsUsage,
    pub option_adjustments: Vec<OptionAdjustment>,
    pub provider_metadata: Value,
    pub telemetry: TtsTelemetry,
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
    Completed { trace_id: String, summary: TtsStreamSummary },
    Error { trace_id: String, error: TtsError, fatal: bool },
}
```

Single-stream and duplex-stream are separate public protocols:

- **Single-stream** (`stream_synthesize`) is for providers that accept a complete text request and stream output. The output stream must include text events and audio chunks so a host app can render synchronized text + speech progress.
- **Duplex-stream** (`start_duplex_stream`) is for providers that accept text chunks over time while streaming audio back. This is the path for feeding LLM text deltas directly into TTS.

Gateway streams and direct provider streams have different lifecycle ownership:

- `TtsGateway` emits `RouteSelected` before dispatching to a provider and then forwards/normalizes provider lifecycle events.
- Direct `TtsProvider` streams must not emit `RouteSelected`; their first lifecycle event is `Started` unless provider setup fails before the stream is returned.
- Gateway-level tests assert `RouteSelected -> Started -> ... -> Completed/Error`; direct provider tests assert `Started -> ... -> Completed/Error`.

Stream lifecycle rules are part of the public contract:

1. `AudioChunk.sequence` and `TextDelta.sequence` are monotonically increasing per stream and per event kind, starting at `0`.
2. `Completed` and `Error { fatal: true }` are terminal events. No further events may be emitted after either terminal event.
3. `Error { fatal: false }` is only for recoverable provider notices, such as a dropped optional style hint; it must not be used for missing audio, auth failures, or stream protocol failures.
4. In duplex mode, sending a `TextChunk { is_final: true, .. }` closes the logical input segment. Additional sends after a final chunk are caller errors and may fail through the `mpsc::Sender` or produce `Error { fatal: true }`.
5. Dropping the duplex input sender before a final chunk is treated as caller cancellation and produces `Cancelled` unless the provider has already completed normally.
6. Dropping the public event receiver is caller cancellation for both single-stream and duplex-stream calls. Gateway/provider tasks should stop reading upstream audio as soon as practical and release provider sessions/connections; best-effort cancellation is sufficient, but tasks must not leak indefinitely.
7. `TtsGatewayConfig.stream_channel_capacity` controls the public event channel capacity. Providers must avoid hidden unbounded channels and must not require callers to drain a provider-internal channel before receiving the public stream handle.

For providers that do not support duplex streaming, both strict and coerce mode must return `unsupported_operation` for `start_duplex_stream` in v0.9.3. The gateway must not create a fake duplex stream that buffers chunks and later invokes single-stream synthesis. A later iteration may add an explicit buffered fallback option, but it must be opt-in, bounded by max buffered chars / timeout, and reported as an `OptionAdjustment`.

### Voice and Controls

```rust
pub struct VoiceSelection {
    pub voice_id: String,
    /// Optional caller hint. Strict mode validates this against catalog/capabilities
    /// when the selected provider exposes enough voice metadata.
    pub kind: Option<VoiceKind>,
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

pub enum VoiceGender {
    Male,
    Female,
    Neutral,
    Unknown,
}

pub struct ListVoicesRequest {
    pub model: Option<String>,
    pub language: Option<Language>,
    pub kind: Option<VoiceKind>,
    pub include_custom: bool,
    pub provider_options: Value,
}

pub enum CapabilitySource {
    ProviderMetadata,
    StaticCatalog,
    CallerConfig,
    ConservativeAssumption,
}

pub struct SpeechControls {
    /// Relative speaking rate multiplier. Valid portable range: 0.5..=2.0.
    pub speed: Option<f32>,
    /// Relative pitch shift in semitones. Valid portable range: -12.0..=12.0.
    pub pitch: Option<f32>,
    /// Relative output gain multiplier before provider encoding. Valid portable range: 0.0..=2.0.
    pub volume: Option<f32>,
    pub emotion: Option<String>,
    pub style: Option<String>,
    pub instruction: Option<String>,
}
```

The numeric `SpeechControls` ranges are the portable Orchest contract, not a claim that every provider uses the same native units. Strict mode rejects out-of-range values with `invalid_request`. Coerce mode may clamp numeric controls to the portable range and record an `OptionAdjustment`; provider-native values outside these ranges must be passed through `provider_options`, not the portable fields.

Voice cloning and voice design management are out of scope for v0.9.3. This crate may synthesize with an existing custom voice id, but it does not create, enroll, delete, or govern custom voices.

Voice kind resolution is deterministic:

1. If `VoiceSelection.kind` is set, strict mode validates it against the selected provider catalog or capabilities when available.
2. If `VoiceSelection.kind` is not set, the gateway may resolve it from `list_voices()` for the selected provider/model.
3. If the kind cannot be resolved, strict mode must not select a route that requires a specific non-system kind. Coerce mode may select a system/default route only when the caller did not request custom/cloned/designed semantics.
4. `provider_options` must not be the only place where public voice kind semantics are expressed.

### Audio Output Contract

```rust
pub struct AudioOutputConfig {
    pub format: AudioFormat,
    pub sample_rate_hz: Option<u32>,
    pub bitrate: Option<u32>,
    pub channels: Option<u16>,
}

pub enum AudioFormat {
    /// Raw signed 16-bit little-endian PCM.
    Pcm16Le,
    /// WAV container carrying signed 16-bit little-endian PCM.
    WavPcm16Le,
    Mp3,
    /// Ogg container carrying Opus frames.
    OggOpus,
}
```

The gateway does not perform general-purpose transcoding in v0.9.3. If a provider cannot produce the requested format, strict mode returns `unsupported_audio_format`; coerce mode may select another supported format and record an `OptionAdjustment` only when application policy allows it. Streaming format support must be validated separately from batch format support because some providers accept WAV for non-streaming calls but produce repeated WAV headers or provider-specific framing in streaming mode.

### Gateway and Routing

```rust
pub struct TtsGateway {
    router: TtsRouter,
    config: TtsGatewayConfig,
}

pub struct TtsGatewayConfig {
    pub compatibility: CompatibilityPolicy,
    pub stream_channel_capacity: usize,
    pub default_output: Option<AudioOutputConfig>,
    /// If false, coerce mode may only make lossless or format-level adjustments.
    /// Dropping instruction/style/emotion or stripping SSML requires this to be true.
    pub allow_semantic_coercions: bool,
}

pub struct TtsRouter {
    /// Registered provider instances keyed by normalized "provider/model".
    providers: HashMap<String, Arc<dyn TtsProvider>>,
    routes: Vec<TtsRoute>,
}

pub struct TtsRoute {
    pub languages: Vec<Language>,
    pub voice_kinds: Vec<VoiceKind>,
    /// Route-level preferred/allowed formats. The router still validates these
    /// against operation-specific `batch_output_formats` or `stream_output_formats`.
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

Provider/model naming follows `agent-runtime-providers`: a model string with an explicit prefix is interpreted as `"provider/model"`; the prefix selects the factory, and the suffix is passed to the provider adapter as the provider-native model or service identifier. TTS does not introduce separate request-level `provider` and `model` fields; instead, request-level explicit selection uses the same single `model: Option<String>` selector as route config. Unlike LLM providers, TTS has no implicit default provider for an unprefixed model string in v0.9.3: `normalize_tts_provider_model()` must reject unprefixed strings unless a future TTS default provider is explicitly documented.

`TtsGatewayConfig.compatibility` is the default used by gateway request builders and examples. The explicit `request.compatibility` field is the effective runtime policy for a request; direct provider calls must honor the request field and must not read gateway config.

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

    pub async fn list_voices(
        &self,
        request: ListVoicesRequest,
    ) -> Result<Vec<VoiceInfo>, TtsError>;
}
```

If `trace_id` is `None`, the gateway generates one and includes it in `RouteSelected`, `Started`, every audio event, batch `SynthesizeResult.telemetry`, streaming `TtsStreamSummary.telemetry`, and all emitted telemetry. Provider adapters should not generate unrelated trace IDs.

The router must be deterministic:

1. Normalize each route's `model` string with `normalize_tts_provider_model()`
2. If the request has `model: Some(...)`, restrict candidates to that normalized provider/model
3. Filter by operation capability (`batch`, `stream_output`, or `duplex_streaming`) and compatibility
4. Filter by voice/language/output format constraints; batch calls validate against `batch_output_formats`, while single-stream and duplex-stream calls validate against `stream_output_formats`
5. Apply latency/cost constraints when configured
6. Select lowest `priority`, then stable normalized provider/model string sort as tie-breaker

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

If `api_key_env` is configured, a missing or empty value is an error and must not fall back to the provider default. `api_key` represents the provider's primary credential, such as DashScope API key or Volcengine X-Api-Key. Legacy or multi-part provider credentials may be carried in typed provider-specific config structs or `provider_options`, but must still use the same redaction and validation rules.

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

`Coerce` prefers a working request and records changes. Coercion is not a license to change user-visible speech semantics silently: if a change drops text, instruction, style, emotion, voice kind, or SSML structure, it requires `TtsGatewayConfig.allow_semantic_coercions == true` and an `OptionAdjustment`. Format-only substitutions may be made in coerce mode when the selected provider can still satisfy the requested delivery path. Examples:

- Requested `instruction` on a non-instruct model -> strict returns `unsupported_option`; coerce drops it only if `TtsGatewayConfig.allow_semantic_coercions` is true.
- Requested `Ssml` on a provider/model without SSML support -> strict returns `unsupported_input`; coerce may convert to plain text only if tags can be safely stripped, `allow_semantic_coercions` is true, and adjustment is recorded.
- Requested `AudioFormat::OggOpus` but selected provider supports only MP3/WAV -> strict returns `unsupported_audio_format`; coerce may select MP3 and record adjustment.
- Requested custom voice kind but route selects a system-only model -> strict returns `unsupported_voice`.

### Capabilities

```rust
pub struct TtsModelCapabilities {
    pub languages: Vec<Language>,
    pub input_kinds: Vec<TtsInputKind>,
    pub batch_output_formats: Vec<AudioFormat>,
    pub stream_output_formats: Vec<AudioFormat>,
    pub stream_output: bool,
    pub batch: bool,
    pub duplex_streaming: bool,
    pub text_event_source: TextEventSourceCapability,
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

pub enum TextEventSourceCapability {
    None,
    GatewayEcho,
    ProviderDelta,
}
```

Capabilities may come from provider metadata, static tables, or conservative assumptions. Strict mode must not rely on assumptions for fields that affect audio semantics.

### Observability

```rust
pub struct TtsTelemetry {
    pub trace_id: String,
    pub provider: String,
    pub model: String,
    /// `None` for operations that do not target a single voice, such as list_voices.
    pub voice_id: Option<String>,
    pub operation: TtsOperation,
    pub input_chars: u64,
    pub latency_first_audio_ms: Option<u64>,
    pub latency_final_ms: u64,
    pub audio_duration_ms: Option<u64>,
    pub output_bytes: Option<u64>,
    pub cost_estimate_micros: Option<u64>,
    pub provider_status: Option<u16>,
    pub option_adjustment_count: u32,
}

pub enum TtsOperation {
    Batch,
    SingleStream,
    DuplexStream,
    ListVoices,
}
```

The TTS crate emits lifecycle and audio events through `TtsStreamEvent` for streaming calls, and records spans / metrics for batch, streaming, and voice-listing calls. It must not require adding TTS-specific variants to core `RuntimeEvent`. Applications can align ASR -> Orchest -> TTS timelines through `trace_id`.

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
| Volcengine | P0 | Domestic realtime TTS baseline; first target model/resource `volcengine/seed-tts-2.0`; map bidirectional WebSocket and unidirectional streaming separately |
| Aliyun | P0 | Domestic realtime TTS comparison point; target `aliyun/cosyvoice-v3-flash` for CosyVoice realtime coverage and `aliyun/qwen3-tts-flash-realtime` / instruct variant for duplex text input and instruction capability calibration |

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
| `volcengine` | Volcengine adapter, optional HTTP/WebSocket dependencies, offline adapter tests, ignored live tests |
| `aliyun` | Aliyun adapter, optional HTTP/WebSocket dependencies, offline adapter tests, ignored live tests |

The default feature set should include the two first-party provider adapters: `default = ["volcengine", "aliyun"]`. Fake providers and router tests must compile without provider features. Provider-specific HTTP/WebSocket dependencies should be optional dependencies activated by the relevant feature. If an implementation proves that a dependency is lightweight and shared by both default adapters, the implementation PR must justify keeping it non-optional.

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
- `chrono`
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
- Capability validation for instruction, SSML, numeric speech control ranges, output format, custom voice, single-stream output and duplex streaming
- API key resolution hierarchy
- Error redaction preserves debug payload while removing secrets
- Streaming event ordering: `RouteSelected` -> `Started` -> zero or more `TextDelta` / `TextAccepted` / `AudioChunk` -> `Completed` with `TtsStreamSummary` or fatal `Error`

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
| 001 | Crate scaffold + public types | -- | Workspace entry, module layout, `TtsProvider`, request/result/summary/usage/error/capability/voice types |
| 002 | Gateway + router | 001 | `TtsGateway`, deterministic `TtsRouter`, request-level provider/model selection, gateway voice listing, operation-specific compatibility validation, fake-provider tests |
| 003 | Streaming contracts | 001, 002 | `TtsOutputStream`, `TtsDuplexStream`, gateway vs direct provider event ownership, terminal events, duplex input lifecycle, text delta semantics, audio chunk output, backpressure docs |
| 004 | Voice catalog + controls | 001, 002 | `VoiceInfo`, `VoiceSelection`, voice capability validation, instruction/emotion/SSML handling |
| 005 | Observability + trace | 001, 002, 003 | `TtsTelemetry`, `TtsStreamEvent` trace propagation, spans/metrics |
| 006 | Volcengine adapter | 001, 003, 004, 005 | Feature-gated provider implementation, config factory, live ignored test |
| 007 | Aliyun adapter + examples | 001, 003, 004, 005, 006 | Feature-gated provider implementation, config factory, live ignored test, README/example snippets |

## Acceptance Criteria

- [ ] `crates/agent-runtime-tts-providers` exists and is part of the workspace
- [ ] `TtsProvider` trait supports batch synthesis, single-stream synthesis, duplex streaming synthesis, and voice listing
- [ ] Crate has zero workspace-internal dependencies
- [ ] Provider/model selection uses the same explicit `"provider/model"` convention as `agent-runtime-providers`; unprefixed TTS model strings are rejected in v0.9.3, and direct adapters reject mismatched request selectors
- [ ] Provider-neutral types exist for provider/model selector, text input, voice selection, voice catalog request, speech controls with portable numeric ranges, output format, stream handle, stream event, stream summary, usage, telemetry with operation, routing, compatibility, semantic coercion policy and errors
- [ ] `TtsModelCapabilities` exists and strict compatibility checks use it
- [ ] `VoiceInfo` distinguishes system, cloned, designed and custom voices
- [ ] Instruction, SSML, speed, pitch, volume, emotion, voice kind and output format controls map to provider-native fields only when supported; invalid numeric ranges or unsupported controls return stable strict-mode errors
- [ ] `TtsGateway` can route synthesis and voice-listing requests to registered fake providers based on route priority, operation, language, voice kind and output format
- [ ] Router tie-break behavior is deterministic
- [ ] Public single-stream API accepts committed text and returns text/audio output events through `TtsOutputStream`
- [ ] Public duplex-stream API lets callers send `TextChunk`s and receive text/audio output events through `TtsDuplexStream`
- [ ] Direct provider streams start with `Started`, while gateway streams prepend `RouteSelected` before provider lifecycle events
- [ ] Streaming contract distinguishes route selection, start, text delta, accepted text, audio chunk, completion and fatal/non-fatal errors
- [ ] Strict and coerce compatibility reject duplex streaming for provider/model combinations that only support single-stream output; v0.9.3 does not implement implicit buffered duplex fallback
- [ ] Streaming tests assert stable event ordering, terminal-event behavior, duplex sender close/final-chunk behavior, public receiver cancellation behavior, `TtsStreamSummary` completion, and no provider-internal channel drain deadlock
- [ ] Telemetry includes `trace_id`, operation, provider/model and optional voice id, and can be correlated with external ASR and Orchest run traces
- [ ] `TtsError` preserves upstream status/code/message/body with secret redaction
- [ ] API key resolution follows explicit key -> explicit env var -> provider default env var
- [ ] Volcengine adapter is implemented behind a feature flag with fake/offline tests and ignored live tests
- [ ] Aliyun adapter is implemented behind a feature flag with fake/offline tests and ignored live tests
- [ ] Offline provider tests cover Volcengine session lifecycle, old/new credential configuration, and Aliyun realtime model capability differences
- [ ] Batch and streaming output format compatibility are validated separately, including streaming WAV/header caveats where provider docs require PCM/Ogg/MP3
- [ ] `cargo test -p agent-runtime-tts-providers --no-default-features` passes
- [ ] `cargo test -p agent-runtime-tts-providers --features volcengine` passes without live credentials
- [ ] `cargo test -p agent-runtime-tts-providers --features aliyun` passes without live credentials
- [ ] `cargo test -p agent-runtime-tts-providers --all-features` passes without live credentials
- [ ] PRD documents that `TtsTool` is out of scope for this standalone crate
- [ ] No changes are required in `agent-runtime-core`
- [ ] `cargo test -p agent-runtime-tts-providers` passes
- [ ] `cargo clippy -p agent-runtime-tts-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes
