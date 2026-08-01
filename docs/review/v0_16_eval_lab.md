# Briefing Desk Eval Lab — v0.16 Validation Report

Application-layer, human-driven harness evaluation pilot on Briefing Desk.
Iteration success criterion: the closed loop **run → record → grade → compare →
human decision** is reproducible and auditable. Finding a higher-scoring prompt
is optional, not required.

## Bottom line

The loop works end-to-end on a live chat model. Offline tests cover corpus
validation, sensitive trajectory sanitization, deterministic graders, runner
preflight, and compare gates. One live baseline + one live candidate were fully
recorded with matching effective-config hashes and differing harness snapshots.

**Formal compare status: `invalid_baseline`.** The original harness failed many
must-pass attempts, so candidate eligibility was correctly **not** computed.
Candidate-1 still produced a complete, higher-scoring run (overall 100 vs
~68.9) with no tag regressions and lower validation tokens/latency. Scorecard
was **not** run (no formally eligible accepted candidate under the gate
contract). Iteration still completes: the gates behaved as designed, and the
artifacts are sufficient to audit the decision.

**Harness decision:** keep candidate-1 prompt/description text in
`examples/demo/briefing-desk/src/harness.rs` as the product default. This is a
human product choice, not an automatic gate acceptance.

## Configuration

| Field | Value |
|------|--------|
| Date (UTC) | 2026-08-01 |
| Git commit (both runs) | `d6d290e6d2e6a3b6d205c107c9020ee893fb90b2` |
| Provider / model | `deepseek` / `deepseek-v4-flash` |
| Request options | default (max tokens via `BRIEFING_DESK_CHAT_MAX_TOKENS=4096`) |
| ASR / TTS | public fakes (not under test) |
| Splits | optimization + validation |
| Repetitions | optimization 1; validation 3 |
| Fixture revision | Loom corpus under `fixtures/research/` |
| Session seeds | `seed-followup-retention`, `seed-followup-sources` (hashes in manifests) |

### Snapshot hashes

| Artifact | baseline | candidate-1 |
|----------|----------|-------------|
| harness `snapshot.sha256` | `206916c06cc52bf197ea381bca1e9d416bc087742c9265e8f4e19e9e540862a4` | `093d22b71382a51e4c89461f04440f282622ae7af59e230754bb1935d5563cbe` |
| effective-config `snapshot.sha256` | `975adc648170630830ec2b1267d68a7539a75c2839339cd97eea6c58fe105964` | **same** |

- Harness texts were restored from each run’s `harness/snapshot.json` (9 surfaces).
- Recomputed SHA-256 of on-disk snapshot bytes matched each manifest entry.
- Effective-config snapshots are byte-equal across baseline and candidate-1
  (only harness surfaces differ).
- Candidate dirty paths were limited to `examples/demo/briefing-desk/src/harness.rs`.

Sensitive full trajectories, tool payloads, and complete generated briefs stay
local under `evals/runs/` (gitignored). This report uses aggregates and short
heading-level evidence only.

## Baseline validity

Baseline must-pass is **not** absolute. Failed must-pass cases included:

- optimization: `opt-tool-search-first`, `opt-modality-audio-quote`,
  `opt-modality-chart-trend`, `opt-conflict-42-vs-35`,
  `opt-report-required-sections`, `opt-citation-real-fixtures`
- validation: `val-full-brief-core`, `val-modality-both`,
  `val-conflict-attribution` (all three attempts where applicable)

Dominant failure modes (deterministic graders):

1. **report_structure** — free-form headings (`Bottom line`, etc.) instead of
   required English H2s (`Executive summary`, `Key findings`, `Conflicts`,
   `Recommendation`, `Sources`).
2. **citation_quality** — fixture basenames absent from a Sources section.
3. **conflict_reconciliation** — `42%` / `35%` present without fixture-basename
   attribution near both numbers.

Resource coverage on the final baseline run: **complete** for all 22 attempts
(after fixing vision usage extraction from `ToolCallCompleted.output`).

Because baseline must-pass failed, compare correctly returned
`invalid_baseline` and did **not** evaluate candidate eligibility gates
(+5 overall, tag non-drop, token/latency). Shared must-pass failure is **not**
interpreted as zero regression.

## Candidate-1

### Pre-registered hypothesis

Baseline misses clustered on report headings, Sources basenames, and dual
retention attribution. Hypothesis: tighten `MAIN_SYSTEM_PROMPT`,
`WRITE_REPORT_TOOL_DESCRIPTION`, and `REVIEWER_SYSTEM_PROMPT` to mandate exact
H2s, both `42%`/`35%` with fixture sources, and a Sources list of real fixture
basenames — without changing tool schemas or runtime.

Written before the candidate run (local note, not committed):
`/tmp/bd-eval-notes/candidate-1-hypothesis.md` during the experiment session.

### Diff surface

Only `examples/demo/briefing-desk/src/harness.rs` (prompt + tool descriptions).
No runtime / provider / binding changes in the candidate experiment itself.

Supporting fixes landed earlier on the branch (not candidate surface):

- UTF-8 safe windowing in conflict attribution grader
- Vision token usage read from structured tool completion `output`

### Live scores (aggregates)

| Metric | baseline | candidate-1 |
|--------|----------|-------------|
| Overall (case-weighted) | 68.92 | 100.0 |
| All completed | true | true |
| Resource incomplete | false | false |
| Validation mean `gate_total_tokens` | 45575.4 | 42821.3 (~94% of baseline) |
| Validation median wall latency ms | 88791.5 | 62233.5 (~70% of baseline) |

Per-tag validation (case-weighted):

| Tag | baseline | candidate-1 |
|-----|----------|-------------|
| tool_selection | 60.42 | 100.0 |
| tool_chaining | 60.42 | 100.0 |
| modality_coverage | 64.81 | 100.0 |
| conflict_reconciliation | 60.0 | 100.0 |
| report_structure | 60.21 | 100.0 |
| citation_quality | 62.37 | 100.0 |
| followup_grounding | 100.0 | 100.0 |

All must-pass cases passed on candidate-1 (optimization + validation).

### Representative behavior (desensitized)

- **Improved — `opt-report-required-sections`:** baseline report_structure
  score ~33 (missing required H2s); candidate 100 with `## Executive summary`
  etc. present.
- **Improved — `val-full-brief-core`:** baseline failed report_structure +
  citation_quality; candidate passed tools + report + citations.
- **Improved — `opt-citation-real-fixtures`:** baseline citation score 0;
  candidate 100 with fixture basenames in Sources.
- **Unchanged strong — `val-followup-grounded`:** both runs 100 / 3/3; no
  re-research tool chain on follow-up seeds.

Full trajectories remain local; not copied here.

## Compare command

```text
briefing-desk eval compare baseline candidate-1
→ status = invalid_baseline
→ artifacts: evals/runs/_compare/compare-baseline-candidate-1.{json,md}
```

Gate table correctly failed `baseline_must_pass` and skipped eligibility.

## Scorecard

**Not run.** Formal eligibility was blocked by invalid baseline. No sealed
scorecard execution; no unsealing event.

## Human decision

1. Accept candidate-1 harness text as the Briefing Desk default surface for
   future demos (checked into the branch).
2. Do **not** claim formal `eligible_for_review` under the v0.16 gate contract
   for this pair of labels, because baseline must-pass failed.
3. Do **not** run sealed scorecard in this iteration.
4. Treat “original harness is too weak to be a valid baseline” as a successful
   closed-loop finding, not as a failed iteration.

## Answers to closeout questions

| Question | Answer |
|----------|--------|
| Is the loop runnable? | **Yes.** CLI `eval run` / `eval compare`, opt-in sensitive recording, manifests, harness + effective-config snapshots, attempt four-files, graders, and compare reports all exercised live and offline. |
| Are graders trustworthy? | **Mostly yes for this pilot.** They are deterministic, fixture-backed, and matched human inspection of heading/citation/conflict failures. They do not judge prose quality. One live panic (UTF-8 window) was fixed before the final baseline. |
| Do manifest + snapshots reproduce inputs? | **Yes for harness + effective config.** Restored 9 surfaces from snapshot; hashes match; effective-config identical across baseline/candidate while harness hashes differ. Session seeds are content-hashed. |
| Valid harness improvement found? | **Behaviorally yes; formally not eligible.** Candidate-1 fixed the measured report/citation/conflict failures. Formal eligibility blocked by invalid baseline. |
| Demo-local vs extract? | **Keep demo-local for now.** The lab is tightly coupled to Briefing Desk tools/fixtures. Extract a generic eval crate only after a second product reuses the same contracts. |

## Runtime / core findings

No Orchest core change was required for the pilot contracts. App-layer notes:

1. Vision tool usage appears on `RuntimeEvent::ToolCallCompleted.output` as the
   structured `details` payload (TokenUsage), not a nested `details` field —
   resource collector adjusted in-demo.
2. Original system prompt was insufficient for grader contracts that require
   exact section titles and basename Sources; that is harness engineering, not
   a runtime bug.

No independent runtime correctness gap filed from this pilot.

## Offline verification

```bash
cargo test -p briefing-desk-demo
# 89 unit + 8 eval_cli + 5 smoke (final counts may grow slightly with fixes)
```

Workspace checks (`cargo test --workspace`, clippy `-D warnings`, `fmt`,
`scripts/lint-check.sh`) run as part of iteration closeout on the branch.

## Non-goals reaffirmed

- No outer agent / auto-writer of harness.
- No generic eval crate, LangSmith, or remote service.
- No core/protocol/provider/binding API changes for the pilot.
- No cross-domain generalization claim (Loom fixture corpus only).
- Live ASR/TTS not required.

## Local artifacts (not committed)

```text
examples/demo/briefing-desk/evals/runs/baseline/
examples/demo/briefing-desk/evals/runs/candidate-1/
examples/demo/briefing-desk/evals/runs/_compare/
```

Delete or keep locally; never commit sensitive trajectories.
