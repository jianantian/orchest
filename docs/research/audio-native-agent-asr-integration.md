# Audio-Native Agent — ASR Integration Architecture

**Status**: draft — architectural exploration for integrating ASR into the Orchest agent runtime to support audio-native agent products.
**Context**: follows [ASR Vendor Landscape](./asr-vendor-landscape.md), which established the multi-vendor ASR evaluation framework for emerging-market mobile IME.

---

## 1. What "Audio-Native Agent" Means

Three interpretations, in increasing order of Orchest involvement:

| Level | Description | Orchest's Role |
|-------|-------------|---------------|
| **A. ASR as pre-processing** | Audio → external ASR → text → `AgentRun::new(text)`. Orchest never sees audio. | Zero changes. ASR is caller-side plumbing. |
| **B. ASR Gateway** | A provider-abstraction crate that normalizes across ASR vendors, feeds text into Orchest. Orchest gets structured ASR observability. | New crate (`agent-runtime-asr-providers`), following the AIGC Gateway pattern (v0.6.1). Core runtime unchanged. |
| **C. Multimodal agent** | Models that natively accept audio (GPT-4o, Gemini) receive audio `ContentBlock`s directly. Text-only models fall back to ASR pre-processing. | `ContentBlock` and `ModelAdapter` extensions. Core runtime aware of audio as a modality. |

A production "audio-native agent product" typically needs **B + C**: an ASR Gateway for vendor routing and cost control, plus multimodal model support for latency-sensitive voice interactions.

---

## 2. Recommended Architecture

```
                          ┌──────────────────────────┐
                          │    Audio Input Sources     │
                          │  mic stream · file upload  │
                          │  WebSocket · phone call    │
                          └────────────┬─────────────┘
                                       │ raw audio
                          ┌────────────▼─────────────┐
                          │    ASR Gateway             │
                          │  agent-runtime-asr-        │
                          │  providers (new crate)     │
                          │                            │
                          │  AsrProvider trait:        │
                          │   - transcribe(batch)      │
                          │   - stream_transcribe()    │
                          │                            │
                          │  Multi-vendor routing:     │
                          │   Soniox / Deepgram /      │
                          │   ElevenLabs / Speechmatics│
                          │   / AssemblyAI / ...       │
                          │                            │
                          │  Observability:            │
                          │   latency, $/1K words,     │
                          │   rollback rate, WER       │
                          └────────────┬─────────────┘
                                       │ streaming text chunks
                                       │ + final transcript
                          ┌────────────▼─────────────┐
                          │   Orchest Agent Runtime    │
                          │   (unchanged text I/O)     │
                          │                            │
                          │   AgentRun::new(text)      │
                          │   → agent loop             │
                          │   → RuntimeEvent stream    │
                          └────────────┬─────────────┘
                                       │ text response
                          ┌────────────▼─────────────┐
                          │   TTS Gateway (future)     │
                          │   agent-runtime-tts-       │
                          │   providers                │
                          └────────────┬─────────────┘
                                       │ audio
                          ┌────────────▼─────────────┐
                          │    Audio Output            │
                          └──────────────────────────┘
```

**Key principle**: Orchest core stays text-in/text-out. ASR is an input adapter layer that lives *before* the agent loop. This respects the "极简 core" design principle — the runtime does loop + state + events, not modality conversion.

### 2.1 Why ASR is NOT a Tool

A Tool is something the *model decides to call* during the agent loop. ASR happens *before* the model sees any text — it's input preprocessing, not an agent action. Making ASR a Tool would mean:

```
User speaks → ??? → model receives audio → model calls transcribe tool → model sees text
```

This adds a full LLM round-trip just to transcribe, and the model has no way to "hear" the user until it calls the tool. Wrong abstraction.

**Exception**: ASR-as-Tool is correct for *agent-initiated* transcription — e.g., "transcribe this podcast episode I uploaded." This is a distinct use case from user speech input.

### 2.2 Why ASR is NOT a Skill

A Skill is procedural knowledge — instructions for *how* to do something. ASR is a capability — *what* the system can do. An ASR Skill would be a SKILL.md saying "use the ASR Gateway to transcribe audio," which is circular. The ASR Gateway is the capability; a Skill might *consume* it.

---

## 3. ASR Gateway Crate Design

Follows the pattern established by `agent-runtime-aigc-providers` (v0.6.1):

```
crates/agent-runtime-asr-providers/
├── Cargo.toml
├── src/
│   ├── lib.rs              # re-exports
│   ├── traits.rs           # AsrProvider trait
│   ├── types.rs            # AsrRequest, AsrResponse, AsrConfig
│   ├── streaming.rs        # StreamingTranscript, PartialResult
│   ├── routing.rs          # AsrRouter, vendor selection
│   ├── observability.rs    # latency, cost, rollback metrics
│   └── providers/
│       ├── mod.rs
│       ├── deepgram.rs
│       ├── soniox.rs
│       ├── elevenlabs.rs
│       ├── speechmatics.rs
│       ├── assemblyai.rs
│       └── ...
└── tests/
    ├── live_asr_gateway.rs
    └── ...
```

### 3.1 Core Trait

```rust
#[async_trait]
pub trait AsrProvider: Send + Sync {
    /// Provider identifier for routing and telemetry.
    fn provider_name(&self) -> &str;

    /// Supported languages (BCP-47). Empty = vendor-default.
    fn supported_languages(&self) -> &[Language];

    /// One-shot transcription. Returns final text + metadata.
    async fn transcribe(
        &self,
        audio: AudioInput,
        options: &TranscribeOptions,
    ) -> Result<TranscribeResult, AsrError>;

    /// Streaming transcription. Returns a stream of partial + final results.
    async fn stream_transcribe(
        &self,
        audio: AudioStream,
        options: &TranscribeOptions,
    ) -> Result<TranscribeStream, AsrError>;
}
```

### 3.2 Key Types

```rust
pub struct TranscribeOptions {
    pub language: Option<Language>,        // BCP-47 hint
    pub model: Option<String>,             // provider-specific model
    pub hot_words: Vec<String>,            // contacts, brands, places
    pub code_switching: bool,              // enable mixed-language
    pub punctuate: bool,
    pub interim_results: bool,             // partial transcripts
    pub vad_config: Option<VadConfig>,     // voice activity detection
}

pub struct TranscribeResult {
    pub text: String,
    pub language: Option<Language>,
    pub confidence: Option<f64>,
    pub words: Vec<WordTimestamp>,
    pub duration_ms: u64,                  // audio duration
    pub provider_latency_ms: u64,          // time to first result
}

pub enum TranscribeStreamItem {
    Partial { text: String, stable: bool },
    Final { result: TranscribeResult },
    Error { message: String, fatal: bool },
}
```

### 3.3 Multi-Vendor Router

```rust
pub struct AsrRouter {
    providers: HashMap<String, Arc<dyn AsrProvider>>,
    routes: Vec<AsrRoute>,
}

pub struct AsrRoute {
    pub languages: Vec<Language>,          // match on language
    pub networks: Vec<NetworkRegion>,      // match on user region
    pub priority: u8,                      // lower = preferred
    pub provider: String,                  // provider name key
}
```

The router selects a provider based on: language, user network region, cost ceiling, and latency requirements. This implements the routing strategy from the vendor landscape document without hardcoding it into the agent loop.

### 3.4 Observability

Each transcription emits structured telemetry that feeds back into vendor selection:

```rust
pub struct AsrTelemetry {
    pub provider: String,
    pub language: String,
    pub audio_duration_ms: u64,
    pub latency_first_token_ms: u64,
    pub latency_final_ms: u64,
    pub partial_rollback_count: u32,
    pub confidence_avg: f64,
    pub cost_estimate_micros: u64,
    pub network_region: String,
}
```

This is the data that tells you whether Deepgram really beats Soniox for Nigerian English in weak-network conditions — not spec sheets, actual telemetry from production traffic.

---

## 4. Multimodal Model Support (Future)

For models that natively accept audio (GPT-4o `audio_input`, Gemini 2.5, Qwen-Audio), the `ContentBlock` type needs an audio variant so the `ModelAdapter` can pass audio directly:

```rust
pub enum ContentBlock {
    Text(String),
    Thinking { .. },
    ToolUse { .. },
    ToolResult { .. },
    // New:
    InputAudio {
        data: Vec<u8>,
        format: AudioFormat,    // "wav", "mp3", "pcm16"
    },
}
```

This is separate from the ASR Gateway. The ASR Gateway handles *text-only* models. Multimodal models skip the ASR Gateway entirely — they receive audio directly. The system needs both paths:

```
Audio Input
    ├─→ Multimodal model (GPT-4o, Gemini)
    │       → Native audio understanding
    │       → No transcription latency
    │       → Higher per-token cost
    │
    └─→ ASR Gateway → Text model (Claude, etc.)
            → Text-only understanding
            → Transcription latency added
            → Lower per-token cost
```

**Route selection**: the `ModelAdapter` already knows its `ModelCapabilities`. A future `ModelCapabilities::accepts_audio` flag tells the caller whether to route through the ASR Gateway or pass audio directly.

---

## 5. Streaming Audio → Streaming Agent Loop

The hardest integration problem: continuous microphone audio → partial ASR results → agent loop.

### 5.1 Turn-Taking Model (Voice Agent)

```
User starts speaking
    ↓
ASR Gateway produces partial transcripts
    ↓ (streaming)
Agent receives partial text, decides when to respond
    ↓
Model generates response
    ↓ (streaming)
TTS converts to speech
    ↓
User hears response, can interrupt (barge-in)
```

This requires an orchestrator *outside* the current `AgentRun::new(input: String)` model. The current run loop is request-response: one input, one run, one output. A voice agent needs a continuous session with interruptibility.

### 5.2 Integration Points

| Integration | What Changes | Complexity |
|------------|-------------|-----------|
| ASR → text → `AgentRun` | Zero. Caller feeds transcribed text. | Low |
| ASR streaming → partial text → agent | Caller decides end-of-speech, then calls `AgentRun` with final text. Partial transcripts can be shown in UI but don't enter agent loop. | Low-Medium |
| Continuous voice session | New orchestration layer outside core runtime. Manages turn-taking, barge-in, audio I/O. Calls `AgentRun` for each turn. | High |
| Audio `ContentBlock` in messages | Extend `Message`/`ContentBlock` types. `ModelAdapter` passes audio through for multimodal models. | Medium |

### 5.3 Recommendation: Start at Layer 2, Defer Layer 3

For the first iteration:

1. **ASR Gateway crate** — provider abstraction + routing + observability
2. **ASR streaming → final text → AgentRun** — caller handles end-of-speech detection; Orchest receives clean text
3. **Partial transcripts** — emitted as `RuntimeEvent` variants for UI, not injected into agent loop

The continuous voice session (Layer 3) is a product-level concern that should live in the application layer consuming Orchest, not inside Orchest core. Orchest provides the agent runtime; the application manages the audio session lifecycle.

---

## 6. Mapping to Orchest Roadmap

| Component | Where It Fits | Precedent |
|-----------|--------------|-----------|
| `agent-runtime-asr-providers` crate | New satellite crate (like v0.6.1 AIGC Gateway) | `agent-runtime-aigc-providers` |
| `AsrProvider` trait | Provider abstraction pattern | `ImageProvider` in AIGC Gateway |
| Multi-vendor routing | Provider routing + telemetry | Multi-provider model routing (research) |
| `ContentBlock::InputAudio` | Core types extension (v0.9+) | None yet — new modality |
| Continuous voice session | Application layer (not Orchest core) | N/A — product concern |

**Suggested sequencing**:

1. **Now**: ASR Gateway crate as a satellite — independent, doesn't block v0.7/v0.8/v0.9 mainline
2. **v0.9+**: `ContentBlock::InputAudio` when multimodal model support is prioritized
3. **Post-v0.9**: Voice session orchestration in application/example layer

---

## 7. What to NOT Build into Orchest Core

These are application-layer concerns, not runtime concerns:

- **End-of-speech detection (VAD)** — ASR vendors provide this; the ASR Gateway normalizes it
- **Audio format conversion** — caller-side concern; the ASR Gateway accepts common formats
- **TTS (text-to-speech)** — a separate gateway, not part of ASR
- **Barge-in / interruption handling** — application session management
- **Audio device management (mic selection, gain control)** — platform/OS concern
- **Call/WebSocket signaling (SIP, Twilio, WebRTC)** — infrastructure, not runtime

---

## 8. Open Questions

1. **Should ASR Gateway be a separate crate or integrated into `agent-runtime-providers`?** Separate crate is cleaner — different trait, different dependency tree, different release cadence. Same pattern as AIGC Gateway being separate.

2. **Should `TranscribeStreamItem` integrate with `RuntimeEvent`?** For now, ASR telemetry should be its own event type, not injected into the `RuntimeEvent` enum. The agent loop only sees text; ASR observability is a parallel channel.

3. **How does ASR routing interact with model routing?** They're independent decisions. ASR routing picks the transcription provider. Model routing picks the LLM provider. The only coupling: if the model is multimodal, skip ASR entirely.

4. **Does the AIGC Gateway's job/asset/storage infrastructure generalize to ASR?** Audio assets (uploaded files, cached transcripts) share the same storage and lifecycle concerns as image assets. The job polling pattern (async transcription for long audio) also generalizes. Consider extracting shared infrastructure into a common crate (`agent-runtime-media-common`?).
