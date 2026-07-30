# 004 · Terminal failure, escalation, restart gap, and ordering

## Background

The runtime restarts on actor failure, while the controlled tool-failure path
ends in run-level failure. Multi-watcher ordering also applies to the
supervisor handle exposed to the application.

## Goal

Prove the terminal failure and escalation chain, observe whether restart
occurs, verify supervisor-watcher delivery ordering, and record the results in
the canonical evidence source.

## Acceptance Criteria

- [ ] Two watchers attach to the same supervisor `RunHandle`.
- [ ] A deterministic assertion proves their delivery order for the same
  emitted event sequence.
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
