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

- [docs/external/aliyun/asr-api-doc.md](/Users/emile/Develop/orchest/docs/external/aliyun/asr-api-doc.md)
- [docs/external/aliyun/asr-guideline.md](/Users/emile/Develop/orchest/docs/external/aliyun/asr-guideline.md)
- [docs/external/volceengine/asr.md](/Users/emile/Develop/orchest/docs/external/volceengine/asr.md)

本地既有实现参考：

- [study_buddy 火山 ASR WebSocket handler](/Users/emile/Develop/study_buddy/server/src/speech/asr.rs)
- [study_buddy 火山二进制协议 helper](/Users/emile/Develop/study_buddy/server/src/speech/protocol.rs)

These implementation files are reference material for observed provider behavior and protocol edge cases. They are not protocol truth sources and are not authoritative API surface for Orchest. The implementation contract is this PRD plus the vendor documentation; when a local implementation conflicts with vendor docs, prefer the vendor docs and capture any intentional adapter behavior explicitly in this PRD.

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

Streaming is duplex-first: callers send audio chunks through an `AsrAudioSink` and receive transcript / lifecycle events through an `AsrEventStream`. The public API must not require callers to await one future while manually draining a second provider-internal channel.

SDK ergonomics are part of the contract, not example-only sugar. The same underlying stream must make both common voice shapes easy without making application-level capture or VAD decisions:

- **Application-driven segmented speech**：caller sends audio, calls `flush_and_wait_final()` when the application decides the current segment is complete, receives one `AsrFinalOutput` for that segment, and can then send the next segment on the same stream when the provider supports multi-segment streaming. The boundary source may be PTT, application-owned client-side VAD, server-side application logic, or another product event; the SDK must not choose that policy.
- **Full-duplex speech**：caller splits the stream into sink/events, keeps sending audio from a capture task, and consumes `TranscriptUpdate`, `EndOfSpeech`, and provider endpoint-driven `AsrFinal` from an independent task without request-response lockstep.

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
    pub timeline: AudioTimelineMode,
    pub options: TranscribeOptions,
    pub compatibility: CompatibilityPolicy,
    pub provider_options: Value,
}

pub enum AudioTimelineMode {
    /// Caller sends audio in continuous realtime order, including silence when
    /// the capture stream contains silence.
    ContinuousRealtime,
    /// Caller may omit silence and send speech-only chunks with capture
    /// timestamps. This is application-owned silence suppression, not SDK VAD.
    SparseSpeechOnly,
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
    pub endpointing: Option<EndpointingOptions>,
    pub word_timestamps: bool,
    pub speaker_diarization: bool,
    pub final_result_scope: FinalResultScope,
    pub flush_timeout: Option<Duration>,
    pub trace_id: Option<String>,
}

pub enum FinalResultScope {
    /// `AsrFinal.result.text` is the current finalized segment only.
    Segment,
    /// `AsrFinal.result.text` accumulates committed text across successful
    /// segment finalization boundaries in this stream.
    Stream,
}

pub struct EndpointingOptions {
    pub mode: EndpointingMode,
    /// Applies only when the selected provider/mode exposes a silence threshold.
    pub silence_timeout: Option<Duration>,
}

pub enum EndpointingMode {
    /// Provider-defined default behavior. This may be automatic segmenting,
    /// silence-based VAD, semantic endpointing, or manual-only depending on the
    /// provider/model.
    ProviderDefault,
    /// Provider-side endpointing based primarily on acoustic silence.
    AcousticSilence,
    /// Provider-side endpointing based on semantic / end-of-turn confidence.
    Semantic,
    /// Provider emits final transcript segments according to its own ASR
    /// algorithm, such as pauses, speaker changes, sentence/phrase boundaries,
    /// or max-delay constraints, without exposing a specific VAD mode.
    NaturalSegmenting,
    /// Disable provider-side endpointing when supported; caller controls segment
    /// finalization through `Flush` / `End`.
    ProviderDisabled,
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

pub struct AsrFinalOutput {
    pub trace_id: String,
    pub segment_id: Option<String>,
    pub reason: AsrFinalReason,
    pub result: TranscribeResult,
}

pub struct AsrUsage {
    pub audio_duration_ms: u64,
    pub billable_duration_ms: Option<u64>,
    pub input_bytes: Option<u64>,
    pub transcript_chars: Option<u64>,
    pub cost_estimate_micros: Option<u64>,
}

pub struct AsrStream {
    pub input: AsrAudioSink,
    pub events: AsrEventStream,
}

pub struct AsrAudioSink {
    inner: mpsc::Sender<AudioChunk>,
}

impl AsrAudioSink {
    pub async fn send_audio(
        &self,
        data: Bytes,
        timestamp_ms: Option<u64>,
    ) -> Result<(), AsrError>;

    /// Finalize the current segment and keep the stream open.
    pub async fn flush_segment(&self) -> Result<(), AsrError>;

    /// Finalize the current segment and request stream shutdown after finalization.
    pub async fn end_stream(&self) -> Result<(), AsrError>;
}

pub struct AsrEventStream {
    inner: mpsc::Receiver<AsrStreamEvent>,
}

impl AsrEventStream {
    pub async fn next(&mut self) -> Option<AsrStreamEvent>;
    pub async fn next_final(&mut self) -> Result<AsrFinalOutput, AsrError>;
}

impl AsrStream {
    pub fn split(self) -> (AsrAudioSink, AsrEventStream);
    pub async fn flush_and_wait_final(&mut self) -> Result<AsrFinalOutput, AsrError>;
    pub async fn end_and_wait_final(&mut self) -> Result<AsrFinalOutput, AsrError>;
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

pub enum AsrFinalReason {
    /// Caller sent `AudioChunkBoundary::Flush`.
    CallerFlush,
    /// Caller sent `AudioChunkBoundary::End`.
    CallerEnd,
    /// Provider-side endpointing finalized a speech segment.
    ProviderEndpoint,
    /// Adapter finalized after waiting too long for provider output.
    Timeout,
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
    EndOfSpeech { trace_id: String, segment_id: Option<String> },
    /// Canonical ASR result for a normalized segment finalization after the
    /// provider has produced transcript text and usage/metadata for that
    /// segment. This is the event callers should use as agent input by default.
    AsrFinal { final_output: AsrFinalOutput },
    Error { trace_id: String, error: AsrError, fatal: bool },
}
```

`EndpointingOptions` controls provider-side endpointing when the selected provider exposes compatible settings. It does not mean Orchest implements, selects, or tunes client-side VAD, microphone capture, noise gates, WebRTC VAD, silence suppression, or turn-taking policy in this crate. If an application uses client-side VAD, that application owns the algorithm and decides when to call `flush_segment()` / `flush_and_wait_final()`.

Client-side silence detection and provider-side endpointing serve different purposes, but provider behavior is not uniform. A common application architecture uses client-side VAD / silence suppression to avoid sending silent audio and reduce bandwidth or ASR billing exposure. Provider-side endpointing may be acoustic silence detection, semantic/end-of-turn detection, provider-defined natural speech segmentation, or absent unless the caller sends a manual commit/end signal. The SDK must represent whether the caller is sending a continuous realtime timeline or sparse speech-only chunks. It must not synthesize silence, drop silence, sleep to emulate realtime pacing, or choose a silence policy for the caller. If an application suppresses silence and still needs an immediate segment final, it should send `AudioChunkBoundary::Flush`; it must not assume the provider can infer speech-end from silence that was never sent.

`EndOfSpeech` is a normalized ASR/endpointing signal from provider-side endpointing or equivalent provider events. It does not make Orchest responsible for turn-taking; voice applications may use it to decide when to start an `AgentRun`.

Streaming output has two distinct layers:

1. `TranscriptUpdate` is the realtime text stream. It may be provisional or committed at segment level, and providers may send either segment snapshots or append-only text. Applications may display it, but it is not the canonical complete transcript.
2. `AsrFinal` is the canonical ASR result for a finalized segment. Finalization may be caller-initiated (`Flush` / `End`) or provider-initiated through server-side VAD / endpointing. `AsrFinal` carries `AsrFinalOutput`, including trace ID, segment ID, reason, complete `TranscribeResult`, usage, telemetry and option adjustments. Unless an application has its own turn-taking policy, this is the only transcript event that should be submitted to `AgentRun`.

`TranscriptStability::Committed` is not the same as `AsrFinal`. A provider can commit multiple segments before the whole stream is final. `EndOfSpeech` is also not automatically the same as `AsrFinal`; it is a normalized speech boundary signal and may arrive before the provider has produced canonical final text. If the provider also delivers final text/metadata for that endpointed segment, the adapter emits `AsrFinal { final_output: AsrFinalOutput { reason: ProviderEndpoint, ... } }`.

If provider endpointing and caller `Flush` are both enabled, adapters must avoid duplicate finals for the same normalized segment. A segment can produce at most one `AsrFinalOutput`; the `reason` records the source that actually finalized it. If a caller waits for finalization and the provider endpoint arrives first, `flush_and_wait_final()` may return that provider-endpoint final instead of forcing a second caller-flush final. Applications that require deterministic caller-owned segmentation should request `EndpointingMode::ProviderDisabled` when the provider supports it.

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
    pub boundary: AudioChunkBoundary,
}

pub enum AudioChunkBoundary {
    /// Regular audio payload for the current segment.
    None,
    /// Caller requests finalization of the current segment while keeping the
    /// stream/session alive for future audio.
    Flush,
    /// Caller requests finalization of the current segment and then stream end.
    End,
}
```

The gateway does not perform general-purpose transcoding in v0.9.1. If a provider cannot accept the supplied format, strict mode returns `unsupported_audio_format`; coerce mode may only adjust metadata or choose a provider that supports the input. Actual audio conversion remains a caller-side concern.

`AudioChunk.timestamp_ms` is caller-supplied capture timing metadata. It is useful when applications send sparse speech-only chunks after client-side silence suppression. Adapters may forward timing only when the provider supports it; otherwise they preserve it for telemetry/diagnostics and must not fabricate silent audio to fill gaps. If the selected provider requires a continuous realtime audio timeline for correct endpointing, timeout behavior, or billing semantics, `AudioTimelineMode::SparseSpeechOnly` must fail compatibility in strict mode.

`AudioChunkBoundary::Flush` is caller-initiated EOS for the current ASR segment, not a request to close the `AsrStream`. It exists because providers such as Volcengine require an explicit protocol last-frame to trigger final recognition for the current segment. Dropping the audio sink is cancellation/end-of-client-input behavior and must not be treated as equivalent to a provider protocol flush.

`AudioChunkBoundary::End` is a final segment boundary plus caller intent to end the stream after finalization. Adapters may map `Flush` and `End` to the same provider last-frame when the provider protocol requires it, but `End` also lets the gateway close local resources after the terminal `AsrFinal`.

When a stream supports multiple finalization boundaries, each normalized segment finalization emits at most one `AsrFinal`. `TranscribeOptions.final_result_scope` controls whether `TranscribeResult.text` is the finalized segment text or the accumulated confirmed stream text. `FinalResultScope::Stream` is useful when one WebSocket connection spans multiple user utterances.

`TranscribeOptions.flush_timeout` bounds flush-to-final latency. If the provider does not respond after a flush, adapters must emit `AsrFinalOutput { reason: Timeout, ... }` with whatever committed text is available, set an option/diagnostic marker indicating timeout, and then allow the next segment to proceed unless the provider connection itself failed. If the provider later sends a final response for the timed-out segment, the adapter must not emit a second `AsrFinal`; it may record the late response in diagnostics or telemetry.

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

- **Local references are implementation aids, not protocol truth sources.** Aliyun adapter work should review `docs/external/aliyun/asr-api-doc.md` and `docs/external/aliyun/asr-guideline.md`; Volcengine adapter work should review `docs/external/volceengine/asr.md` plus the study_buddy ASR implementation files listed above for observed binary-frame, flush timeout, and definite-utterance deduplication behavior. The adapter contract remains this PRD plus vendor documentation, not the local reference implementations.
- **Endpointing varies by provider.** Some providers expose explicit acoustic silence VAD with configurable silence timeouts; some expose semantic/end-of-turn detection; some emit final segments through provider-defined natural segmentation without a configurable VAD mode; some require manual commit/finish events for finalization. Adapters must map these behaviors into `EndpointingMode`, `EndOfSpeech`, `AsrFinalReason`, `segment_flush`, and `multi_segment_streaming` rather than assuming one universal VAD model.
- **Audio timeline requirements vary by provider.** Some providers expect continuous realtime audio and may use transmitted silence for endpointing, timeout, or segment timing. Others tolerate sparse speech-only chunks when timestamps are preserved, or operate mainly through explicit commit/flush boundaries. Adapters must validate `StreamingTranscribeRequest.timeline` against `AsrModelCapabilities.audio_timeline_modes`; they must not insert synthetic silence to make an unsupported sparse stream look continuous.
- **Aliyun Fun-ASR / Paraformer realtime** use WebSocket duplex tasks: send `run-task`, wait for `task-started`, stream binary audio chunks, receive `result-generated`, then send `finish-task` and wait for `task-finished`. `task-failed` is fatal for the current stream. Fun-ASR / Paraformer connections can be reused only after `task-finished`; failed tasks must discard the connection. Reused tasks need new provider task IDs, so this maps to `ConnectionReuse::ReusableAfterProviderTaskFinished`, not transparent reuse of one provider task.
- **Aliyun Fun-ASR / Paraformer endpointing** is acoustic silence based: local docs expose `max_sentence_silence` as the VAD sentence boundary threshold. This maps to `EndpointingMode::AcousticSilence` and `EndpointingOptions.silence_timeout` when supported. `result-generated` maps to `TranscriptUpdate`; `task-finished` or explicit finish synthesis maps to `AsrFinal`.
- **Aliyun Qwen-ASR realtime** uses a realtime session shape (`session.update`, `input_audio_buffer.append`, `input_audio_buffer.commit`, `session.finish`) and does not share the Fun-ASR connection reuse semantics. It has server VAD mode by default (`session.turn_detection` with `type=server_vad`, `silence_duration_ms`, and provider-specific `threshold`) and Manual mode when `turn_detection=null`, where callers send `input_audio_buffer.commit`. Manual mode maps naturally to `AudioChunkBoundary::Flush`; `session.finish` maps to `AudioChunkBoundary::End`. v0.9.1 may implement `aliyun/fun-asr-realtime` first, but the Aliyun adapter design must leave room for `aliyun/qwen3-asr-flash-realtime`.
- **Aliyun Qwen-ASR constraints** from local docs: non-VAD/manual mode recommends keeping accumulated sent audio under 60 seconds; Qwen-ASR Realtime sessions must be closed after session end and do not support connection reuse; Qwen-ASR Realtime currently does not return timestamps. Strict capability checks must reject unsupported `word_timestamps` for Qwen realtime and unsupported sparse/manual timeline combinations when a model requires continuous realtime audio.
- **Aliyun Qwen-ASR transcript events** split realtime text into `text` plus `stash` in local examples; adapters must combine them according to provider semantics before emitting public `TranscriptUpdate`. Qwen/Paraformer emotion fields are provider metadata in v0.9.1 and must not be promoted into the public ASR result until the common-field rule is satisfied.
- **Volcengine** exposes several realtime modes. v0.9.1 should target the optimized duplex streaming mode first (`bigmodel_async` in the local docs) because it returns only changed results and aligns with the gateway's duplex-first API. Provider-specific options such as `resource_id`, `enable_nonstream`, `enable_itn`, `enable_punc`, `enable_speaker_info`, `show_utterances`, `end_window_size`, and corpus/context fields remain in request-level `provider_options` unless promoted later.
- **Volcengine binary protocol** is not JSON-over-WebSocket. The adapter must implement the provider's binary frame protocol: compact header bytes, message flags including last-frame / no-sequence variants, payload size fields, gzip-compressed full client request payloads, audio-only frames, sequence/error-code parsing, response decompression, and JSON payload parsing after decompression. `AudioChunkBoundary::Flush` / `End` maps to the provider last-frame required to trigger final results.
- Provider finality signals map to two public layers. Aliyun `result-generated` / Qwen `conversation.item.input_audio_transcription.text` events map to `TranscriptUpdate`; provider endpoint/completed signals such as Qwen `conversation.item.input_audio_transcription.completed`, Fun-ASR completed task state, explicit flush completion, and stream completion synthesis map to `AsrFinalOutput` with an `AsrFinalReason`. Volcengine `definite` utterances map to `TranscriptUpdate { stability: Committed, ... }`; when Volcengine or another provider also declares an endpointed segment final, the adapter emits `AsrFinal { final_output: AsrFinalOutput { reason: ProviderEndpoint, ... } }`.
- Providers may replay already committed segments. Volcengine `show_utterances=true` can return previous `definite` utterances again in later responses. Adapters must deduplicate committed segments before emitting `TranscriptUpdate { stability: Committed, ... }`; gateway consumers should see each committed segment once. A stable key should use provider timing plus text identity when available, for example `(start_time, end_time, text_hash)`.
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
    pub audio_timeline_modes: Vec<AudioTimelineMode>,
    pub interim_results: bool,
    pub endpointing_modes: Vec<EndpointingMode>,
    pub segment_flush: bool,
    pub multi_segment_streaming: bool,
    pub connection_reuse: ConnectionReuse,
    pub word_timestamps: bool,
    pub speaker_diarization: bool,
    pub confidence: bool,
    pub code_switching: bool,
    pub hot_words: bool,
    pub context_prompt: bool,
    pub provider_option_keys: Vec<String>,
    pub max_duration_ms: Option<u64>,
    pub default_flush_timeout_ms: Option<u64>,
    pub source: CapabilitySource,
    pub diagnostic_metadata: Value,
}

pub enum ConnectionReuse {
    NotReusable,
    ReusableAfterTerminalFinal,
    ReusableAfterProviderTaskFinished,
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
    pub update_rollback_count: u32,
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
- transcript update rollback count
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
- Capability validation, including streaming audio format, sample rate, channel, timeline, endpointing and connection-reuse constraints
- API key resolution hierarchy
- Error redaction preserves debug payload while removing secrets
- Model normalization rejects bare ASR model strings without provider prefix
- Requests with non-empty `provider_options` and no explicit `model` return `invalid_request`
- Flush boundary handling: `AudioChunkBoundary::Flush` emits an `AsrFinal` for the current segment without closing the stream; `AudioChunkBoundary::End` finalizes and then allows stream teardown
- SDK ergonomics for both application-driven segmentation and full-duplex: `flush_and_wait_final()` returns the current segment final for application-owned boundaries, and `split()` supports independent audio capture and event-consumer tasks
- Flush timeout handling: if a fake provider does not answer after flush, the adapter emits `AsrFinalOutput { reason: Timeout, ... }` and permits the next segment to proceed
- Late provider final handling: if the provider sends final output after timeout for the same segment, the adapter suppresses duplicate `AsrFinal`
- Provider endpointing handling: a fake provider can emit `EndOfSpeech` and then `AsrFinalOutput { reason: ProviderEndpoint, ... }` without caller `Flush`
- Streaming event ordering: `RouteSelected` -> `Started` -> zero or more `TranscriptUpdate` / `EndOfSpeech` -> one `AsrFinal` for each finalized normalized segment or one fatal `Error`
- Streaming tests assert `TranscriptUpdate { stability: Committed, ... }` is not treated as terminal and that each normalized segment finalization emits one `AsrFinalOutput` with the corresponding `AsrFinalReason`
- Streaming tests assert provider endpointing plus caller `Flush` does not emit duplicate `AsrFinal` for the same normalized segment
- Streaming tests assert committed segment deduplication before public `TranscriptUpdate` emission

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
| 003 | Duplex streaming contract | 001, 002 | `AsrStream`, application-driven segmentation/full-duplex SDK helpers, stream event ordering, end-of-speech semantics, backpressure docs |
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
- [ ] `AsrModelCapabilities` distinguishes `streaming_inputs` from `batch_inputs` and includes format, sample-rate, channel, timeline, duration and byte-size constraints
- [ ] `AsrModelCapabilities.endpointing_modes` declares which provider-side endpointing modes are supported instead of collapsing all provider behavior into one boolean
- [ ] `AsrModelCapabilities.connection_reuse` declares whether provider streams are not reusable, reusable after terminal final, or reusable only after provider task-finished events
- [ ] `EndpointingOptions` only configures provider-side endpointing behavior; client-side VAD, silence suppression, and silence policy remain application-owned and out of scope
- [ ] `EndpointingOptions.silence_timeout` is applied only for provider/mode combinations that expose a silence threshold; unsupported use returns `unsupported_option` in strict mode
- [ ] Strict compatibility rejects unsupported streaming audio format, sample rate or channel count using `unsupported_audio_format`
- [ ] `TranscribeOptions.hot_words` and `context_prompt` map to provider-native hotword/keyterm/corpus/prompt mechanisms only when supported, otherwise strict mode returns `unsupported_option`
- [ ] Strict compatibility rejects unsupported or unknown `provider_options` keys/values for the selected provider/model
- [ ] Automatic routing only runs when request `model` is omitted and a centralized route config file is configured
- [ ] Route config parsing/validation is centralized in the ASR crate and route definitions are not hardcoded in adapters or scattered across examples
- [ ] `AsrGateway` can route requests to registered fake providers based on centralized route config priority and language
- [ ] Omitting request `model` without configured route config returns `no_matching_provider`
- [ ] Router tie-break behavior is deterministic
- [ ] Public streaming API is duplex-first: callers send audio through `AsrAudioSink` and receive `AsrStreamEvent`s through `AsrEventStream`
- [ ] `AsrStream::split()` supports full-duplex use where audio capture and event consumption run in independent tasks without request-response lockstep
- [ ] `AsrStream::flush_and_wait_final()` supports application-driven segmentation by sending a segment flush and returning the corresponding `AsrFinalOutput` while keeping the stream reusable when supported
- [ ] `StreamingTranscribeRequest.timeline` declares whether the caller sends continuous realtime audio or sparse speech-only chunks
- [ ] Strict compatibility rejects sparse speech-only timelines for providers/models that require continuous realtime audio
- [ ] SDK never synthesizes silence, drops silence, or chooses a client-side VAD policy
- [ ] `AudioChunk.timestamp_ms` can preserve caller capture timing for sparse audio streams, but adapters do not fabricate silent audio to fill omitted gaps
- [ ] Public streaming API distinguishes regular audio, segment flush, and stream end through `AudioChunkBoundary`
- [ ] `AudioChunkBoundary::Flush` finalizes the current segment without requiring the audio sink to be dropped
- [ ] Flush-to-final timeout is configurable and emits `AsrFinalOutput { reason: Timeout, ... }` instead of leaving callers waiting indefinitely
- [ ] Late provider final output after timeout does not emit a duplicate `AsrFinal` for the timed-out segment
- [ ] `TranscribeResult.text` behavior for multi-flush streams is controlled by `FinalResultScope::Segment` vs `FinalResultScope::Stream`
- [ ] `AsrFinalOutput` carries trace ID, segment ID, `AsrFinalReason`, and `TranscribeResult`
- [ ] `AsrFinalReason` distinguishes caller flush, caller end, provider endpointing and timeout finalization
- [ ] Provider-side endpointing can emit `EndOfSpeech` and, when final text/metadata is available, `AsrFinalOutput { reason: ProviderEndpoint, ... }` without caller `Flush`
- [ ] A normalized segment emits at most one `AsrFinal`; provider endpointing plus caller `Flush` for the same segment must not duplicate final output
- [ ] Streaming contract distinguishes route selection, start, realtime transcript updates, segment-level `AsrFinal`, end-of-speech and fatal/non-fatal errors
- [ ] Streaming contract treats `TranscriptUpdate` and `AsrFinal` as separate layers: realtime updates are display-oriented, while only `AsrFinal` is the canonical finalized ASR result for default `AgentRun` input
- [ ] Streaming tests assert stable event ordering and no provider-internal channel drain deadlock
- [ ] Telemetry includes `trace_id` and can be correlated with an external Orchest run
- [ ] `AsrError` preserves upstream status/code/message/body with secret redaction
- [ ] API key resolution follows explicit key -> explicit env var -> provider default env var
- [ ] Volcengine adapter is implemented behind a feature flag with fake/offline tests and ignored live tests
- [ ] Aliyun adapter is implemented behind a feature flag with fake/offline tests and ignored live tests
- [ ] Offline provider tests cover Aliyun task lifecycle events and Volcengine binary frame protocol parsing/building
- [ ] Offline provider tests cover Aliyun Fun-ASR / Paraformer connection reuse only after `task-finished` and failed-task connection discard
- [ ] Offline provider tests cover Aliyun Qwen-ASR server VAD vs Manual mode mapping when Qwen support is added
- [ ] Offline provider tests cover Aliyun Qwen-ASR `text` + `stash` transcript normalization when Qwen support is added
- [ ] Capability tests reject `word_timestamps=true` for Aliyun Qwen-ASR realtime and allow timestamp support only for models that expose it
- [ ] Offline provider tests cover Volcengine `definite` utterance normalization and deduplication before public committed updates
- [ ] Strict compatibility rejects realtime speaker diarization when the selected provider/model capabilities do not support it
- [ ] `cargo test -p agent-runtime-asr-providers --no-default-features` passes
- [ ] `cargo test -p agent-runtime-asr-providers --features volcengine` passes without live credentials
- [ ] `cargo test -p agent-runtime-asr-providers --features aliyun` passes without live credentials
- [ ] PRD documents that `AsrTool` is out of scope for this standalone crate
- [ ] No changes are required in `agent-runtime-core`
- [ ] `cargo test -p agent-runtime-asr-providers` passes
- [ ] `cargo clippy -p agent-runtime-asr-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes
