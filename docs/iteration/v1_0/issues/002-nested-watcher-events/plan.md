# 002 implementation plan

## Files to read

- `docs/archive/iteration/v0_11/seam-gap-analysis.md`
- `crates/orchest/src/run/actor.rs`
- `crates/orchest/src/run/llm_watcher.rs`
- `crates/orchest/src/tool/agent_as_tool.rs`

## Files to change

- Runtime event fan-out and watcher formatting
- Nested-delivery integration tests
- Research Pipeline evidence after verification

## Steps

1. Write failing tests for attached-watcher child delivery and formatting.
2. Route child events through the supported subscriber contract.
3. Add structured `LlmWatcher` formatting without duplicate delivery.
4. Run the verifier and update SB-4/SB-8 evidence.
