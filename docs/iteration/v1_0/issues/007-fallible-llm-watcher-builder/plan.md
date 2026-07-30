# 007 implementation plan

## Files to read

- `docs/iteration/v0_11/seam-gap-analysis.md`
- `crates/orchest/src/run/llm_watcher.rs`
- `crates/orchest/src/run/config.rs`

## Files to change

- LLM watcher builder/configuration error types
- Builder tests and all call sites
- Research Pipeline evidence after verification

## Steps

1. Write a failing missing-model error test.
2. Add the typed error and fallible builder signature.
3. Update all Rust/binding/example call sites.
4. Verify and close RB-1.
