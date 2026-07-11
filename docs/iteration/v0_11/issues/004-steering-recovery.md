# 004 · Steering injection and supervisor recovery

## Background

This issue exercises the deepest parts of the supervised delegation API: mid-run steering via `InjectCmd`, multi-watcher concurrency, controlled fault injection, and supervisor recovery. These paths are the highest-risk seams for Multivac M2.

## Goal

Implement the `InjectCmd` injection scenario, add a second watcher to test multi-watcher FIFO, trigger the `fault_trigger` tool and verify supervisor recovery.

## Acceptance Criteria

- [ ] Watcher returns `WatcherAction::Inject(message)` from `on_event()` at a predetermined point in the worker run (e.g., after the first tool call event). The injected message is visible in the worker event stream and changes the worker's next step. (`WatcherAction` is at `orchest::run::watcher::WatcherAction`.)
- [ ] Worker processes the injection without panicking or losing existing state.
- [ ] Two watchers are attached concurrently to the same worker run. Both receive all events. Event delivery order is the same for both watchers across repeated runs on the same machine.
- [ ] Fault injection scenario: supervisor run includes a step that triggers `fault_trigger`. Worker run terminates with a failure. Supervisor detects the failure through the public failure detection API (not by catching a panic or checking a side channel).
- [ ] Supervisor recovery: after failure detection, supervisor either restarts the worker or escalates to a summary output. The chosen path is exercised end-to-end.
- [ ] Completion gate: supervisor waits for worker done without relying on a fixed timeout. The mechanism used is documented in `FINDINGS.md` or validation notes.
- [ ] Smoke test for the full path (inject -> multi-watcher -> fault -> recovery -> completion gate) passes and is deterministic.
- [ ] All seam gaps found during this issue are added to `FINDINGS.md` with preliminary classification.

## Notes

Steering injection via `WatcherAction::Inject` works by returning the value from `Watcher::on_event()`. The watcher receives each `RuntimeEvent` and can choose to return `WatcherAction::Inject(message)` once. A helper flag in the watcher state (`injected_once: bool`) is the right guard to ensure exactly one injection. If there is no clean way to target injection at a specific event type, that is a seam gap finding (likely a seam blocker).

`RunHandle::inject_message()` / `RunHandle::steer()` are the external-caller alternatives; test these as well to verify both steering paths work.

**Pre-seeded finding PSF-3**: Fake ASR/TTS providers are in `tests/fake_provider.rs` inside provider crates. This issue does not use them, but note any parallel fake-provider access issues and record for issue 005.

Multi-watcher FIFO test should use a simple event counter assertion, not wall-clock ordering. Record the exact assertion used so issue 005 can evaluate whether the test proves the property.

Supervisor recovery design: prefer restart over escalation if the public API makes restart easy. If restart requires private access or undocumented state management, use escalation and record the restart gap as a seam blocker.
