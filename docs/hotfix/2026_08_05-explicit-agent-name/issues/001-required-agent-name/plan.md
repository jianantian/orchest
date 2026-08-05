# Implementation Plan

## Files to Read

- `crates/orchest/src/run/config.rs`
- `crates/orchest/src/run/actor.rs`
- `crates/orchest-py/src/lib.rs`
- `crates/orchest-node/src/lib.rs`

## Files to Change

- Core config, actor, bindings, tests, examples, and SDK guides that construct agents.

## Steps

1. Add required `name` to `AgentConfig` and its builder.
2. Read names directly in run-start and handoff paths.
3. Require and forward names in Python and Node bindings.
4. Update call sites and add run/handoff regression coverage.
5. Run workspace verification.

