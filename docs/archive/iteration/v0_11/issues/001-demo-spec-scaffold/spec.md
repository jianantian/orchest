# 001 · Demo contract, scaffold, and finding schema

## Background

Research Pipeline validates product-shaped supervised delegation before v1.0.
The public runtime surface and the finding/evidence contract must be fixed
before runtime code is added, so later issues cannot redefine success around
whatever happens to work.

## Goal

Create the demo scaffold and the canonical `findings.json` contract. Pre-seed
the known seams, including the missing delegated-worker handle, steering
target mismatch, run-level restart gap, nested event formatting, watcher
backpressure, the start/attach race, and missing cross-watcher action-order
guarantee.

## Acceptance Criteria

- [ ] `examples/demo/research-pipeline/Cargo.toml` builds as package
  `research-pipeline-demo`.
- [ ] `README.md` documents the evidence-run flow, public API boundaries,
  commands, and credential-gated live path.
- [ ] `fixtures/research/` contains or symlinks at least three Briefing Desk
  fixtures.
- [ ] `findings.json` exists as the only editable finding fact source.
- [ ] The v1 schema, serde model, validator skeleton, renderer CLI skeleton,
  and valid/invalid contract fixtures exist.
- [ ] The seam API checklist uses public paths, including
  `orchest::run::RunHandle`, `orchest::run::EventReceiver`, and
  `orchest::run::SupervisionStrategy`.
- [ ] Pre-seeded findings have stable ids and are `open/untriaged`.
- [ ] Source evidence uses repository path plus stable symbol; mutable line
  numbers are not canonical locators.
- [ ] The pre-seeded set includes:
  - `SB-6`: no public start-with-watchers or pre-run pause seam;
  - `SB-7`: no global registration-order guarantee for actions returned by
    independently running watchers.
- [ ] A required live run may be initialized as `not-run`; this forces
  readiness to `unverified`.
- [ ] No supervisor or worker runtime behavior is implemented in this issue.

## Finding Ownership

Issue 001 creates the canonical file and its pre-seeded entries. Issues
002–004 update those entries and add executed evidence. Issue 005 renders the
report. No free-form findings document is part of the workflow.
