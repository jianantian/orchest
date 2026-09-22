# 008 · v0.10 and v0.11 live-provider verification

GitHub issue: #258

## Background

The required v0.11 live-provider run is `not-run` because
`RESEARCH_PIPELINE_CHAT_MODEL` and `RESEARCH_PIPELINE_API_KEY` are absent.
v0.10 live LLM/ASR/TTS verification is also outstanding. No decision owner
has accepted this uncertainty.

## Goal

Execute and record both demos' required live-provider paths before v1.0.

## Acceptance Criteria

- [x] The v0.10 live scenarios run with the required real LLM, ASR, and TTS
  providers and record redacted outcomes.
- [x] The v0.11 normal and controlled-fault live scenarios run with the
  configured real chat model.
- [x] Each record includes exact command, revision, date, provider, model,
  outcome, and a bounded redacted diagnostic when applicable.
- [x] Fixture, deterministic test, smoke, and live evidence remain separate.
- [x] The v0.11 canonical JSON validates and its generated report passes the
  byte-for-byte staleness check after the live result is recorded.
- [x] v1.0 readiness is recomputed from the evidence; closing the issue does
  not waive any remaining open release or seam blocker.

## Notes

This issue is an explicit v1.0 release gate. A waiver would require a named
decision owner and rationale in the canonical evidence; none exists.

## Implementation record (2026-09-22)

Execution at revision `8a52610` plus the demo-side fixes listed below, with
the repository credentials.

**v0.10 Briefing Desk** — one live session, chat (`openrouter` /
`anthropic/claude-sonnet-4.6`), vision (same adapter), ASR (`aliyun` /
`fun-asr-flash-2026-06-15`) and TTS (`volcengine` /
`volc.service_type.10029`) all live; exit 0, brief and audio written. The run
surfaced three demo-side wiring defects (`review_report` schema/mapper
mismatch, ASR model filter ignored, live TTS voice missing); all three are
fixed here and triaged as rows 11–14 of
[`docs/review/v0_10_demo_validation.md`](../../../review/v0_10_demo_validation.md).

**v0.11 Research Pipeline** — `run-live-provider` `passed` (normal scenario,
3 delegated workers, root `EndTurn`, 57 watcher events) and
`run-live-provider-controlled-fault` `passed`: the designed fault path
(`fault_trigger` fatal → `RunRestarted { attempt: 1 }` → escalation) in all
four post-repair live runs. The first drill wiring put the fault instruction in
the *delegated* request, which the live `LlmWatcher` read as a prompt injection
and aborted in 2 of 4 attempts; the drill now carries the fault in the worker's
own prompt, exposes only `search_corpus` + `fault_trigger` to the drill worker,
and scopes the drill watcher prompt. P1-6 records the underlying observation
that the default watcher prompt states no boundary for `abort`.

**Ledger state** — RB-1 moved to `verified` on the issue #255 verifier
(`cargo test -p orchest --lib build_fails_with_missing_model_when_model_not_set`),
`run-report-smoke`'s stale report path corrected to
`docs/review/v0_11_seam_gap_analysis.md`, readiness recomputed to `ready` with
no open seam or release blocker. The generated report was re-rendered and
`seam-report check` reports it current.

**Open after this issue** — review of the recorded live evidence; the v1.0
release-packaging acceptance items are separate.
