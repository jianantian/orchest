# Issue 001 Design Note: Protocol Spine Type Shapes

> Status: **pins the shapes** for Issue 002 (`orchest-protocol`). Design only — no source code lands
> here. Every type below is grounded in code that exists today (anchors are `path:line` at the time of
> writing). Decisions that genuinely belong to a later issue are deferred explicitly and listed under
> *Open Questions*.
>
> Inputs: [`prd.md`](../../prd.md) §Starting Point / §Target / §Decisions; [`ADR-0001`](../../../../adr/0001-provider-unification.md)
> Decision 5 ("common core + typed extensions", delta-granular event core, static descriptor core).
>
> Hard acceptance (ADR-0001): the shapes must seat **omni** (full-duplex; audio in / audio+text out;
> mid-stream tool use) and **Chameleon** (a turn that natively emits an `Image`) with **no**
> provider-local content/event structs. Both are traced in §6.

---

## 0. Inventory (what we depart from)

The refactor collapses four parallel stacks. The concrete artifacts today:

### Event enums (four)

| Enum | Anchor | Granularity / variants |
|---|---|---|
| `StreamEvent` | `agent-runtime-model/src/stream.rs:13` | delta: `Text`, `ThinkingStart`/`Thinking`/`ThinkingEnd`, `ToolUseStart`/`ToolUseArgsChunk`/`ToolUseEnd`, `Done{usage}` |
| `AsrStreamEvent` | `agent-runtime-asr-providers/src/types.rs:354` | `RouteSelected`, `Started`, `TranscriptUpdate{stability,update_kind}`, `EndOfSpeech`, `AsrFinal`, `Error{fatal}` |
| `TtsStreamEvent` | `agent-runtime-tts-providers/src/streaming.rs:8` | `RouteSelected`, `Started{voice}`, `TextDelta`, `TextAccepted`, `AudioChunk{data,format,sequence}`, `Completed{summary}`, `Error{fatal}` |
| `VolcengineRealtimeEvent` + `…MappedEvent` | `…/realtime/mod.rs:156,199` | transport: `Handshake`, `SessionStarted`, `AudioInputAccepted`, `ClientInterrupted`, `SessionClosed`, `ProviderError`; mapped: `Lifecycle`, `AudioOutput`, `Transcript{is_interim}`, `ModelText`, `Error`, `Metadata`, `Unsupported` |

### Descriptors (three structs + a catalog layer)

| Descriptor | Anchor | Note |
|---|---|---|
| `ModelCapabilities` | `agent-runtime-model/src/options.rs:87` | `streaming/tool_use/parallel_tool_use/reasoning/prompt_cache/max_output_tokens/context_window_size/source/pricing` — **no input-modality fields** |
| `AsrModelCapabilities` | `agent-runtime-asr-providers/src/types.rs:431` | 20+ ASR-specific fields (endpointing, diarization, `connection_reuse`, hot_words, …) |
| `TtsModelCapabilities` | `agent-runtime-tts-providers/src/types.rs:368` | TTS/voice-specific |
| catalog `LlmModelEntry`/`Modality`/`ModelScene`/`ThinkingSpec` | `agent-runtime-providers/src/catalog/mod.rs:29,48,57,72` | the **static** modality info, lives in a different crate/layer than `ModelCapabilities` |

`CapabilitySource{Static,ProviderMetadata,Assumed}` is **duplicated** verbatim at
`agent-runtime-model/src/options.rs:39` and `agent-runtime-asr-providers/src/types.rs:416`.

### Errors (four)

`ModelError` (`…/model/src/error.rs:21`), `AsrError` (`…/asr/error.rs:27`), `TtsError`
(`…/tts/error.rs:27`), `RealtimeError` (`…/realtime/error.rs:20`). The three satellite errors are
**structurally identical** — `{ message, code: <CodeEnum>, model, status, upstream_code,
upstream_message, upstream_body, diagnostic_metadata }` — with near-identical code enums.
`ModelError` differs only superficially (`code: Option<String>`, `provider` instead of `model`,
`retry_after_secs`, `upstream: Option<Arc<UpstreamErrorDetail>>`).

### Traits / sessions / gateways

| Capability | Today | Anchor | Delivery |
|---|---|---|---|
| chat | `ModelAdapter::complete(.., tx: Option<Sender<StreamEvent>>)` | `…/model/src/adapter.rs:17` | **push** |
| asr | `AsrProvider::{transcribe, start_stream→AsrStream}` | `…/asr/traits.rs:10` | **pull** (`AsrStream{input: AsrAudioSink, events: AsrEventStream}`) |
| tts | `TtsProvider::{synthesize, stream_synthesize→TtsOutputStream, start_duplex_stream→TtsDuplexStream}` + `VoiceManager` | `…/tts/traits.rs:12,39` | **pull** |
| realtime | concrete `VolcengineRealtimeSession` (`start/send_audio_chunk/interrupt/close`, events via `mpsc::Receiver`) | `…/realtime/mod.rs:249` | **pull** events / imperative send |
| gen-task | concrete `ImageGateway::generate()` / `generate_with_events()` | `…/aigc/.../gateway/image/mod.rs:20,54` | submit + optional progress events |

---

## 1. Unified event model

### 1.1 Decision

One **delta-granular content-event core** (`StreamEvent`'s existing granularity, extended in place)
carries everything that is genuinely *content or lifecycle*. Concerns that are **routing/control**
(`RouteSelected`, `Started`, transport handshakes) are **not** content events — they become a small
`Lifecycle` sub-vocabulary, not top-level variants. Per-capability **detail that has no content
meaning** (ASR segment ids, endpointing) rides as a **typed extension** payload, not a core variant.

The core is named `StreamEvent` (kept — Issue 002 extends it in place so push-based `ModelAdapter`
keeps compiling; ADR-0001 Decision 5 and Issue 002 spec both require in-place extension). A
capability-neutral alias `pub use StreamEvent as ProtocolEvent;` is provided for new pull-side code so
call sites read naturally; they are the same type.

### 1.2 Core shape (sketch)

```rust
// orchest-protocol::event
pub enum StreamEvent {
    // --- text / thinking (unchanged from today) ---
    Text { delta: String },
    ThinkingStart,
    Thinking { delta: String },
    ThinkingEnd { signature: Option<String>, provider_details: Option<Value> },

    // --- tool use (unchanged delta granularity — load-bearing for omni) ---
    ToolUseStart { id: String, name: String },
    ToolUseArgsChunk { id: String, delta: String },
    ToolUseEnd { id: String },

    // --- NEW: audio output (duplex/tts) ---
    AudioDelta { data: Bytes, format: AudioFormat, sequence: u64 },

    // --- NEW: transcript (asr/omni) — interim vs committed ---
    Transcript { text: String, stability: TranscriptStability, segment: Option<SegmentRef> },

    // --- NEW: image/video emitted mid-turn (Chameleon) ---
    //     reuses the *content* model, not a new struct
    Content { block: ContentBlock },

    // --- NEW: lifecycle (routing/session/control, capability-neutral) ---
    Lifecycle(LifecycleEvent),

    // --- NEW: in-band error (non-fatal or fatal) ---
    Error { error: ProtocolError, fatal: bool },

    // --- terminal (extended: usage stays; realtime/asr/tts carry their summary here) ---
    Done { usage: TokenUsage },

    // --- typed per-capability extension escape hatch (NOT a god-payload) ---
    //     only ASR-segment/endpointing-class detail with no core meaning rides here
    Extension(CapabilityEventExt),
}

pub enum LifecycleEvent {
    RouteSelected { provider: String, model: String, trace_id: String },
    SessionStarted { session_id: String },
    EndOfSpeech { segment: Option<SegmentRef> },
    Interrupted,                // client- or server-initiated barge-in
    SessionClosed,
}

pub struct SegmentRef { pub segment_id: Option<String>, pub update_kind: TranscriptUpdateKind }

// Typed, *closed* per-capability extension — one variant per capability that needs it.
pub enum CapabilityEventExt {
    Asr(AsrEventExt),   // endpointing/diarization-class detail with no core analogue
    // Tts(..) / Realtime(..) added only if a real need appears; today none required.
}
```

`AudioFormat`, `TranscriptStability`, `TranscriptUpdateKind` are lifted from the asr/tts crates into
`orchest-protocol` (they are pure content-shape enums). `Bytes` keeps the asr/tts `bytes::Bytes`
choice (zero-copy audio).

### 1.3 Mapping table — every old variant lands

| Old variant | → core |
|---|---|
| `StreamEvent::Text/Thinking*/ToolUse*` | **identical** (unchanged) |
| `StreamEvent::Done{usage}` | `Done{usage}` (unchanged) |
| `AsrStreamEvent::RouteSelected` | `Lifecycle(RouteSelected)` |
| `AsrStreamEvent::Started` | `Lifecycle(RouteSelected)` (model/trace) — `Started` is the asr "route locked" signal |
| `AsrStreamEvent::TranscriptUpdate{text,stability,segment_id,update_kind}` | `Transcript{text, stability, segment: Some(SegmentRef{segment_id, update_kind})}` |
| `AsrStreamEvent::EndOfSpeech{segment_id}` | `Lifecycle(EndOfSpeech{segment})` |
| `AsrStreamEvent::AsrFinal{final_output}` | `Transcript{stability: Committed, ..}` (final text) **+** `Done{usage}` (final carries duration usage; see §2.4); rich `AsrFinalOutput` detail → `Extension(Asr(Final{..}))` |
| `AsrStreamEvent::Error{error,fatal}` | `Error{error, fatal}` |
| `TtsStreamEvent::RouteSelected` | `Lifecycle(RouteSelected)` |
| `TtsStreamEvent::Started{voice}` | `Lifecycle(RouteSelected)` + voice → `Extension`/metadata (voice id is detail) |
| `TtsStreamEvent::TextDelta` | `Text{delta}` (echoed input text) |
| `TtsStreamEvent::TextAccepted{chars}` | `Lifecycle`/dropped — pure backpressure ack, not content (see Open Q3) |
| `TtsStreamEvent::AudioChunk{data,format,sequence}` | `AudioDelta{data, format, sequence}` |
| `TtsStreamEvent::Completed{summary}` | `Done{usage}` (summary → usage/details) |
| `TtsStreamEvent::Error{error,fatal}` | `Error{error, fatal}` |
| `VolcengineRealtimeEvent::Handshake` | `Lifecycle(SessionStarted)` (pre-start) |
| `…::SessionStarted` | `Lifecycle(SessionStarted)` |
| `…::AudioInputAccepted{bytes}` | dropped (local ack — not server content; see Open Q3) |
| `…::ClientInterrupted` / mapped `Interrupted` | `Lifecycle(Interrupted)` |
| `…::SessionClosed` | `Lifecycle(SessionClosed)` |
| `…::ProviderError` / mapped `Error` | `Error{fatal: true}` |
| `…MappedEvent::Lifecycle{name}` | `Lifecycle(..)` (named server lifecycle) |
| `…MappedEvent::AudioOutput{bytes}` | `AudioDelta{data, format, sequence}` |
| `…MappedEvent::Transcript{text,is_interim}` | `Transcript{text, stability: is_interim?Provisional:Committed}` |
| `…MappedEvent::ModelText{content}` | `Text{delta: content}` |
| `…MappedEvent::Metadata{name,payload}` | `Extension` or `Lifecycle` w/ payload |
| `…MappedEvent::Unsupported{name,reason}` | `Extension` (diagnostic) — never silently dropped |

No old variant requires a provider-local struct: every payload is either core, lifecycle, or a typed
extension defined **in the protocol crate**.

### 1.4 Delivery: push → pull (decision + migration boundary)

**Target = a pulled `events()` stream** (ADR-0001 Decision 5: "delivery unifies on a pulled `events()`
stream"). ASR/TTS/realtime are already pull-native; only chat (`ModelAdapter`) pushes via
`tx: Option<mpsc::Sender<StreamEvent>>`.

The migration is **staged, not done in the spine**:

- **Issue 002 (spine):** `StreamEvent` is extended **in place**. The push-based `ModelAdapter` is
  retained unchanged as the compat bridge and keeps behaving exactly as today. The **new**
  `RealtimeSession` trait is born pull-native (`events()`); `GenTask` uses its own submit/poll/fetch
  lifecycle (not `events()`).
- **Issue 005 (LLM):** owns the chat push→pull convergence — LLM providers move from `complete(.., tx)`
  onto the pulled `ChatModel::events()`; `ModelAdapter` stays a working bridge until Issue 008 removes it.
- **Issue 006 (asr/tts/omni):** already pull/stream-native; no push bridge to carry.

Pull stream type (sketch — concrete handle naming settled in 002):

```rust
pub struct EventStream { /* wraps mpsc::Receiver<StreamEvent> */ }
impl EventStream { pub async fn next(&mut self) -> Option<StreamEvent>; }
```

The existing `AsrEventStream`/`TtsEventStream`/`AsrStream.events` are the prototypes; the spine
generalizes them to `EventStream<StreamEvent>`.

---

## 2. Capability descriptor

### 2.1 Decision

A **common queryable core** (`CapabilityDescriptor`) holds only what the **registry filters on**, plus
a **typed per-capability extension** that preserves full detail. The core must be available
**statically** (catalog data) so the registry filters *before* instantiating a provider. The single
`CapabilitySource` is moved to `orchest-protocol`; the asr duplicate is deleted.

### 2.2 Common core (sketch)

```rust
// orchest-protocol::descriptor
pub struct CapabilityDescriptor {
    pub provider: &'static str,
    pub model: &'static str,
    pub capability: Capability,                 // Chat | Asr | Tts | Realtime | GenTask | VoiceMgmt
    pub input_modalities:  ModalitySet,         // folds catalog `input_modalities`
    pub output_modalities: ModalitySet,         // folds catalog `output_modalities`
    pub streaming: bool,
    pub tools: bool,                            // ModelCapabilities.tool_use
    pub thinking: bool,                         // catalog `thinking.is_some()` ∪ ModelCapabilities.reasoning.supported
    pub duplex: bool,                           // bidirectional realtime / tts-duplex / asr-streaming
    pub interruptible: bool,                    // realtime barge-in
    pub source: CapabilitySource,
    pub ext: CapabilityExt,                     // typed per-capability detail (below)
}

pub enum Capability { Chat, Asr, Tts, Realtime, GenTask, VoiceManagement }

// `Modality` (catalog) is lifted to protocol; ModalitySet is a small bitset/Vec.
pub enum Modality { Text, Image, Video, Audio }
```

`tools`/`thinking`/`context_window` come from `ModelCapabilities` + catalog `LlmModelEntry`; the
**modality bit that was missing from `ModelCapabilities`** is now in the core (folded from the catalog
layer), so a query like `.accepts([Image,Video]).thinking()` reads **one** struct, not two crates.

### 2.3 Typed extensions (detail preserved, never flattened)

```rust
pub enum CapabilityExt {
    Chat(ChatCapabilityExt),     // reasoning efforts, prompt_cache, parallel_tool_use, pricing, scenes
    Asr(AsrModelCapabilities),   // the EXISTING struct, moved verbatim — 20+ fields survive
    Tts(TtsModelCapabilities),   // existing struct, moved verbatim
    Realtime(RealtimeCapabilityExt),
    GenTask(GenTaskCapabilityExt),
    None,
}
```

`AsrModelCapabilities` (endpointing/diarization/connection_reuse/hot_words/…) is **moved verbatim** as
the `Asr` extension — the PRD risk "descriptor flattening" is mitigated by *not* flattening: the core is
only the queryable subset; the full struct rides as `ext`.

### 2.4 Static catalog form

The catalog (`LlmModelEntry` today) becomes the **static** producer of `CapabilityDescriptor`s:

```rust
pub trait CatalogEntry {
    fn descriptor(&self) -> CapabilityDescriptor;   // const-friendly; no network, no credentials
}
```

Each impl crate contributes a `&'static [CapabilityDescriptor]` (or a builder) so Issue 004's registry
filters statically. `LlmModelEntry`'s `input_modalities`/`output_modalities`/`thinking`/`pricing` fold
into the descriptor core + `Chat` ext; `Modality`/`ModelScene`/`ThinkingSpec` move to protocol.

### 2.5 `CapabilitySource` de-dup

One definition in `orchest-protocol::descriptor`: `enum CapabilitySource { Static, ProviderMetadata,
Assumed }`. `agent-runtime-asr-providers`'s copy (`types.rs:416`) is removed and re-exported from the
spine during migration.

---

## 3. Unified error

The three satellite errors are structurally identical; `ModelError` is a near-twin. One protocol error:

```rust
// orchest-protocol::error
pub struct ProtocolError {
    pub message: String,
    pub code: ErrorCode,                 // unified enum (superset below)
    pub provider: Option<String>,        // ModelError.provider ∪ {Asr,Tts,Realtime}.model
    pub model: Option<String>,
    pub status: Option<u16>,
    pub retry_after_secs: Option<u64>,   // from ModelError (HTTP Retry-After)
    pub upstream: Option<Arc<UpstreamErrorDetail>>,  // {code,message,body}
    pub diagnostic_metadata: Value,      // from the satellite errors
}

pub enum ErrorCode {
    // auth
    MissingApiKey, InvalidApiKey,
    // routing/identity
    UnknownProvider, UnknownModel, NoMatchingProvider,
    // capability/validation
    UnsupportedOperation, UnsupportedOption, UnsupportedLanguage, UnsupportedAudioFormat,
    InvalidAudio, InvalidRequest,
    // transport/provider
    ProviderHttpError, ProviderStreamError, ProviderTaskFailed,
    Timeout, Cancelled,
    // chat-side
    ContentFilter, ContextWindowExceeded,
    Internal,
    Other(String),
}
```

The enum is the **union** of `AsrErrorCode`/`TtsErrorCode`/`RealtimeErrorCode` (identical lists; tts adds
`UnsupportedVoice`/`InvalidText`/`InvalidVoice` → fold into `UnsupportedOption`/`InvalidRequest` or keep
as variants) plus `ModelError`'s `code: Option<String>` (→ `Other`/`Internal`). Old errors become
deprecated aliases with `From<ProtocolError>` / `From<Old> for ProtocolError` shims (Issue 002), so
`ModelError::internal(..)` call sites keep compiling.

---

## 4. Capability traits

Parallel, opt-in, no god-trait. Confirmed mappings + two new traits.

```rust
// chat ← ModelAdapter (move + rename; ModelAdapter kept as deprecated alias)
#[async_trait]
pub trait ChatModel: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn descriptor(&self) -> CapabilityDescriptor;      // was capabilities()->ModelCapabilities
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,         // push retained until Issue 005 adds events()
    ) -> Result<ModelResponse, ProtocolError>;
}

// asr ← AsrProvider (delivery already pull)
#[async_trait]
pub trait Asr: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn descriptor(&self) -> CapabilityDescriptor;
    fn supported_languages(&self) -> &[Language];
    async fn transcribe(&self, req: TranscribeRequest) -> Result<TranscribeResult, ProtocolError>;
    async fn start_stream(&self, req: StreamingTranscribeRequest) -> Result<AsrStream, ProtocolError>;
}

// tts ← TtsProvider; VoiceManager stays a SEPARATE optional trait (not merged)
#[async_trait]
pub trait Tts: Send + Sync {
    fn provider_name(&self) -> &str; fn model_name(&self) -> &str;
    fn descriptor(&self) -> CapabilityDescriptor;
    async fn synthesize(&self, req: SynthesizeRequest) -> Result<SynthesizeResult, ProtocolError>;
    async fn stream_synthesize(&self, req: SynthesizeRequest) -> Result<EventStream, ProtocolError>;
    async fn start_duplex_stream(&self, req: DuplexSynthesizeRequest) -> Result<DuplexStream, ProtocolError>;
}
pub trait VoiceManager: Send + Sync { /* clone_voice / design_voice / delete_voice — unchanged */ }
```

### 4.1 `RealtimeSession` (NEW — lifted from `VolcengineRealtimeSession`)

The concrete session's surface (`start`, `send_audio_chunk`, `interrupt`, `close`, events via
`mpsc::Receiver`) abstracts to a **send side = `SessionInput`** + **receive side = pulled `events()`**:

```rust
#[async_trait]
pub trait RealtimeSession: Send + Sync {
    async fn send(&self, input: SessionInput) -> Result<(), ProtocolError>;
    fn events(&mut self) -> &mut EventStream;          // pulled unified StreamEvent stream
    async fn close(&mut self) -> Result<(), ProtocolError>;
}

pub enum SessionInput {
    Audio(Bytes),                  // ← send_audio_chunk
    Text(String),                  // ← text input mode
    ToolResult { tool_use_id: String, content: Value },   // mid-stream tool result (omni)
    Interrupt,                     // ← interrupt(PushToTalk)
}
```

`SessionInput::ToolResult` is the load-bearing addition that lets the omni ruler send a tool result back
**without blocking the audio stream** (see §6.1). Construction (`start`) is a factory concern (the
session is handed back already started, matching `start_stream`/`start_duplex_stream`).

### 4.2 `GenTask` (NEW — lifted from `ImageGateway`)

`ImageGateway::generate()` / `generate_with_events()` abstract to a submit/poll/fetch lifecycle (image
**and** video share it; the aigc crate already has a parallel video gateway):

```rust
#[async_trait]
pub trait GenTask: Send + Sync {
    fn provider_name(&self) -> &str; fn model_name(&self) -> &str;
    fn descriptor(&self) -> CapabilityDescriptor;
    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError>;
    async fn poll(&self, handle: &GenHandle) -> Result<GenStatus, ProtocolError>;   // Pending|Running|Done|Failed
    async fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError>;  // assets (URL/base64/stored)
}
```

`GenTask` does **not** use `events()` — its progress (`ImageGenerationEvent`) is exposed via an optional
progress channel on `submit`, but the lifecycle is submit/poll/fetch, not a content-event stream
(Issue 002 spec is explicit). The concrete `generate()` is reconstructable as
`submit` → poll-loop → `fetch` in Issue 007.

---

## 5. Where each type lives (for Issue 002)

| Type | Module in `orchest-protocol` |
|---|---|
| `ContentBlock`, `MediaSource`, `Message`, `Role`, `ToolDef` | `content` (kept, modality-complete) |
| `StreamEvent` (+ `LifecycleEvent`, `SegmentRef`, `CapabilityEventExt`), `EventStream` | `event` |
| `CapabilityDescriptor`, `Capability`, `Modality`, `CapabilitySource`, `CapabilityExt`, catalog form | `descriptor` |
| `ProtocolError`, `ErrorCode`, `UpstreamErrorDetail` | `error` |
| `ChatModel`/`Asr`/`Tts`/`VoiceManager`/`RealtimeSession`/`GenTask`, `SessionInput` | `capability` (one submodule per trait) |
| `RequestOptions`, `ThinkingLevel`, pricing | `options` (kept) |
| deprecated aliases (`ModelAdapter`, `ModelError`, `ModelCapabilities`, …) | `compat` (re-exports) |

---

## 6. Acceptance ruler traces

### 6.1 Omni full-duplex (audio in / audio+text out / mid-stream tool use)

1. Factory returns a `Box<dyn RealtimeSession>` (openspeech dialect, Issue 006), already started.
2. Caller spawns a reader on `session.events()` and a writer feeding mic audio via
   `session.send(SessionInput::Audio(bytes))`.
3. Server streams concurrently → reader observes, interleaved:
   - `StreamEvent::AudioDelta{..}` (TTS-side audio out) — **never blocked**,
   - `StreamEvent::Transcript{stability: Provisional, ..}` (user speech),
   - `StreamEvent::Text{delta}` (model text out),
   - `StreamEvent::ToolUseStart{id,name}` → `ToolUseArgsChunk{id,delta}*` → `ToolUseEnd{id}`.
4. On `ToolUseEnd`, the caller runs the tool **on a separate task**; audio keeps flowing because the
   reader/writer/tool-runner are independent tasks over the one `EventStream` + `send()`.
5. Tool result returns via `session.send(SessionInput::ToolResult{tool_use_id, content})` — same send
   channel as audio, so it does not stall audio.
6. Barge-in: `session.send(SessionInput::Interrupt)`; server emits `Lifecycle(Interrupted)`.

No provider-local content/event type is touched: audio = `AudioDelta`, tool calls = the **existing
delta `ToolUse*`** variants, tool result = `SessionInput::ToolResult` + `ContentBlock::ToolResult`
shape. ✔

### 6.2 Chameleon (a turn natively emits an `Image`)

1. A `ChatModel` (REST/SSE dialect) runs `complete(..)`; its event stream is `StreamEvent`.
2. The model emits an image inline → `StreamEvent::Content{ block: ContentBlock::Image{ source, detail } }`
   — reusing the **content model**, no `GenTask`, no provider-local struct.
3. `ModelResponse.content` ends with `ContentBlock::Image{..}` alongside any `Text`.

The `Content{block}` core variant is exactly the seam that lets a turn carry any `ContentBlock`
(`Image`/`Video`/`Audio`) without a separate generation capability. ✔

---

## 7. Open questions (feed Issues 002–007)

1. **`EventStream` concrete handle (002).** Newtype over `mpsc::Receiver<StreamEvent>` vs. a
   `futures::Stream` impl. Leaning newtype-with-`next()` to match the existing
   `AsrEventStream`/`TtsEventStream` and avoid a `futures` dep in the leaf crate. Settle in 002.
2. **`Done` for non-chat (002/006).** ASR `AsrFinalOutput`/TTS `TtsStreamSummary` carry
   duration/asset usage, not `TokenUsage`. Either widen `Done` to `Done{usage: Usage}` with a `Usage`
   enum (token/duration/asset) or attach summaries via `Extension`. Ties into pricing reconciliation
   (Issue 007). **Recommendation:** widen to a `Usage` enum in 002 so 006/007 don't re-split.
3. **Pure-ack events (`TextAccepted`, `AudioInputAccepted`) (006).** These are backpressure/local acks,
   not server content. Proposed: drop from the unified stream (caller uses send-side `Result` for
   backpressure). Confirm no consumer depends on them when porting tests in 006.
4. **`SessionInput` construction/`start` (002/006).** Whether `RealtimeSession` is always handed back
   started (factory does `start`) or exposes `start()`. Leaning factory-started for parity with
   `start_stream`. Confirm against the openspeech handshake in 006.
5. **Registry surface (004).** This note deliberately stops at the **static catalog form**
   (`CatalogEntry::descriptor()`); the fluent selection builder (`registry.chat().accepts([..]).pick()`)
   is ADR-0001 Decision 4, owned by Issue 004.
6. **`GenTask` progress channel (007).** Whether `submit` takes an optional
   `mpsc::Sender<GenProgress>` (matching `generate_with_events`) or progress is poll-only. Leaning
   optional channel to preserve `emit_partial_images`. Settle in 007.
7. **`ModelScene` placement (002/004).** `scenes` is an LLM discovery aid, not a hard capability filter.
   Proposed: keep in `ChatCapabilityExt`, not the core. Confirm the registry doesn't need to filter on
   it in 004.

These are the only "invent the type here" gaps remaining; §1–§6 remove the rest for Issue 002.
