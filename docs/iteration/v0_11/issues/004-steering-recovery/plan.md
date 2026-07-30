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
6. Attach two recording watchers to the supervisor. Map events to stable keys
   and assert the expected deterministic milestone subsequence within each
   watcher; with no observed drop, compare the complete keyed sequences for
   equality.
7. Do not treat sequence equality as action-order evidence. Record that
   `run::supervisor::reattach_watcher()` runs each watcher in an independent
   task, so actions are applied when each `on_event` future completes rather
   than by registration order. An adversarial gated-watcher reproduction is
   optional source-backed evidence, not a prerequisite for the finding.
8. Drive completion from `EventReceiver` until a terminal event.
9. Update run records, checklist states, evidence refs, and finding refs in
   `findings.json`.

## Verification

```bash
cargo test -p research-pipeline-demo --test failure_escalation
cargo test -p research-pipeline-demo --test watcher_order
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json
```
