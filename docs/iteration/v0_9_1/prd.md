# v0.9.1 Spec：ASR Provider Gateway

## 背景

Orchest 已有两类 provider 封装：

- `agent-runtime-providers`：LLM provider adapter，统一模型调用、streaming、tool protocol 映射
- `agent-runtime-aigc-providers`：AIGC provider gateway，统一图片等生成式媒体 provider、资产、任务和事件输出

音频原生 agent 产品需要同级别的 ASR/STT provider 封装，但 ASR 不应进入 `agent-runtime-core`。ASR 发生在 agent loop 之前，是输入适配和供应商路由问题；core runtime 仍只负责 agent loop、state、tool dispatch、events、budget 和 approval。

本卫星迭代新增 `agent-runtime-asr-providers` crate，为语音转文本提供统一 provider abstraction、streaming transcript、routing 和 telemetry。第一期只实现火山引擎和阿里云两个国内 ASR provider：两个 provider 足以验证公共抽象，避免围绕单一平台特化，同时不把迭代扩展成全球 ASR provider 横向铺开。

ElevenLabs Scribe v2 Realtime 作为接口校准参考，但不进入 v0.9.1 实现范围。它的价值在于提供一个海外、WebSocket、实时 STT、word timestamp、keyterm prompting、speaker diarization 语义较完整的对照面；PRD 的双流式接口不能被国内 provider 的私有协议形态绑死。后续接入计划记录在 `docs/todo/`。

参考研究：

- [`docs/research/audio-native-agent-asr-integration.md`](../../research/audio-native-agent-asr-integration.md)
- [`docs/research/audio-native-agent-products-landscape.md`](../../research/audio-native-agent-products-landscape.md)
- [`docs/research/audio-native-agent-full-architecture.md`](../../research/audio-native-agent-full-architecture.md)

本地供应商资料：

- [`docs/external/aliyun/asr-api-doc.md`](../../external/aliyun/asr-api-doc.md)
- [`docs/external/aliyun/asr-guideline.md`](../../external/aliyun/asr-guideline.md)
- [`docs/external/volceengine/asr.md`](../../external/volceengine/asr.md)

## 目标

1. 新增 `crates/agent-runtime-asr-providers` 卫星 crate
2. 定义 `AsrProvider` trait，streaming-first，并预留 one-shot `transcribe()` 接口
3. 定义 provider-neutral ASR 类型：audio input、options、result、stream handle、stream event、usage、telemetry、error
4. 提供 `AsrGateway` / `AsrRouter`，支持显式 `model` 调用；未指定 `model` 时可通过集中路由配置选择 provider
5. 提供结构化 observability，至少包含 `trace_id`、normalized model、latency、duration、confidence 和 cost estimate
6. 实现火山引擎和阿里云两个 provider adapter，live tests 通过 env var gated
7. 保持 core runtime 不变：ASR 输出 transcript 后由调用方传入 `AgentRun`

## 统一原则

- **Capability first**：调用方表达的是转写意图（语言、热词、是否流式、是否需要词级时间戳、是否启用 endpointing），不是某个 provider 的 endpoint 形状。
- **Standalone crate**：`agent-runtime-asr-providers` 不依赖 `agent-runtime-core` 或绑定 crate。core 可以后续选择 re-export 或包装成 tool，但 ASR crate 本身必须可独立使用。
- **No silent semantic loss**：provider 无法满足请求字段时，按 compatibility policy 返回 `unsupported_option` 或记录 `OptionAdjustment`。
- **Provider/model selector is explicit**：调用方可以在 request 上指定 normalized `"provider/model"`。未指定时才由 gateway router 按语言、地区、延迟和成本策略选择默认 route。
- **Unified ASR semantics stay typed**：`TranscribeOptions`、`AsrStreamEvent` 和 `TranscribeResult` 表达统一 ASR 语义，不把 provider-native 参数摊平成公共字段。
- **Provider-specific power has one escape hatch**：少数 provider/model 私有参数通过 request-level `provider_options` 传递给已选 adapter；当一个概念被至少两个 provider 稳定支持，或影响公共 ASR 合同时，再提升为 typed option。
- **Streaming is first-class**：`TranscriptUpdate`、`AsrFinal`、end-of-speech、error 都必须有稳定事件语义。
- **Core stays text-in/text-out**：实时用户语音是 pre-loop input adapter，不进入 agent loop。

## 范围

### Crate 结构

```text
crates/agent-runtime-asr-providers/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── traits.rs
│   ├── types.rs
│   ├── streaming.rs
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
    └── router.rs
```

Both provider modules must implement the public ASR contract. Live credential tests are ignored by default and gated by env vars, but fake-provider tests must exercise routing, compatibility, streaming, and telemetry behavior in normal CI.

### Dependency Direction

```text
agent-runtime-asr-providers/  <-- standalone, zero workspace-internal dependencies
  owns: AsrProvider trait + gateway + public ASR types + adapters

agent-runtime-core/           <-- no dependency added in v0.9.1
  owns: run loop, tools, skills, MCP, budget, events
```

`agent-runtime-asr-providers` must not import from `agent-runtime-core`, `agent-runtime-py`, or `agent-runtime-node`. If later ASR is exposed as an agent-initiated transcription tool, that wrapper belongs in core or a separate extension crate, not inside this provider crate.

### Core Trait

```rust
#[async_trait]
pub trait AsrProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capabilities(&self) -> AsrModelCapabilities;
    fn supported_languages(&self) -> &[Language];

    /// Reserved one-shot transcription surface for complete audio inputs.
    /// v0.9.1 does not require provider adapters to implement this path;
    /// unsupported providers return `AsrErrorCode::UnsupportedOperation`.
    async fn transcribe(
        &self,
        request: TranscribeRequest,
    ) -> Result<TranscribeResult, AsrError>;

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError>;
}
```

Streaming is duplex-first: callers send audio chunks into `AsrStream.input` and receive transcript / lifecycle events from `AsrStream.events`. The public API must not require callers to await one future while manually draining a second provider-internal channel.

`transcribe()` is reserved for future complete-audio transcription and file transcription work. v0.9.1 defines the request/result/error shape so downstream SDKs can see the intended API, but adapter implementation and batch/file acceptance tests are deferred. The follow-up item lives in `docs/todo/2026-06-04-asr-provider-backlog.md`.

### Key Types

```rust
pub struct TranscribeRequest {
    /// Optional explicit provider/model selector, for example
    /// "volcengine/bigmodel_async" or "aliyun/fun-asr-realtime".
    /// If omitted, `AsrGateway` selects a route.
    pub model: Option<String>,
    pub audio: AudioInput,
    pub options: TranscribeOptions,
    pub compatibility: CompatibilityPolicy,
    pub provider_options: Value,
}

pub struct StreamingTranscribeRequest {
    /// Optional explicit provider/model selector. If present, routing is limited
    /// to this provider/model and capability validation still applies.
    pub model: Option<String>,
    pub format: StreamingAudioFormat,
    pub options: TranscribeOptions,
    pub compatibility: CompatibilityPolicy,
    pub provider_options: Value,
}

pub struct TranscribeOptions {
    pub language: Option<Language>,
    /// Stable terminology hints. Adapters map this to provider-native hotword,
    /// keyterm, corpus, or prompt-context mechanisms when supported.
    pub hot_words: Vec<String>,
    pub context_prompt: Option<String>,
    pub code_switching: bool,
    pub punctuate: bool,
    pub interim_results: bool,
    pub vad: Option<VadOptions>,
    pub word_timestamps: bool,
    pub speaker_diarization: bool,
    pub trace_id: Option<String>,
}

pub struct TranscribeResult {
    pub text: String,
    pub language: Option<Language>,
    pub confidence: Option<f64>,
    pub words: Vec<WordTimestamp>,
    pub speakers: Vec<SpeakerSegment>,
    pub audio_duration_ms: u64,
    pub processing_latency_ms: u64,
    pub usage: AsrUsage,
    pub option_adjustments: Vec<OptionAdjustment>,
    pub telemetry: AsrTelemetry,
}

pub struct AsrUsage {
    pub audio_duration_ms: u64,
    pub billable_duration_ms: Option<u64>,
    pub input_bytes: Option<u64>,
    pub transcript_chars: Option<u64>,
    pub cost_estimate_micros: Option<u64>,
}

pub struct AsrStream {
    pub input: mpsc::Sender<AudioChunk>,
    pub events: mpsc::Receiver<AsrStreamEvent>,
}

pub enum TranscriptStability {
    /// Provider may revise this text in a later streaming update.
    Provisional,
    /// Provider has committed this segment, but the full ASR session may still
    /// produce more segments before `AsrFinal`.
    Committed,
}

pub enum TranscriptUpdateKind {
    /// `text` is the provider's current best snapshot for the segment.
    Snapshot,
    /// `text` is append-only text for the segment when the provider guarantees it.
    Append,
}

pub enum AsrStreamEvent {
    RouteSelected { trace_id: String, model: String },
    Started { trace_id: String, model: String },
    /// Low-latency streaming text update for UI and realtime feedback. This is
    /// not the canonical full transcript and must not be treated as the final
    /// text to feed into `AgentRun`.
    TranscriptUpdate {
        trace_id: String,
        segment_id: Option<String>,
        text: String,
        stability: TranscriptStability,
        update_kind: TranscriptUpdateKind,
    },
    EndOfSpeech { trace_id: String },
    /// Canonical final ASR result for the stream. Emitted exactly once after
    /// the provider has produced all transcript text and usage/metadata that it
    /// can provide. This is the event callers should use as agent input by
    /// default.
    AsrFinal { result: TranscribeResult },
    Error { trace_id: String, error: AsrError, fatal: bool },
}
```

`EndOfSpeech` is a normalized ASR/endpointing signal. It does not make Orchest responsible for turn-taking; voice applications may use it to decide when to start an `AgentRun`.

Streaming output has two distinct layers:

1. `TranscriptUpdate` is the realtime text stream. It may be provisional or committed at segment level, and providers may send either segment snapshots or append-only text. Applications may display it, but it is not the canonical complete transcript.
2. `AsrFinal` is the final ASR result for the stream. It carries the complete `TranscribeResult`, usage, telemetry and option adjustments. Unless an application has its own turn-taking policy, this is the only transcript event that should be submitted to `AgentRun`.

`TranscriptStability::Committed` is not the same as `AsrFinal`. A provider can commit multiple segments before the whole stream is final. `EndOfSpeech` is also not the same as `AsrFinal`; it is only a normalized speech boundary signal and may arrive before the provider has produced the final transcript.

`provider_options` is a request-level escape hatch for the selected provider/model. It is not a dumping ground for common fields. A field should be promoted into typed config when at least two providers support the concept or when it becomes important to the public ASR contract. Provider adapters must document supported `provider_options` keys and reject unknown or unsupported keys in `Strict` mode.

### Audio Input Contract

```rust
pub enum AudioInput {
    Bytes {
        data: Vec<u8>,
        format: AudioFormat,
        sample_rate_hz: Option<u32>,
    },
    File {
        path: PathBuf,
        format: Option<AudioFormat>,
    },
    Url {
        url: String,
        format: Option<AudioFormat>,
    },
}

pub enum StreamingAudioFormat {
    Pcm16 {
        sample_rate_hz: u32,
        channels: u16,
    },
    Encoded {
        format: AudioFormat,
    },
}

pub struct AudioChunk {
    pub data: Bytes,
    pub timestamp_ms: Option<u64>,
    pub is_final: bool,
}
```

The gateway does not perform general-purpose transcoding in v0.9.1. If a provider cannot accept the supplied format, strict mode returns `unsupported_audio_format`; coerce mode may only adjust metadata or choose a provider that supports the input. Actual audio conversion remains a caller-side concern.

Audio format compatibility must be explicit enough to test. Streaming and complete-audio inputs may have different constraints, so adapters must not collapse them into one coarse `Vec<AudioFormat>`.

### Gateway and Routing

```rust
pub struct AsrGateway {
    router: AsrRouter,
    config: AsrGatewayConfig,
}

pub struct AsrGatewayConfig {
    /// Optional centralized route config path. If omitted, requests must provide
    /// an explicit `model` or the gateway returns `no_matching_provider`.
    pub route_config_path: Option<PathBuf>,
}

pub struct AsrRouter {
    /// Registered provider instances keyed by normalized "provider/model".
    providers: HashMap<String, Arc<dyn AsrProvider>>,
    routes: Vec<AsrRoute>,
}

pub struct AsrRoute {
    pub languages: Vec<Language>,
    pub regions: Vec<NetworkRegion>,
    pub max_latency_ms: Option<u64>,
    pub max_cost_micros_per_minute: Option<u64>,
    pub priority: u8,
    /// Provider/model selector using the same "provider/model" convention as
    /// agent-runtime-providers, for example "volcengine/bigmodel_async",
    /// "aliyun/fun-asr-realtime", or "aliyun/qwen3-asr-flash-realtime".
    pub model: String,
}
```

Routing is independent from model routing. The only coupling is outside this crate: if a caller chooses a multimodal model that accepts audio directly, the caller may skip ASR entirely.

Automatic ASR routing is optional and must be configured centrally. v0.9.1 must not scatter route tables across provider adapters, examples, or hardcoded match arms. If the caller omits request `model`, `AsrGateway` loads route policy from a single config file and selects from that route set. If no route config is configured, omitting `model` returns `no_matching_provider`.

Suggested config location and shape:

```toml
# config/asr-routes.toml
[[routes]]
model = "volcengine/bigmodel_async"
priority = 10
languages = ["zh-CN"]
regions = ["cn"]
max_latency_ms = 800

[[routes]]
model = "aliyun/fun-asr-realtime"
priority = 20
languages = ["zh-CN", "en", "ja"]
regions = ["cn"]
```

The concrete file path is application configuration, not a global default baked into the crate. The crate owns parsing/validation types, deterministic selection and test coverage; applications own where the file lives.

Gateway helpers provide the public convenience surface:

```rust
impl AsrGateway {
    pub async fn transcribe(
        &self,
        request: TranscribeRequest,
    ) -> Result<TranscribeResult, AsrError>;

    pub async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError>;
}
```

If `TranscribeOptions.trace_id` is `None`, the gateway generates one and includes it in `RouteSelected`, `Started`, every transcript event, `TranscribeResult.telemetry`, and all emitted telemetry. Provider adapters should not generate unrelated trace IDs.

Provider/model selection follows one rule everywhere: a model string is a normalized `"provider/model"` selector. ASR reuses the `agent-runtime-providers` convention that the prefix selects the provider factory and the suffix is the provider-native model, but it does not inherit LLM bare-model fallback behavior. If `model` is present, it must contain a provider prefix; bare model strings such as `"fun-asr-realtime"` or `"bigmodel_async"` return `invalid_model`. `TranscribeRequest.model` and `StreamingTranscribeRequest.model` are request-level selectors. `AsrRoute.model` and `AsrProviderRuntimeConfig.model` are configuration-level selectors.

The router must be deterministic:

1. If the request has `model`, normalize it with `normalize_asr_provider_model()` and select only that provider/model.
2. If the request omits `model`, load the centralized route config, normalize each route's `model` string, and build the route candidate set.
3. Filter by capability compatibility.
4. Filter by route language/region when route selection is in use.
5. Apply latency/cost constraints when configured.
6. Select lowest `priority`, then stable normalized provider/model string sort as tie-breaker.

If no provider matches, or if the request omits `model` and no centralized route config is configured, return `no_matching_provider` with rejected constraints included in the stable error body. Provider/model rejection details may be attached to diagnostic metadata for debugging.

The prefix selects the factory, and the suffix is passed to the provider adapter as the provider-native model or service identifier. ASR does not introduce separate `provider` and `model` request fields; the public selector is always one `model: Option<String>` containing `"provider/model"`.

`provider_options` are interpreted only by the selected adapter and must not influence route selection. A request with non-empty `provider_options` must also provide explicit `model`; otherwise the gateway returns `invalid_request`. Automatic routing requests can only use provider-neutral typed fields in `TranscribeOptions`. In `Strict` mode, unsupported keys or values return `unsupported_option` or `invalid_request`; in `Coerce` mode, adapters may ignore or adjust only documented options and must record `OptionAdjustment`.

### Provider-Specific Protocol Notes

These notes are implementation constraints from the local vendor docs, not extra public API surface:

- **Aliyun Fun-ASR / Paraformer realtime** use WebSocket duplex tasks: send `run-task`, wait for `task-started`, stream binary audio chunks, receive `result-generated`, then send `finish-task` and wait for `task-finished`. `task-failed` is fatal for the current stream. Fun-ASR / Paraformer connections can be reused only after `task-finished`; failed tasks must discard the connection.
- **Aliyun Qwen-ASR realtime** uses a realtime session shape (`session.update`, `input_audio_buffer.append`, `input_audio_buffer.commit`, `session.finish`) and does not share the Fun-ASR connection reuse semantics. v0.9.1 may implement `aliyun/fun-asr-realtime` first, but the Aliyun adapter design must leave room for `aliyun/qwen3-asr-flash-realtime`.
- **Volcengine** exposes several realtime modes. v0.9.1 should target the optimized duplex streaming mode first (`bigmodel_async` in the local docs) because it returns only changed results and aligns with the gateway's duplex-first API. Provider-specific options such as `resource_id`, `enable_nonstream`, `enable_itn`, `enable_punc`, `enable_speaker_info`, `show_utterances`, `end_window_size`, and corpus/context fields remain in request-level `provider_options` unless promoted later.
- Provider finality signals map to two public layers. Aliyun `result-generated` / Qwen `conversation.item.input_audio_transcription.text` events map to `TranscriptUpdate`; Qwen `conversation.item.input_audio_transcription.completed`, Fun-ASR completed task state, and stream completion synthesis map to one terminal `AsrFinal`. Volcengine `definite` utterances map to `TranscriptUpdate { stability: Committed, ... }`; the adapter still emits one terminal `AsrFinal` after the provider stream is complete.
- Realtime `speaker_diarization` is not a universal capability. Aliyun Fun-ASR realtime does not support it; Volcengine requires provider-specific settings and compatible modes. Strict compatibility must reject unsupported diarization instead of silently ignoring it.
- Audio chunk pacing matters. Local docs use roughly 100 ms chunks for Aliyun examples and recommend 100-200 ms chunks for Volcengine. The gateway should document backpressure and chunk pacing but must not sleep internally in a way that prevents callers from controlling realtime capture.

### Runtime Config and Factory

```rust
pub struct AsrProviderRuntimeConfig {
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

pub fn create_asr_provider_from_config(
    config: AsrProviderRuntimeConfig,
) -> Result<Arc<dyn AsrProvider>, AsrError>;

pub struct NormalizedAsrProviderModel<'a> {
    pub provider: &'a str,
    pub model: &'a str,
}

pub fn normalize_asr_provider_model(model: &str) -> Result<NormalizedAsrProviderModel<'_>, AsrError>;
```

API key resolution follows the same hierarchy as other provider crates:

1. explicit `api_key`
2. explicit local `api_key_env`
3. provider default environment variable

If `api_key_env` is configured, a missing or empty value is an error and must not fall back to the provider default. SDKs and tool wrappers must not duplicate provider credential resolution.

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

- Requested `word_timestamps=true`, provider only supports segment timestamps -> strict returns `unsupported_option`; coerce sets `word_timestamps=false` and records adjustment only if caller still gets segment-level timing.
- Requested explicit language unsupported by provider -> strict returns `unsupported_language`; coerce may route to another provider but must not silently force vendor auto-detect on the same provider.
- Requested audio format unsupported by selected provider -> strict returns `unsupported_audio_format`; coerce may select another provider but must not transcode.

### Capabilities

```rust
pub struct AsrModelCapabilities {
    /// Normalized "provider/model" selector this capability record describes.
    pub model: String,
    pub languages: Vec<Language>,
    pub streaming: bool,
    pub batch: bool,
    pub streaming_inputs: Vec<AudioInputCapability>,
    pub batch_inputs: Vec<AudioInputCapability>,
    pub interim_results: bool,
    pub endpointing: bool,
    pub word_timestamps: bool,
    pub speaker_diarization: bool,
    pub confidence: bool,
    pub code_switching: bool,
    pub hot_words: bool,
    pub context_prompt: bool,
    pub provider_option_keys: Vec<String>,
    pub max_duration_ms: Option<u64>,
    pub source: CapabilitySource,
    pub diagnostic_metadata: Value,
}

pub struct AudioInputCapability {
    pub format: AudioFormat,
    pub sample_rates_hz: SampleRateSupport,
    pub channels: ChannelSupport,
    pub max_duration_ms: Option<u64>,
    pub max_bytes: Option<u64>,
}

pub enum SampleRateSupport {
    Any,
    Exact(Vec<u32>),
    Range { min: u32, max: u32 },
}

pub enum ChannelSupport {
    Any,
    Exact(Vec<u16>),
}
```

Capabilities may come from provider metadata, static tables, or conservative assumptions, but they are exposed in provider-neutral Orchest terms. Strict mode must not rely on assumptions for features that affect transcript semantics. `provider_option_keys` is a documented allow-list for request-level provider-specific options; adapters may keep richer validation internally, but unknown keys must be rejected in `Strict` mode.

`streaming_inputs` validates `StreamingTranscribeRequest.format` and `AudioChunk` stream expectations. `batch_inputs` is reserved for future `transcribe()` implementation, but the field is still part of the public capability contract so downstream SDKs can reason about upcoming complete-audio support without changing type shape.

### Observability

```rust
pub struct AsrTelemetry {
    pub trace_id: String,
    /// Normalized "provider/model" selected for this request.
    pub model: String,
    pub language: Option<String>,
    pub audio_duration_ms: u64,
    pub latency_first_update_ms: Option<u64>,
    pub latency_final_ms: u64,
    pub partial_rollback_count: u32,
    pub confidence_avg: Option<f64>,
    pub cost_estimate_micros: Option<u64>,
    pub network_region: Option<String>,
    pub upstream_status: Option<u16>,
    pub option_adjustment_count: u32,
}
```

The ASR crate emits lifecycle and transcript events through `AsrStreamEvent` for streaming calls, and records spans / metrics for streaming calls plus reserved `transcribe()` calls, including unsupported-operation outcomes. It must not require adding ASR-specific variants to core `RuntimeEvent`. Applications can align ASR events with Orchest run events through `trace_id`.

Spans:

- `asr.gateway.transcribe`
- `asr.gateway.stream`
- `asr.provider.request`
- `asr.provider.stream`
- `asr.router.select`

Metrics:

- request duration
- first `TranscriptUpdate` latency
- `AsrFinal` latency
- audio duration
- upstream error count by stable code/status
- partial rollback count
- option adjustment count

Do not log raw audio bytes, signed URLs, API keys, or full transcripts by default. Transcript logging is application policy.

### Error Handling

```rust
pub struct AsrError {
    pub message: String,
    pub code: AsrErrorCode,
    pub model: Option<String>,
    pub status: Option<u16>,
    pub upstream_code: Option<String>,
    pub upstream_message: Option<String>,
    pub upstream_body: Option<Value>,
    pub diagnostic_metadata: Value,
}

pub enum AsrErrorCode {
    MissingApiKey,
    InvalidApiKey,
    UnknownProvider,
    UnknownModel,
    NoMatchingProvider,
    UnsupportedOperation,
    UnsupportedOption,
    UnsupportedLanguage,
    UnsupportedAudioFormat,
    InvalidAudio,
    InvalidRequest,
    ProviderHttpError,
    ProviderStreamError,
    ProviderTaskFailed,
    Timeout,
    Cancelled,
}
```

Provider response bodies should be preserved in errors for debugging, with secrets redacted. Application control flow must use stable `AsrErrorCode` values and upstream status/code fields; `model` and diagnostic metadata are for routing/debug context.

### Provider Scope

Initial provider targets:

| Provider | Priority | Notes |
|----------|----------|-------|
| Volcengine | P0 | Domestic realtime ASR baseline; first target model/mode `volcengine/bigmodel_async` |
| Aliyun | P0 | Domestic realtime ASR comparison point; first target model `aliyun/fun-asr-realtime`, with request shape leaving room for `aliyun/qwen3-asr-flash-realtime` |

Reference-only provider:

| Provider | Status | Why |
|----------|--------|-----|
| ElevenLabs Scribe v2 Realtime | Not implemented in v0.9.1 | Interface calibration for realtime WebSocket STT, word timestamps, keyterm prompting and diarization semantics |

Live provider tests should be gated behind env vars and ignored by default. Fake-provider tests must run in normal CI.

### Feature Flags

| Feature | Contents |
|---------|----------|
| `volcengine` | Volcengine adapter and live tests |
| `aliyun` | Aliyun adapter and live tests |

The default feature set should include fake/test-safe infrastructure and the two first-party provider adapters unless a provider requires a heavy optional dependency. WebSocket dependencies for streaming providers should be feature-gated if they are not already required by the default adapters.

## Not in Scope

- Changes to `agent-runtime-core` run loop
- Adding `AsrPartialTranscript` / `AsrFinalTranscript` to core `RuntimeEvent`
- Continuous voice session orchestration
- Turn-taking policy, barge-in, interruption handling, WebRTC, SIP, Twilio, audio device management
- TTS provider gateway
- `ContentBlock::InputAudio` / native multimodal audio model support
- ElevenLabs / Deepgram / Soniox / AssemblyAI / Speechmatics implementation
- Treating real-time user speech ASR as a Tool
- Treating ASR as a Skill

## Tool Adapter Boundary

v0.9.1 does not implement an `AsrTool` because this crate must remain standalone and must not depend on `agent-runtime-core`.

A future core wrapper or extension crate may expose ASR as a Tool only for agent-initiated transcription, such as "transcribe this uploaded podcast file." That wrapper must be documented as distinct from real-time user speech input.

Real-time user speech flow remains:

```text
audio input -> ASR Gateway -> AsrFinal -> AgentRun
```

not:

```text
audio input -> AgentRun -> model calls transcribe tool
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
- Existing workspace conventions from `agent-runtime-providers` and `agent-runtime-aigc-providers`

New provider-specific HTTP/WebSocket dependencies must be feature-gated and justified in the implementation PR.

`anyhow` is not allowed in this library crate. Provider adapters should return `AsrError` with stable codes and preserved upstream details.

## Testing Strategy

Unit tests:

- Fake provider implements streaming paths and the reserved `transcribe()` unsupported-operation behavior
- Router determinism and tie-break behavior
- Compatibility policy strict/coerce behavior
- Capability validation, including streaming audio format, sample rate and channel constraints
- API key resolution hierarchy
- Error redaction preserves debug payload while removing secrets
- Model normalization rejects bare ASR model strings without provider prefix
- Requests with non-empty `provider_options` and no explicit `model` return `invalid_request`
- Streaming event ordering: `RouteSelected` -> `Started` -> zero or more `TranscriptUpdate` / `EndOfSpeech` -> exactly one `AsrFinal` or one fatal `Error`
- Streaming tests assert `TranscriptUpdate { stability: Committed, ... }` is not treated as terminal and that `AsrFinal` is emitted exactly once on successful streams

Integration tests:

- `cargo test -p agent-runtime-asr-providers` runs without live credentials
- Live provider tests are `#[ignore]` and gated by provider-specific env vars
- Volcengine and Aliyun live tests, when env vars are set, stream generated PCM chunks and assert non-empty text plus telemetry fields

Test fixtures:

- Include only tiny synthetic audio fixtures suitable for repository storage, or generate PCM fixtures in test code.
- Do not commit real user recordings.

## Issue Breakdown

| Issue | Title | Depends on | Scope |
|-------|-------|------------|-------|
| 001 | Crate scaffold + public types | -- | Workspace entry, module layout, `AsrProvider`, request/result/usage/error/capability types |
| 002 | Gateway + router | 001 | `AsrGateway`, deterministic `AsrRouter`, centralized route config parsing/validation, compatibility validation, fake-provider tests |
| 003 | Duplex streaming contract | 001, 002 | `AsrStream`, stream event ordering, end-of-speech semantics, backpressure docs |
| 004 | Observability + trace | 001, 002 | `AsrTelemetry`, `AsrStreamEvent` trace propagation, spans/metrics |
| 005 | Volcengine adapter | 001, 003, 004 | Feature-gated provider implementation, config factory, live ignored test |
| 006 | Aliyun adapter + examples | 001, 003, 004, 005 | Feature-gated provider implementation, config factory, live ignored test, README/example snippets |

## Acceptance Criteria

- [ ] `crates/agent-runtime-asr-providers` exists and is part of the workspace
- [ ] `AsrProvider` trait supports streaming transcription and reserves a one-shot `transcribe()` signature
- [ ] v0.9.1 adapters may return `AsrErrorCode::UnsupportedOperation` from `transcribe()`; batch/file transcription implementation is deferred to `docs/todo/2026-06-04-asr-provider-backlog.md`
- [ ] Crate has zero workspace-internal dependencies
- [ ] `TranscribeRequest` and `StreamingTranscribeRequest` expose an optional `model: Option<String>` request selector using the `"provider/model"` convention
- [ ] ASR model normalization rejects bare model strings without provider prefix instead of applying any default provider fallback
- [ ] `TranscribeRequest` and `StreamingTranscribeRequest` expose request-level `provider_options: Value` for selected provider/model-specific parameters
- [ ] Requests with non-empty `provider_options` and no explicit request `model` return `invalid_request`
- [ ] `TranscribeOptions` remains provider-neutral; provider-native parameters stay in `provider_options` until they are promoted into typed options by the common-field rule
- [ ] Provider-neutral types exist for audio input, options, result, stream handle, stream event, usage, telemetry, routing, compatibility and errors
- [ ] `AsrModelCapabilities` exists and strict compatibility checks use it
- [ ] `AsrModelCapabilities` distinguishes `streaming_inputs` from `batch_inputs` and includes format, sample-rate, channel, duration and byte-size constraints
- [ ] Strict compatibility rejects unsupported streaming audio format, sample rate or channel count using `unsupported_audio_format`
- [ ] `TranscribeOptions.hot_words` and `context_prompt` map to provider-native hotword/keyterm/corpus/prompt mechanisms only when supported, otherwise strict mode returns `unsupported_option`
- [ ] Strict compatibility rejects unsupported or unknown `provider_options` keys/values for the selected provider/model
- [ ] Automatic routing only runs when request `model` is omitted and a centralized route config file is configured
- [ ] Route config parsing/validation is centralized in the ASR crate and route definitions are not hardcoded in adapters or scattered across examples
- [ ] `AsrGateway` can route requests to registered fake providers based on centralized route config priority and language
- [ ] Omitting request `model` without configured route config returns `no_matching_provider`
- [ ] Router tie-break behavior is deterministic
- [ ] Public streaming API is duplex-first: callers send `AudioChunk`s and receive `AsrStreamEvent`s through one `AsrStream` handle
- [ ] Streaming contract distinguishes route selection, start, realtime transcript updates, terminal `AsrFinal`, end-of-speech and fatal/non-fatal errors
- [ ] Streaming contract treats `TranscriptUpdate` and `AsrFinal` as separate layers: realtime updates are display-oriented, while only `AsrFinal` is the canonical complete ASR result for default `AgentRun` input
- [ ] Streaming tests assert stable event ordering and no provider-internal channel drain deadlock
- [ ] Telemetry includes `trace_id` and can be correlated with an external Orchest run
- [ ] `AsrError` preserves upstream status/code/message/body with secret redaction
- [ ] API key resolution follows explicit key -> explicit env var -> provider default env var
- [ ] Volcengine adapter is implemented behind a feature flag with fake/offline tests and ignored live tests
- [ ] Aliyun adapter is implemented behind a feature flag with fake/offline tests and ignored live tests
- [ ] Offline provider tests cover Aliyun task lifecycle events and Volcengine `definite` utterance normalization
- [ ] Strict compatibility rejects realtime speaker diarization when the selected provider/model capabilities do not support it
- [ ] `cargo test -p agent-runtime-asr-providers --no-default-features` passes
- [ ] `cargo test -p agent-runtime-asr-providers --features volcengine` passes without live credentials
- [ ] `cargo test -p agent-runtime-asr-providers --features aliyun` passes without live credentials
- [ ] PRD documents that `AsrTool` is out of scope for this standalone crate
- [ ] No changes are required in `agent-runtime-core`
- [ ] `cargo test -p agent-runtime-asr-providers` passes
- [ ] `cargo clippy -p agent-runtime-asr-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes
