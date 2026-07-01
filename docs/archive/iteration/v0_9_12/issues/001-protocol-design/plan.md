# Issue 001 Plan: Capability descriptor & event-model reconciliation design

## Files to Read

- `docs/iteration/v0_9_12/prd.md` (§Starting Point, §Target, §Decisions)
- `docs/adr/0001-provider-unification.md`
- `crates/agent-runtime-model/src/stream.rs` (`StreamEvent`), `options.rs` (`ModelCapabilities`, `CapabilitySource`), `adapter.rs` (`ModelAdapter`), `types.rs` (`ContentBlock`)
- `crates/agent-runtime-asr-providers/src/types.rs` (`AsrStreamEvent`, `AsrModelCapabilities`, `CapabilitySource` dup), `traits.rs`, `streaming.rs` (`AsrStream`)
- `crates/agent-runtime-tts-providers/src/types.rs` (`TtsModelCapabilities`), `traits.rs`, `streaming.rs` (`TtsStreamEvent`)
- `crates/agent-runtime-providers/src/catalog/mod.rs` (`LlmModelEntry`/`Modality`/`ModelScene`/`ThinkingSpec`)
- `crates/agent-runtime-realtime-providers/src/providers/volcengine/realtime/mod.rs` (`VolcengineRealtimeEvent`, `VolcengineRealtimeSession`)
- `crates/agent-runtime-aigc-providers/src/gateway/image/mod.rs` (`ImageGateway`)

## Files to Change

- Create `docs/iteration/v0_9_12/issues/001-protocol-design/design.md` — the type-shape design note (prose + Rust sketches). No source code in this issue.

## Steps

1. Inventory every variant/field of the four event enums and the three descriptor structs + catalog fields; tabulate them.
2. Draft the unified event **core** (delta granularity) + typed extensions; map each existing variant onto it; flag any variant that is a routing/control concern rather than a content event.
3. Decide push vs pull delivery for the event stream; record the rationale and the migration implication for `ModelAdapter::complete`.
4. Draft the capability descriptor: common queryable core, typed per-capability extensions, and the static catalog representation; de-dup `CapabilitySource`; fold catalog modality info into the core.
5. Draft `RealtimeSession` (`send`/`events`/`SessionInput`) and `GenTask` (submit/poll/fetch) trait signatures; confirm `ChatModel`/`Asr`/`Tts` map from `ModelAdapter`/`AsrProvider`/`TtsProvider`.
6. Trace **omni** and **Chameleon** through the drafted types end to end; confirm no provider-local structs are required.
7. Review against ADR-0001 hard acceptance; record open questions that feed Issues 002–007.
