# 002 · Core observability metrics

## Background

Core telemetry is currently tool-centric. Runtime users need visibility into model, budget, approval, compaction and event-channel behavior.

## Goal

Add core metrics/spans for the missing runtime paths.

## Acceptance Criteria

- [x] Model call duration is recorded.
- [x] Token usage or estimated token usage is observable where available.
- [x] Budget utilization is observable.
- [x] Approval gate latency is observable.
- [x] Compaction frequency and token savings are observable.
- [x] Event channel drops/backpressure are observable.
- [x] Metric/span names and units are documented in `docs/polaris/observability.md` or the closest runtime observability guide.
- [x] Tests or snapshot-style assertions cover emitted telemetry for model call, approval and compaction paths.
- [x] Examples show how an application subscribes to or exports the new telemetry.
