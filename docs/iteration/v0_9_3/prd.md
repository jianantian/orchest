# v0.9.3 Spec：ASR Provider Gateway

## 背景

Orchest 已有两类 provider 封装：

- `agent-runtime-providers`：LLM provider adapter，统一模型调用、streaming、tool protocol 映射
- `agent-runtime-aigc-providers`：AIGC provider gateway，统一图片等生成式媒体 provider、资产、任务和事件输出

音频原生 agent 产品需要同级别的 ASR/STT provider 封装，但 ASR 不应进入 `agent-runtime-core`。ASR 发生在 agent loop 之前，是输入适配和供应商路由问题；core runtime 仍只负责 agent loop、state、tool dispatch、events、budget 和 approval。

本卫星迭代新增 `agent-runtime-asr-providers` crate，为语音转文本提供统一 provider abstraction、streaming transcript、routing 和 telemetry。它与 v0.9 主线 Supervised Delegation 并行，不阻塞 core 演进。

参考研究：

- [`docs/research/audio-native-agent-asr-integration.md`](../../research/audio-native-agent-asr-integration.md)
- [`docs/research/audio-native-agent-products-landscape.md`](../../research/audio-native-agent-products-landscape.md)
- [`docs/research/audio-native-agent-full-architecture.md`](../../research/audio-native-agent-full-architecture.md)

## 目标

1. 新增 `crates/agent-runtime-asr-providers` 卫星 crate
2. 定义 `AsrProvider` trait，统一 batch 和 streaming transcription
3. 定义 provider-neutral ASR 类型：audio input、options、result、stream item、usage、telemetry、error
4. 提供 `AsrGateway` / `AsrRouter`，支持按语言、地区、成本和延迟策略选择 provider
5. 提供结构化 observability，至少包含 `trace_id`、latency、duration、confidence、provider 和 cost estimate
6. 提供第一批 provider adapter skeleton，并实现 Deepgram adapter 或等价 P0 provider adapter
7. 保持 core runtime 不变：ASR 输出 transcript 后由调用方传入 `AgentRun`

## 统一原则

- **Capability first**：调用方表达的是转写意图（语言、热词、是否流式、是否需要词级时间戳、是否启用 endpointing），不是某个 provider 的 endpoint 形状。
- **Standalone crate**：`agent-runtime-asr-providers` 不依赖 `agent-runtime-core` 或绑定 crate。core 可以后续选择 re-export 或包装成 tool，但 ASR crate 本身必须可独立使用。
- **No silent semantic loss**：provider 无法满足请求字段时，按 compatibility policy 返回 `unsupported_option` 或记录 `OptionAdjustment`。
- **Provider details are adapter-private by default**：公共返回值归一化；provider 原始响应只保留在 `provider_metadata` / error debug 字段中。
- **Provider-specific power remains available**：通用稳定字段进入 typed config；少数 provider 特性通过 `provider_options` 透传。
- **Streaming is first-class**：partial、final、end-of-speech、error 都必须有稳定事件语义。
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
│       ├── deepgram.rs
│       ├── soniox.rs
│       ├── assemblyai.rs
│       ├── speechmatics.rs
│       └── elevenlabs.rs
└── tests/
    ├── fake_provider.rs
    └── router.rs
```

Provider modules may start as feature-gated skeletons if live credentials are unavailable, but the crate must have fake-provider tests that exercise the public contract.

### Dependency Direction

```text
agent-runtime-asr-providers/  <-- standalone, zero workspace-internal dependencies
  owns: AsrProvider trait + gateway + public ASR types + adapters

agent-runtime-core/           <-- no dependency added in v0.9.3
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

    async fn transcribe(
        &self,
        request: TranscribeRequest,
    ) -> Result<TranscribeResult, AsrError>;

    async fn stream_transcribe(
        &self,
        request: StreamingTranscribeRequest,
        tx: mpsc::Sender<TranscribeStreamItem>,
    ) -> Result<TranscribeResult, AsrError>;
}
```

`stream_transcribe()` returns the final `TranscribeResult` after the stream completes. Callers must drain `tx` while awaiting the returned future; higher-level gateway helpers may wrap this as a single stream to avoid misuse.

### Key Types

```rust
pub struct TranscribeRequest {
    pub audio: AudioInput,
    pub options: TranscribeOptions,
    pub compatibility: CompatibilityPolicy,
    pub provider_options: Value,
}

pub struct StreamingTranscribeRequest {
    pub audio: AudioStream,
    pub options: TranscribeOptions,
    pub compatibility: CompatibilityPolicy,
    pub provider_options: Value,
}

pub struct TranscribeOptions {
    pub language: Option<Language>,
    pub model: Option<String>,
    pub hot_words: Vec<String>,
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
    pub provider_latency_ms: u64,
    pub usage: AsrUsage,
    pub option_adjustments: Vec<OptionAdjustment>,
    pub provider_metadata: Value,
    pub telemetry: AsrTelemetry,
}

pub struct AsrUsage {
    pub audio_duration_ms: u64,
    pub billable_duration_ms: Option<u64>,
    pub input_bytes: Option<u64>,
    pub transcript_chars: Option<u64>,
    pub cost_estimate_micros: Option<u64>,
}

pub enum TranscribeStreamItem {
    Started { trace_id: String, provider: String, model: String },
    Partial { text: String, stable: bool, segment_id: Option<String> },
    Final { result: TranscribeResult },
    EndOfSpeech,
    Error { error: AsrError, fatal: bool },
}
```

`EndOfSpeech` is a normalized ASR/endpointing signal. It does not make Orchest responsible for turn-taking; voice applications may use it to decide when to start an `AgentRun`.

`provider_options` is an explicit escape hatch. It is not a dumping ground for common fields. A field should be promoted into typed config when at least two providers support the concept or when it becomes important to the public ASR contract.

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

pub enum AudioStream {
    Pcm16 {
        rx: mpsc::Receiver<AudioChunk>,
        sample_rate_hz: u32,
        channels: u16,
    },
    Encoded {
        rx: mpsc::Receiver<AudioChunk>,
        format: AudioFormat,
    },
}

pub struct AudioChunk {
    pub data: Bytes,
    pub timestamp_ms: Option<u64>,
    pub is_final: bool,
}
```

The gateway does not perform general-purpose transcoding in v0.9.3. If a provider cannot accept the supplied format, strict mode returns `unsupported_audio_format`; coerce mode may only adjust metadata or choose a provider that supports the input. Actual audio conversion remains a caller-side concern.

### Gateway and Routing

```rust
pub struct AsrGateway {
    router: AsrRouter,
    config: AsrGatewayConfig,
}

pub struct AsrRouter {
    providers: HashMap<String, Arc<dyn AsrProvider>>,
    routes: Vec<AsrRoute>,
}

pub struct AsrRoute {
    pub languages: Vec<Language>,
    pub regions: Vec<NetworkRegion>,
    pub max_latency_ms: Option<u64>,
    pub max_cost_micros_per_minute: Option<u64>,
    pub priority: u8,
    pub provider: String,
}
```

Routing is independent from model routing. The only coupling is outside this crate: if a caller chooses a multimodal model that accepts audio directly, the caller may skip ASR entirely.

Gateway helpers provide the public convenience surface:

```rust
impl AsrGateway {
    pub async fn transcribe(
        &self,
        request: TranscribeRequest,
    ) -> Result<TranscribeResult, AsrError>;

    pub fn stream_transcribe(
        &self,
        request: StreamingTranscribeRequest,
    ) -> (impl Future<Output = Result<TranscribeResult, AsrError>>, mpsc::Receiver<TranscribeStreamItem>);
}
```

If `TranscribeOptions.trace_id` is `None`, the gateway generates one and includes it in `Started`, `TranscribeResult.telemetry`, and all emitted telemetry. Provider adapters should not generate unrelated trace IDs.

The router must be deterministic:

1. Filter providers by explicit provider/model request when present
2. Filter by capability compatibility
3. Filter by route language/region
4. Apply latency/cost constraints when configured
5. Select lowest `priority`, then stable provider name sort as tie-breaker

If no provider matches, return `no_matching_provider` with the rejected constraints included in `provider_metadata`.

### Runtime Config and Factory

```rust
pub struct AsrProviderRuntimeConfig {
    pub provider: String,
    pub model: Option<String>,
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
    pub languages: Vec<Language>,
    pub input_formats: Vec<AudioFormat>,
    pub streaming: bool,
    pub batch: bool,
    pub interim_results: bool,
    pub endpointing: bool,
    pub word_timestamps: bool,
    pub speaker_diarization: bool,
    pub confidence: bool,
    pub code_switching: bool,
    pub hot_words: bool,
    pub max_duration_ms: Option<u64>,
    pub source: CapabilitySource,
    pub provider_metadata: Value,
}
```

Capabilities may come from provider metadata, static tables, or conservative assumptions. Strict mode must not rely on assumptions for features that affect transcript semantics.

### Observability

```rust
pub struct AsrTelemetry {
    pub trace_id: String,
    pub provider: String,
    pub language: Option<String>,
    pub audio_duration_ms: u64,
    pub latency_first_partial_ms: Option<u64>,
    pub latency_final_ms: u64,
    pub partial_rollback_count: u32,
    pub confidence_avg: Option<f64>,
    pub cost_estimate_micros: Option<u64>,
    pub network_region: Option<String>,
    pub provider_status: Option<u16>,
    pub option_adjustment_count: u32,
}
```

The ASR crate emits its own `AsrEvent` / telemetry stream. It must not require adding ASR-specific variants to core `RuntimeEvent`. Applications can align ASR events with Orchest run events through `trace_id`.

```rust
pub enum AsrEvent {
    RouteSelected { trace_id: String, provider: String, model: String },
    TranscriptionStarted { trace_id: String, provider: String, model: String },
    PartialTranscript { trace_id: String, text: String, stable: bool },
    EndOfSpeech { trace_id: String },
    TranscriptionCompleted { trace_id: String, telemetry: AsrTelemetry },
    TranscriptionFailed { trace_id: String, error: AsrError },
}
```

Spans:

- `asr.gateway.transcribe`
- `asr.gateway.stream`
- `asr.provider.request`
- `asr.provider.stream`
- `asr.router.select`

Metrics:

- request duration
- first partial latency
- final transcript latency
- audio duration
- provider error count by code/status
- partial rollback count
- option adjustment count

Do not log raw audio bytes, signed URLs, API keys, or full transcripts by default. Transcript logging is application policy.

### Error Handling

```rust
pub struct AsrError {
    pub message: String,
    pub code: AsrErrorCode,
    pub provider: Option<String>,
    pub status: Option<u16>,
    pub upstream_code: Option<String>,
    pub upstream_message: Option<String>,
    pub upstream_body: Option<Value>,
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

Provider response bodies should be preserved in errors for debugging, with secrets redacted.

### Provider Scope

Initial provider targets:

| Provider | Priority | Notes |
|----------|----------|-------|
| Deepgram | P0 | First implemented adapter; streaming STT, endpointing, common voice-agent baseline |
| Soniox | P1 | Code-switching and multilingual routing candidate |
| AssemblyAI | P1 | Rich transcript metadata, voice-agent friendly |
| Speechmatics | P2 | Multilingual fallback |
| ElevenLabs Scribe | P2 | Useful when paired with ElevenLabs TTS/Conversational AI stack |

Live provider tests should be gated behind env vars and ignored by default. Fake-provider tests must run in normal CI.

### Feature Flags

| Feature | Contents |
|---------|----------|
| `deepgram` | Deepgram adapter and live tests |
| `soniox` | Soniox adapter skeleton or implementation |
| `assemblyai` | AssemblyAI adapter skeleton or implementation |
| `speechmatics` | Speechmatics adapter skeleton or implementation |
| `elevenlabs` | ElevenLabs Scribe adapter skeleton or implementation |

The default feature set should include only fake/test-safe infrastructure and any provider adapter that does not require extra heavy dependencies. WebSocket dependencies for streaming providers should be feature-gated.

## Not in Scope

- Changes to `agent-runtime-core` run loop
- Adding `AsrPartialTranscript` / `AsrFinalTranscript` to core `RuntimeEvent`
- Continuous voice session orchestration
- Turn-taking policy, barge-in, interruption handling, WebRTC, SIP, Twilio, audio device management
- TTS provider gateway
- `ContentBlock::InputAudio` / native multimodal audio model support
- Treating real-time user speech ASR as a Tool
- Treating ASR as a Skill

## Tool Adapter Boundary

v0.9.3 does not implement an `AsrTool` because this crate must remain standalone and must not depend on `agent-runtime-core`.

A future core wrapper or extension crate may expose ASR as a Tool only for agent-initiated transcription, such as "transcribe this uploaded podcast file." That wrapper must be documented as distinct from real-time user speech input.

Real-time user speech flow remains:

```text
audio input -> ASR Gateway -> final transcript -> AgentRun
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
- Existing workspace conventions from `agent-runtime-providers` and `agent-runtime-aigc-providers`

New provider-specific HTTP/WebSocket dependencies must be feature-gated and justified in the implementation PR.

`anyhow` is not allowed in this library crate. Provider adapters should return `AsrError` with stable codes and preserved upstream details.

## Testing Strategy

Unit tests:

- Fake provider implements one-shot and streaming paths
- Router determinism and tie-break behavior
- Compatibility policy strict/coerce behavior
- Capability validation
- API key resolution hierarchy
- Error redaction preserves debug payload while removing secrets
- Streaming event ordering: `Started` -> zero or more `Partial` / `EndOfSpeech` -> `Final` or fatal `Error`

Integration tests:

- `cargo test -p agent-runtime-asr-providers` runs without live credentials
- Live provider tests are `#[ignore]` and gated by provider-specific env vars
- Deepgram live test, when env vars are set, transcribes a tiny fixture and asserts non-empty text plus telemetry fields

Test fixtures:

- Include only tiny synthetic audio fixtures suitable for repository storage, or generate PCM fixtures in test code.
- Do not commit real user recordings.

## Issue Breakdown

| Issue | Title | Depends on | Scope |
|-------|-------|------------|-------|
| 001 | Crate scaffold + public types | -- | Workspace entry, module layout, `AsrProvider`, request/result/usage/error/capability types |
| 002 | Gateway + router | 001 | `AsrGateway`, deterministic `AsrRouter`, compatibility validation, fake-provider tests |
| 003 | Streaming contract | 001, 002 | `stream_transcribe()` helper, stream item ordering, end-of-speech semantics, backpressure docs |
| 004 | Observability + trace | 001, 002 | `AsrTelemetry`, `AsrEvent`, trace generation/propagation, spans/metrics |
| 005 | Deepgram adapter | 001, 003, 004 | Feature-gated provider implementation, config factory, live ignored test |
| 006 | Provider skeletons + examples | 001, 002 | Soniox/AssemblyAI/Speechmatics/ElevenLabs skeletons, README/example snippets |

## Acceptance Criteria

- [ ] `crates/agent-runtime-asr-providers` exists and is part of the workspace
- [ ] `AsrProvider` trait supports one-shot and streaming transcription
- [ ] Crate has zero workspace-internal dependencies
- [ ] Provider-neutral types exist for audio input, options, result, stream item, usage, telemetry, routing, compatibility and errors
- [ ] `AsrModelCapabilities` exists and strict compatibility checks use it
- [ ] `AsrGateway` can route requests to registered fake providers based on route priority and language
- [ ] Router tie-break behavior is deterministic
- [ ] Streaming contract distinguishes partial transcript, final transcript, end-of-speech and fatal/non-fatal errors
- [ ] Streaming tests assert stable event ordering
- [ ] Telemetry includes `trace_id` and can be correlated with an external Orchest run
- [ ] `AsrError` preserves upstream status/code/message/body with secret redaction
- [ ] API key resolution follows explicit key -> explicit env var -> provider default env var
- [ ] Deepgram adapter or an explicitly chosen equivalent P0 provider adapter is implemented behind a feature flag
- [ ] P1/P2 provider modules exist as feature-gated skeletons or implementations
- [ ] PRD documents that `AsrTool` is out of scope for this standalone crate
- [ ] No changes are required in `agent-runtime-core`
- [ ] `cargo test -p agent-runtime-asr-providers` passes
- [ ] `cargo clippy -p agent-runtime-asr-providers -- -D warnings` passes
- [ ] `cargo fmt --check` passes
