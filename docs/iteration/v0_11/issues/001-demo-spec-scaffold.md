# 001 · Demo spec and scaffold

## Background

Demo B (Research Pipeline) validates the supervised delegation API surface before v1.0. Before any delegation code is written, the spec must lock the delegation flow, the fixture re-use plan and the seam API checklist, so every subsequent issue has a clear target.

## Goal

Lock the Research Pipeline user flow, define the live model configuration, enumerate the seam API entry points to be exercised, and create the crate scaffold.

## Acceptance Criteria

- [ ] `examples/demo/research-pipeline/Cargo.toml` exists and compiles (empty binary is fine).
- [ ] `examples/demo/research-pipeline/README.md` describes the purpose, the two-level delegation flow, the run command and env-var configuration.
- [ ] `examples/demo/research-pipeline/fixtures/research/` contains or symlinks the Briefing Desk fixture corpus (or a subset of at least 3 files).
- [ ] A seam API checklist is added to the issue or README listing every public Orchest API entry point the demo must exercise, checked off as issues 002–004 complete them.
- [ ] The live model configuration is documented: which env vars configure the chat model (`RESEARCH_PIPELINE_CHAT_MODEL`, `_API_KEY`, `_API_URL`, `_MAX_TOKENS`).
- [ ] No runtime code beyond a `main.rs` stub is implemented in this issue.

## Notes

The seam API checklist is the single most important output of this issue. If a required API does not exist in the public Orchest surface when building the checklist, that is itself a seam gap finding and must be recorded immediately.
