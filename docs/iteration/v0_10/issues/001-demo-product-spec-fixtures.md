# 001 · Demo product spec and fixtures

## Background

v0.10 exists to validate Orchest through a small complete product before v1.0. The demo needs product-level fixtures and acceptance data before implementation starts, otherwise it risks becoming another example snippet.

## Goal

Define the Briefing Desk user flow, fixture research corpus, expected report shape and validation rubric.

## Acceptance Criteria

- [ ] `examples/demo/briefing-desk/README.md` describes the product purpose and end-to-end flow.
- [ ] `examples/demo/briefing-desk/fixtures/research/` contains at least 5 small Markdown/text source files.
- [ ] Fixture sources cover conflicting facts, missing data and at least one citation-worthy quote.
- [ ] `examples/demo/briefing-desk/fixtures/expected/report-shape.md` defines the required report sections.
- [ ] The validation rubric states what counts as API friction, documentation friction, runtime bug and product bug.
- [ ] The spec explicitly says the demo must use public Orchest APIs only.
- [ ] No runtime code is implemented in this issue.

## Notes

Keep fixture data synthetic and repository-safe. The goal is deterministic validation, not realistic domain research.
