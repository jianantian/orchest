# Issue 002 Plan: orchest-protocol spine

## Files to Read

- `docs/iteration/v0_9_12/issues/001-protocol-design/design.md` (the pinned shapes)
- `docs/iteration/v0_9_12/prd.md`, `docs/adr/0001-provider-unification.md`
- `crates/agent-runtime-model/src/*` (all)
- `crates/agent-runtime-asr-providers/src/{traits.rs,types.rs,error.rs,streaming.rs}`
- `crates/agent-runtime-tts-providers/src/{traits.rs,types.rs,error.rs,streaming.rs}`
- `crates/agent-runtime-realtime-providers/src/{error.rs, providers/volcengine/realtime/mod.rs}`
- `crates/agent-runtime-core/src/model/mod.rs` (the re-export surface node/py use)

## Files to Change

- Rename/evolve crate `agent-runtime-model` → `orchest-protocol` (Cargo.toml package name; workspace members; keep `agent-runtime-model` as a thin re-export crate)
- `crates/orchest-protocol/src/*`: traits (chat/asr/tts/voice/realtime/gen), event model, descriptor, unified error
- Deprecated alias modules for old type paths
- Workspace `Cargo.toml`

## Steps

1. Create `orchest-protocol` from `agent-runtime-model` (package rename; add re-export shim crate for the old name).
2. Define the unified error; alias the old errors via `From`.
3. Define the unified event model (core + typed extensions) per `design.md`; map the four old enums in tests.
4. Define the capability descriptor (core + typed extensions + static form); remove the duplicate `CapabilitySource`.
5. Define capability traits: `ChatModel`/`Asr`/`Tts`/`VoiceManager` + new `RealtimeSession`/`GenTask`.
6. Add deprecated aliases for old `agent-runtime-model` public types; verify `core/node/py` compile.
7. Add fake-based omni + Chameleon shape tests.
8. Run fmt / clippy / test on the workspace.
