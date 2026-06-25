# Issue 008: Cleanup + bindings

## Background

With all providers behind the wall, remove the deprecated re-export crates/shims, finalize the feature
graph, update `node/py` bindings + examples to the new surface, and record the dependency-weight evidence.
This is the breaking-removal phase; every earlier phase kept old paths alive.

## Goal / Scope

In scope:

- Remove the deprecated re-export shells (`agent-runtime-{model,providers,asr,tts,aigc}` and the deleted
  `realtime`) once consumers are migrated.
- Update `node/py` to the new construction surface (drop the deprecated `create_adapter_from_config` /
  `normalize_provider_model` shims) + update examples.
- Finalize feature flags; record `cargo tree --features llm` evidence (no `tokio-tungstenite`/OSS).
- Update docs and `roadmap.md` (mark v0.9.12 done).

Out of scope: any new behavior.

## Acceptance Criteria

- [ ] Deprecated re-export shells are removed; the workspace builds with only `orchest-*` provider crates
      (+ protocol/core).
- [ ] `node/py` and examples use the new surface; their tests pass.
- [ ] `cargo tree -e features -p <consumer> --features llm` shows no `tokio-tungstenite`/OSS — evidence
      recorded in the iteration note.
- [ ] `roadmap.md` updated; the v0.9.12 PRD acceptance criteria are all checked.

## Notes

Final phase, the breaking removal. Do not start until 005–007 have migrated every consumer off the deprecated paths.
