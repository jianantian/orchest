# 005 implementation plan

## Files to read

- `docs/iteration/v0_11/seam-gap-analysis.md`
- `crates/orchest/src/run.rs`
- `crates/orchest/src/run/handle.rs`

## Files to change

- Public run-start configuration
- Watcher registration lifecycle tests
- Research Pipeline attachment evidence after verification

## Steps

1. Write the first-event observation contract as a failing integration test.
2. Add the smallest pre-run registration/activation surface.
3. Preserve post-start attachment compatibility.
4. Verify and update SB-6.
