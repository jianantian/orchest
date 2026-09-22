# 003 · Restart delegated runs after run-level failure

GitHub issue: #251

## Background

SB-3 proves `SupervisionStrategy::Restart` reacts to actor failure but not a
delegated `RunFailed` result caused by tool errors, budgets, or max steps.

## Goal

Apply bounded restart policy to declared delegated run-level failures and
make restart attempts observable.

## Acceptance Criteria

- [x] An eligible delegated `RunFailed` result triggers at most the configured
  retry count.
- [x] Each restart emits attributable `RunRestarted` evidence.
- [x] Retry-unsafe or explicitly non-restartable failures preserve their
  declared terminal semantics.
- [x] Exhausted retries escalate one final failure without a restart loop.
- [x] Existing actor-crash restart behavior remains covered.

## Notes

This is a v1.0 pre-freeze gate.
