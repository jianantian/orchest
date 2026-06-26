# Issue 002: orchest-protocol spine

## Background

Issue 001 pins the type shapes; this issue builds them. Evolve `agent-runtime-model` into
`orchest-protocol` — the leaf crate every layer and consumer speaks. It carries the content model, the
unified capability traits, the unified event model, the unified capability descriptor, and the unified
error. Old `agent-runtime-model` public types stay as deprecated aliases so nothing downstream breaks yet
(Phase 1, no provider behavior change).

## Goal / Scope

Implement the protocol spine designed in Issue 001.

In scope:

- Keep `ContentBlock` (already modality-complete).
- Capability traits: `ChatModel` (← `ModelAdapter`), `Asr` (← `AsrProvider`), `Tts` (← `TtsProvider`),
  `VoiceManager` (separate optional), and **new** `RealtimeSession` (`send(SessionInput)` + `events()`) and
  `GenTask` (submit/poll/fetch).
- Unified event model: delta-granular core + typed extensions. `StreamEvent` is **extended in place** so
  the existing push-based `ModelAdapter` keeps compiling and behaving as today; the new
  `RealtimeSession` trait exposes the pulled `events()` stream (`GenTask` uses its own submit/poll/fetch
  lifecycle, not `events()`). The push→pull convergence for
  chat/asr/tts is **not** done here (see Out of scope).
- Unified capability descriptor: common queryable core + typed extensions + static catalog form; a single
  `CapabilitySource`.
- Unified error replacing `ModelError`/`AsrError`/`TtsError`/`RealtimeError`.
- Deprecated aliases for the old `agent-runtime-model` public types.

Out of scope:

- No provider impl changes (providers still compile against aliases).
- **No push→pull migration of chat/asr/tts.** `ModelAdapter` (push) is retained unchanged as the compat
  bridge; converging providers onto the pulled traits is owned by Issue 005 (LLM's push `ModelAdapter` →
  `ChatModel`). ASR/TTS/omni are already pull/stream-native, so their move in Issue 006 carries no push bridge.
- No `orchest-provider-core` (Issue 003), no registry/wall (Issue 004).
- No deletion of old modality crates.

## Acceptance Criteria

- [ ] `orchest-protocol` exists (evolved from `agent-runtime-model`); workspace builds.
- [ ] Capability traits incl. `RealtimeSession`/`GenTask` are defined per Issue 001; no god-trait.
- [ ] One event model (core + typed extensions) and one descriptor (core + typed extensions + static form);
      the duplicate `CapabilitySource` is removed.
- [ ] `StreamEvent` is extended in place; existing push-based `ModelAdapter` providers compile and behave
      unchanged — no push→pull migration happens in this issue.
- [ ] One unified error; old error types alias or `From`-convert.
- [ ] Old `agent-runtime-model` public API resolves via deprecated aliases; `agent-runtime-core/node/py`
      compile unchanged.
- [ ] Fake-based omni + Chameleon shape tests compile against the protocol with no provider-local structs.

## Notes

Depends on Issue 001. Behavior-preserving. Keep `agent-runtime-model` as a thin re-export shell during migration.
