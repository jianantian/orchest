# Issue 001: Capability descriptor & event-model reconciliation design

## Background

ADR-0001 (`docs/adr/0001-provider-unification.md`) fixes the architecture and the "common core + typed
extensions" decision, but deliberately leaves the concrete type shapes open. Before the `orchest-protocol`
spine (Issue 002) can be built, the unified event model, the capability descriptor, and the new
`RealtimeSession` / `GenTask` trait signatures must be designed concretely and validated against the two
acceptance rulers. Today these exist as **four divergent event enums**, **three descriptor structs plus a
separate catalog layer**, and **two missing traits** (see PRD §Starting Point). This issue is design only —
it pins the shapes so Issue 002 implements without inventing.

## Goal / Scope

Produce a concrete type-shape design note (prose + Rust type sketches) for the protocol spine, proven
against omni + Chameleon.

In scope:

- **Unified event model:** the common delta-granular core (from `StreamEvent`) + typed per-capability
  extensions; how `AudioDelta` / `Transcript{stability}` / `Lifecycle` fit; the push→pull delivery decision
  (today turn pushes via `mpsc::Sender<StreamEvent>`, ASR pulls via `AsrStream`).
- **Capability descriptor:** the common queryable core fields + typed extensions; the **static catalog
  form** (so the registry filters before instantiation); de-dup of `CapabilitySource`; folding the catalog
  modality info (`LlmModelEntry`/`Modality`) into the core.
- **New trait signatures:** `RealtimeSession` (`send(SessionInput)` + `events()`) and `GenTask`
  (submit/poll/fetch), lifted from the concrete `VolcengineRealtimeSession` / `ImageGateway`; confirm the
  `ChatModel`/`Asr`/`Tts` mappings from `ModelAdapter`/`AsrProvider`/`TtsProvider`.

Out of scope:

- No implementation, no new crate (that is Issue 002).
- No registry selection API surface (Issue 004).
- No provider migration / behavior change.

## Acceptance Criteria

- [ ] A design note records the **unified event model** (core + typed extensions) with Rust type sketches;
      every variant of `StreamEvent` / `AsrStreamEvent` / `TtsStreamEvent` / `VolcengineRealtimeEvent` maps onto it.
- [ ] The **capability descriptor** is specified (common core + typed extensions, with a static catalog
      form); `CapabilitySource` de-dup and the catalog-modality fold are covered; `AsrModelCapabilities`-level
      detail is preserved as a typed extension (not flattened).
- [ ] `RealtimeSession` and `GenTask` trait signatures are specified; `ChatModel`/`Asr`/`Tts` mappings from
      the existing traits are confirmed, including the push→pull delivery decision.
- [ ] **Omni** (audio in / audio+text out / mid-stream tool use) and **Chameleon** (turn emits `Image`) are
      each traced through the proposed types with **no** provider-local content/event structs.
- [ ] Open questions that affect Issue 002–007 are recorded explicitly.

## Notes

Gates Issue 002 (spine). Inputs are ADR-0001 and the PRD §Starting Point inventory. Design only — the goal
is to remove all "invent the type here" guesswork from the spine implementation.
