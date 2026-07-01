# Issue 008 Plan: Cleanup + bindings

## Files to Read

- `docs/iteration/v0_9_12/prd.md` (§Success Metrics, §Acceptance Criteria, §Verification)
- `crates/agent-runtime-node/src/lib.rs`, `crates/agent-runtime-py/src/lib.rs` (construction call sites)
- `examples/` (provider usage), workspace `Cargo.toml`
- The deprecated re-export shells left by Issues 002 / 005 / 007

## Files to Change

- Remove deprecated crates `agent-runtime-{model,providers,asr,tts,aigc}` (or reduce to nothing); drop from workspace
- `crates/agent-runtime-node/src/lib.rs`, `crates/agent-runtime-py/src/lib.rs`: use `orchest-protocol` + `orchest-providers` directly
- `examples/*` updated to the new surface
- `docs/iteration/roadmap.md` (mark v0.9.12 done); iteration note with `cargo tree` evidence

## Steps

1. Confirm no consumer still references a deprecated path (grep the workspace + examples).
2. Update `node/py` to construct via `orchest-providers` and reference `orchest-protocol` traits directly.
3. Update examples to the new surface.
4. Remove the deprecated re-export crates; drop workspace members.
5. Finalize the feature graph; run `cargo tree -e features --features llm` and record the evidence.
6. Update `roadmap.md` (move v0.9.12 to "已完成"); tick the PRD acceptance criteria.
7. Full workspace fmt/clippy/test; verify the build is `orchest-*` only.
