# 003 · RetryHint dispatch

## Background

`RetryHint` exists to describe whether tool failure retry is safe, cautious or unsafe, but tool dispatch does not consume it. This forces applications and models to reinvent retry decisions.

## Goal

Implement bounded, budget-aware retry behavior in the tool dispatch path.

## Acceptance Criteria

- [x] `RetryHint::Safe` + `ErrorKind::Transient` retries automatically with exponential backoff, max 3 attempts.
- [x] `RetryHint::Safe` with non-transient errors does not retry unless explicitly documented and tested.
- [x] `RetryHint::Unsafe` never retries automatically.
- [x] `RetryHint::Caution` requests approval before retrying.
- [x] Approval requests distinguish `InitialToolCall` from retry approval and include retry attempt count plus previous structured error for cautious retries.
- [x] Approval denial returns the structured error payload to the model.
- [x] Retry attempts emit debuggable events or telemetry with tool name and attempt count.
- [x] Retry attempts respect existing run budget and timeout constraints.
- [x] Tests cover safe success-after-retry, safe max-retry exhaustion, unsafe no-retry, caution approval-denied and caution approval context.
- [x] Public examples that show approval handling are updated for the new approval reason/context shape.

## Notes

Keep retry logic local to tool dispatch. Do not conflate this with model API retry policy.
