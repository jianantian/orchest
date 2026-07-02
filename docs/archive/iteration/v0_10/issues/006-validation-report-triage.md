# 006 · Validation report and release-blocker triage

## Background

The output of v0.10 is not only a demo app. It is a decision record for what must change before v1.0 and what can remain backlog.

## Goal

Run Briefing Desk in fake and live modes, write the validation report and update v1.0 scope based on evidence.

## Acceptance Criteria

- [ ] `docs/review/v0_10_demo_validation.md` exists.
- [ ] Report includes fake smoke command, result and commit/date tested.
- [ ] Report includes live provider command, provider/model, date and outcome.
- [ ] Report lists API friction with file/function references where possible.
- [ ] Report lists modality gateway friction (ASR/TTS/multimodal/AIGC), including the known fake-provider accessibility finding from issue 005.
- [ ] Report lists documentation gaps with target docs paths.
- [ ] Every finding is classified as demo blocker, release blocker or post-1.0 backlog.
- [ ] Report includes a **freeze coverage statement**: which public gateways (LLM, ASR, TTS, multimodal, AIGC) this demo actually exercised, and for any gateway left un-dogfooded (e.g. AIGC), an explicit note that its freeze risk is consciously accepted and why (e.g. maturity + crate-level test coverage).
- [ ] `docs/iteration/roadmap.md` links v1.0 dependency to the validation report.
- [ ] Release-blocker fixes are grounded in demo evidence, not inferred from unvalidated backlog.

## Notes

If no live credentials are available, the report must say so explicitly and v1.0 cannot proceed until a live run is completed or the release gate is consciously changed.
