# 002 implementation plan

## Files to read

- `crates/orchest/src/run/actor.rs`
- `crates/orchest/src/hook/mod.rs`
- `crates/orchest/src/tool/agent_as_tool.rs`
- `crates/orchest/src/run/tests.rs`

## Files to change

- `examples/demo/research-pipeline/src/worker.rs`
- `examples/demo/research-pipeline/src/fault.rs`
- `examples/demo/research-pipeline/src/events.rs`
- `examples/demo/research-pipeline/tests/worker.rs`
- `examples/demo/research-pipeline/findings.json`

## Steps

1. Implement deterministic corpus search, file read, and draft output tools.
2. Implement `fault_trigger` with `ErrorKind::Fatal` and
   `RetryHint::Unsafe`.
3. Implement the abort hook and set `repeated_failure_threshold(1)` in the
   worker configuration.
4. Test the tool result in isolation, then test that threshold plus hook
   produces the terminal `RunFailed` event.
5. Add separate context fixtures for `Fresh`, bounded `Fork`, and an empty
   fork error.
6. Add event rendering needed to identify model turns, tool calls, tool
   results, and terminal status without provider-specific output.
7. Update the canonical checklist and finding evidence refs only after the
   relevant command has run.

## Verification

```bash
cargo test -p research-pipeline-demo --test worker
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json
```

The test assertions must show both facts: the tool error is returned, and
the configured repeated-failure policy is what turns it into `RunFailed`.
