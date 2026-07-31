# 006 implementation plan

## Files to read

- `docs/archive/iteration/v0_11/seam-gap-analysis.md`
- `crates/orchest/src/run/supervisor.rs`
- `crates/orchest/src/run/watcher.rs`

## Files to change

- Watcher action collection/arbitration
- Adversarial multi-watcher tests
- Research Pipeline ordering evidence after verification

## Steps

1. Lock precedence and ordering semantics in adversarial tests.
2. Collect/apply actions through one deterministic arbitration point.
3. Preserve delivery FIFO as a separate property.
4. Verify and update SB-7.
