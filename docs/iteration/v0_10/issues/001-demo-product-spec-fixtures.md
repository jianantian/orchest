# 001 · Demo product spec and fixtures

## Background

v0.10 exists to validate Orchest through a small complete product before v1.0. The demo needs product-level fixtures and acceptance data before implementation starts, otherwise it risks becoming another example snippet.

## Goal

Define the Briefing Desk user flow, fixture research corpus, expected report shape and validation rubric.

## Acceptance Criteria

- [ ] `examples/demo/briefing-desk/README.md` describes the product purpose and end-to-end multimedia flow.
- [ ] `examples/demo/briefing-desk/fixtures/research/` contains at least 5 small Markdown/text source files.
- [ ] Fixture sources cover conflicting facts, missing data and at least one citation-worthy quote.
- [ ] The corpus also includes at least one image fixture (a chart/screenshot, PNG) and at least one short audio fixture (a recorded "interview" clip) so the ASR and multimodal-image paths have real inputs.
- [ ] The spec defines what the agent is expected to extract from the audio and image fixtures (so transcription/vision correctness is checkable).
- [ ] `examples/demo/briefing-desk/fixtures/expected/report-shape.md` defines the required report sections, including how transcribed-audio and image-derived facts are cited.
- [ ] The validation rubric states what counts as API friction, modality gateway friction, documentation friction, runtime bug and product bug.
- [ ] The spec explicitly says the demo must use public Orchest APIs only (core + AIGC/ASR/TTS provider crates).
- [ ] No runtime code is implemented in this issue.

## Notes

Keep fixture data synthetic and repository-safe. The goal is deterministic validation, not realistic domain research.

Keep media fixtures tiny: the audio clip should be a few seconds and the image a small synthetic chart. They exist to exercise the gateways, not to stress encoders. If committing binary fixtures is undesirable, document a generation script that produces them deterministically instead.
