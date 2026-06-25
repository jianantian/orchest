# 001 · Demo spec and scaffold

## Background

Demo B (Research Pipeline) validates the supervised delegation API surface before v1.0. Before any delegation code is written, the spec must lock the delegation flow, the fixture re-use plan and the seam API checklist, so every subsequent issue has a clear target.

## Goal

Lock the Research Pipeline user flow, define the fake-model contract, enumerate the seam API entry points to be exercised, and create the crate scaffold.

## Acceptance Criteria

- [ ] `examples/demo/research-pipeline/Cargo.toml` exists and compiles (empty binary is fine).
- [ ] `examples/demo/research-pipeline/README.md` describes the purpose, the two-level delegation flow, the fake-model run command and the live provider run command.
- [ ] `examples/demo/research-pipeline/fixtures/research/` contains or symlinks the Briefing Desk fixture corpus (or a subset of at least 3 files).
- [ ] A seam API checklist is added to the issue or README listing every public Orchest API entry point the demo must exercise, checked off as issues 002–004 complete them.
- [ ] The fake-model contract is documented: what responses the fake model emits at each delegation step and what the demo checks as a result.
- [ ] No runtime code beyond a `main.rs` stub is implemented in this issue.

## Notes

The seam API checklist is the single most important output of this issue. If a required API does not exist in the public Orchest surface when building the checklist, that is itself a seam gap finding and must be recorded immediately.
