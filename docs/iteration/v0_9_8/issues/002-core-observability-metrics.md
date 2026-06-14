# 002 · Core observability metrics

## Background

Core telemetry is currently tool-centric. Runtime users need visibility into model, budget, approval, compaction and event-channel behavior.

## Goal

Add core metrics/spans for the missing runtime paths.

## Acceptance Criteria

- [ ] Model call duration is recorded.
- [ ] Token usage or estimated token usage is observable where available.
- [ ] Budget utilization is observable.
- [ ] Approval gate latency is observable.
- [ ] Compaction frequency and token savings are observable.
- [ ] Event channel drops/backpressure are observable.
