# 004 · Terminal failure, escalation, restart gap, and ordering

## Background

The runtime restarts on actor failure, while the controlled tool-failure path
ends in run-level failure. Multiple supervisor watchers preserve their own
event order, but their independently running tasks do not establish a global
registration-order contract for returned actions.

## Goal

Prove the terminal failure and escalation chain, observe whether restart
occurs, verify the two event-delivery properties the public runtime supports,
and record missing cross-watcher action ordering as a separate seam.

## Acceptance Criteria

- [ ] Two watchers attach to the same supervisor `RunHandle`.
- [ ] For a deterministic scenario, each watcher records stable event keys
  and observes the expected milestone subsequence in order.
- [ ] With sufficient capacity and no observed drop, both watchers receive
  the same indexed event sequence.
- [ ] The test does not infer action order from event-sequence equality.
- [ ] The lack of a global registration-order guarantee for
  `Inject` / `Steer` / `Abort` actions is updated as a pre-seeded seam
  finding, with independent watcher-task source evidence.
- [ ] The controlled scenario records:
  `ToolError(Fatal, Unsafe)` → threshold `1` →
  `Hook::on_repeated_failure` returning `HookAction::Abort` → worker
  `RunFailed`.
- [ ] The supervisor observes the failed delegation result and produces an
  escalation summary without panic.
- [ ] `SupervisionStrategy::Restart { max_retries: 1 }` is configured through
  `orchest::run::SupervisionStrategy`.
- [ ] `RuntimeEvent::RunRestarted` is only claimed if captured. Its expected
  absence on the nested run-level failure path is recorded as a finding.
- [ ] Completion is gated by a terminal event received through
  `orchest::run::EventReceiver`, not a fixed timeout.
- [ ] Watcher backpressure and possible `try_send` loss are recorded with the
  evidence available; the test does not overclaim lossless delivery.
- [ ] All owned records are updated in `findings.json`.

## Recovery Language

“Recovery” in this issue means supervisor escalation after worker
`RunFailed`. It does not mean worker restart unless a `RunRestarted` event is
actually observed.
