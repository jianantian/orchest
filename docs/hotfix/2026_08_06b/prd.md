# Hotfix 2026-08-06b PRD: v0.16 Eval Contract Repair

## Background

The merged v0.16 Briefing Desk Eval Lab has a functioning corpus, recorder,
grader, runner, and compare loop, but a completion audit found checked
acceptance criteria that are not true of the final implementation. In
particular, the effective-config snapshot is built from hard-coded stand-ins,
missing terminal/resource evidence can be accepted too optimistically, and the
published live experiment used an invalid baseline and predates later grader
changes.

This hotfix repairs the application-local Eval Lab contract without changing
Orchest runtime, protocol, provider crates, or bindings. The detailed approved
design is
[`docs/superpowers/specs/2026-08-06-v0-16-eval-contract-repair-design.md`](../../superpowers/specs/2026-08-06-v0-16-eval-contract-repair-design.md).

## Goals

1. Repair corpus and deterministic-grader correctness gaps.
2. Derive effective configuration from the exact resolved Briefing Desk
   execution plan and make attempt state conservative.
3. Harden resource and comparison completeness gates.
4. Re-run a valid live baseline/candidate experiment on the repaired code and
   retain a sanitized, independently verifiable evidence bundle.
5. Complete roadmap and archive closeout after merge.

## Non-goals

- No generic eval crate, remote service, LLM judge, embedding grader, or prompt
  optimizer.
- No changes under `crates/orchest*` or either language binding.
- No raw trajectories, Tool payloads, generated reports, credentials, or hidden
  reasoning committed to Git.
- No rewriting the merged history of PR #265.
- No cross-domain generalization claim beyond the Loom fixture corpus.

## Issues and dependency order

| Issue | Title | Depends on |
|---|---|---|
| 001 | Corpus and grader contract repair | — |
| 002 | Resolved execution config and attempt lifecycle | 001 |
| 003 | Resource and comparison gate hardening | 002 |
| 004 | Live validation evidence and closeout | 001–003 |

Implementation order is strictly `001 → 002 → 003 → 004`. Each issue is one
focused commit with its GitHub issue number in `closes #N`.

## Global constraints

- Candidate edits are limited to prompt/Tool-description text in
  `examples/demo/briefing-desk/src/harness.rs`.
- Corpus, grader, runtime, Tool schema, fixtures, gates, and evidence schema are
  fixed before the formal live baseline/candidate pair.
- Baseline and candidate use the same Git commit, provider/model, non-secret
  request options, fixtures, cases, session seeds, repetitions, and effective
  configuration; the harness snapshot is the only allowed execution-input
  difference.
- A formal baseline is valid only when every must-pass attempt passes.
- Missing terminal, cleanup, artifact, grader, or usage evidence never becomes
  a completed/pass result.
- Scorecard runs only after an eligible candidate is accepted by a human.
- All code changes remain inside `examples/demo/briefing-desk`; documentation
  changes remain under `docs/`.

## Overall acceptance

- [ ] All four issue specs pass and have one corresponding `closes #N` commit.
- [ ] Offline tests directly cover every repaired failure mode.
- [ ] Effective-config changes when any actual non-harness execution input
      changes and remains stable for harness-only edits.
- [ ] A repaired live baseline is absolutely valid before the formal candidate
      is compared.
- [ ] The public evidence bundle can recompute published aggregates without
      containing sensitive payloads.
- [ ] `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`,
      `cargo fmt --check`, and `bash scripts/lint-check.sh` pass.
- [ ] PR review and CI pass before merge.
- [ ] After merge, roadmap and archive closeout are committed on `main`.
