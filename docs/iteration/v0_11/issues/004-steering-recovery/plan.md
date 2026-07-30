# 004 implementation plan

## Files to read

- `crates/orchest/src/run/actor.rs`
- `crates/orchest/src/run/supervisor.rs`
- `crates/orchest/src/run/handle.rs`
- `crates/orchest/src/hook/runner.rs`

## Files to change

- `examples/demo/research-pipeline/src/fault.rs`
- `examples/demo/research-pipeline/src/supervisor.rs`
- `examples/demo/research-pipeline/src/watcher.rs`
- `examples/demo/research-pipeline/tests/failure_escalation.rs`
- `examples/demo/research-pipeline/tests/watcher_order.rs`
- `examples/demo/research-pipeline/findings.json`

## Steps

1. Configure `SupervisionStrategy::Restart { max_retries: 1 }` without
   changing the controlled failure from run-level to actor-level.
2. Execute the issue 002 fault tool under threshold `1` and the abort hook.
3. Capture each event/result boundary through worker failure, tool-return
   failure, and supervisor escalation.
4. Assert the worker terminal event is `RunFailed`.
5. Inspect the event stream for `RunRestarted`. Record absence as evidence
   for the pre-seeded restart-gap finding; do not turn absence into a passing
   recovery assertion.
6. Attach two recording watchers to the supervisor and compare indexed event
   sequences. Avoid wall-clock ordering assertions.
7. Drive completion from `EventReceiver` until a terminal event.
8. Update run records, checklist states, evidence refs, and finding refs in
   `findings.json`.

## Verification

```bash
cargo test -p research-pipeline-demo --test failure_escalation
cargo test -p research-pipeline-demo --test watcher_order
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json
```
