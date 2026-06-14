# 003 · RetryHint dispatch

## Background

`RetryHint` exists to describe whether tool failure retry is safe, cautious or unsafe, but tool dispatch does not consume it. This forces applications and models to reinvent retry decisions.

## Goal

Implement bounded, budget-aware retry behavior in the tool dispatch path.

## Acceptance Criteria

- [ ] `RetryHint::Safe` + `ErrorKind::Transient` retries automatically with exponential backoff, max 3 attempts.
- [ ] `RetryHint::Safe` with non-transient errors does not retry unless explicitly documented and tested.
- [ ] `RetryHint::Unsafe` never retries automatically.
- [ ] `RetryHint::Caution` requests approval before retrying.
- [ ] Approval denial returns the structured error payload to the model.
- [ ] Retry attempts emit debuggable events or telemetry with tool name and attempt count.
- [ ] Retry attempts respect existing run budget and timeout constraints.
- [ ] Tests cover safe success-after-retry, safe max-retry exhaustion, unsafe no-retry and caution approval-denied.

## Notes

Keep retry logic local to tool dispatch. Do not conflate this with model API retry policy.
