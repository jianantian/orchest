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

- [ ] The v0.10 live scenarios run with the required real LLM, ASR, and TTS
  providers and record redacted outcomes.
- [ ] The v0.11 normal and controlled-fault live scenarios run with the
  configured real chat model.
- [ ] Each record includes exact command, revision, date, provider, model,
  outcome, and a bounded redacted diagnostic when applicable.
- [ ] Fixture, deterministic test, smoke, and live evidence remain separate.
- [ ] The v0.11 canonical JSON validates and its generated report passes the
  byte-for-byte staleness check after the live result is recorded.
- [ ] v1.0 readiness is recomputed from the evidence; closing the issue does
  not waive any remaining open release or seam blocker.

## Notes

This issue is an explicit v1.0 release gate. A waiver would require a named
decision owner and rationale in the canonical evidence; none exists.
