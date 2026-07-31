# 004 implementation plan

## Files to read

- `docs/review/v0_11_seam_gap_analysis.md`
- `crates/orchest/src/run/actor.rs`
- `crates/orchest/src/run/handle.rs`

## Files to change

- Watcher subscription delivery/recovery contract
- Backpressure integration tests
- Research Pipeline evidence after verification

## Steps

1. Lock the loss signal and bounded recovery behavior in saturation tests.
2. Implement the smallest public recovery contract.
3. Preserve no-drop FIFO behavior.
4. Verify and update SB-5.
