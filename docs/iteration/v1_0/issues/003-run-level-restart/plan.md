# 003 implementation plan

## Files to read

- `docs/iteration/v0_11/seam-gap-analysis.md`
- `crates/orchest/src/run/supervisor.rs`
- `crates/orchest/src/tool/agent_as_tool.rs`

## Files to change

- Supervision/restart state handling
- Run-level restart tests
- Research Pipeline failure evidence after verification

## Steps

1. Define eligible run-level failure and retry exhaustion semantics in tests.
2. Apply bounded restart without changing actor-crash behavior.
3. Emit attributable restart events.
4. Verify and update SB-3.
