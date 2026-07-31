# 001 implementation plan

## Files to read

- `docs/archive/iteration/v0_11/seam-gap-analysis.md`
- `crates/orchest/src/tool/agent_as_tool.rs`
- `crates/orchest/src/run/handle.rs`

## Files to change

- Public run/tool modules under `crates/orchest/src/`
- Delegation and steering tests
- Research Pipeline evidence after verification

## Steps

1. Lock the public child-control and ownership semantics in tests.
2. Expose child-target steering and completion through that surface.
3. Preserve supervisor-level behavior and error propagation.
4. Run the declared verifier and update SB-1, SB-2, and P1-4 evidence.
