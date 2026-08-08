# v0.16 Eval Contract Repair — Design

## Status

Approved direction: repair the already-merged v0.16 work through a dedicated
`hotfix/2026_08_06b` rather than rewriting merged history or silently archiving
known acceptance gaps.

## Background

v0.16 introduced the Briefing Desk Eval Lab and merged through PR #265 with
issues #260–#264 closed. A completion audit against the checked PRD/spec
contracts found that the main evaluation loop exists, but several claims are
not true of the current implementation:

- corpus decoding cannot attach the case ID to unknown behavior-tag errors;
- citation grading can normalize `../<existing-basename>` into an accepted
  in-root citation;
- modality grading can count a selected but failed media Tool as coverage;
- committed grader fixtures are presence-checked rather than executed;
- the effective-config snapshot is assembled from hard-coded stand-ins instead
  of the exact resolved execution configuration;
- event-stream closure without a terminal event, cleanup failure, stop reason,
  and artifact-write failure do not follow the documented attempt lifecycle;
- an empty usage object can be treated as complete resource evidence;
- several compare gates lack independent tests and compare trusts incomplete
  result cardinality;
- cost is omitted instead of being displayed as a value or `unknown`;
- the retained public report describes a live experiment whose sensitive local
  artifacts no longer exist, whose baseline was invalid, and which predates
  later semantic changes to graders and aggregation;
- completed v0.16 docs remain under `docs/iteration/` instead of the archive.

The repair keeps the Eval Lab application-local. It does not reopen or rewrite
the historical v0.16 commits.

## Goals

1. Make every checked v0.16 corpus, recorder, grader, runner, comparison, and
   live-validation acceptance criterion true of the final code and evidence.
2. Ensure the effective-config hash represents the same resolved inputs used by
   the execution path, with harness text as the only permitted candidate
   difference.
3. Make attempt states and resource completeness conservative: missing evidence
   must never become a completed/pass result.
4. Re-run a same-commit live baseline/candidate experiment on the repaired
   implementation, beginning with an absolutely valid baseline.
5. Preserve a non-sensitive evidence bundle sufficient to verify configuration
   equality and recalculate aggregate decisions without committing trajectories,
   Tool payloads, generated reports, credentials, or hidden reasoning.
6. Finish the repository Workflow: issues first, one commit per issue, one
   hotfix branch/worktree, full checks, PR/merge, roadmap closeout, and archive.

## Non-goals

- No changes to `orchest`, `orchest-protocol`, provider crates, or language
  bindings.
- No generic eval crate, remote service, LLM judge, embedding grader, or
  automatic prompt writer.
- No claim that the Loom corpus generalizes beyond Briefing Desk.
- No retroactive history rewrite of PR #265 or its issue commits.
- No commitment of raw `evals/runs/` contents.

## Repair slices

The hotfix has four ordered issues. All GitHub issues are created before the
first implementation change.

### 001 — Corpus and grader contract repair

Corpus loading performs a contextual raw-value validation pass before typed
deserialization. For every case it extracts `case_id` first, then validates
enum-like fields such as `split` and `tags`, so an unknown value reports the
actual case and field. Negative tests cover unknown tags and illegal splits in
addition to the existing validation categories.

Citation candidates retain their original relative path until the grader has
rejected absolute paths, parent traversal, and canonical targets outside the
fixture root. Basename extraction happens only after path safety succeeds. A
regression test uses `../` with the basename of a real fixture.

Modality coverage requires successful completion of the relevant media Tool,
not merely a start/failure/retry lifecycle event. Follow-up grounding requires
observable resume metadata rather than caller-supplied seed fields alone.

Every committed grader fixture becomes an executable test input. The suite runs
the pass, fail, and boundary fixtures through the corresponding production
grader and asserts the expected decision.

### 002 — Resolved execution plan, recorder, and attempt lifecycle

Briefing Desk gains one application-level resolved execution plan that is the
single source for both execution and snapshotting. The plan resolves, before
the first model call:

- main and reviewer model identity and non-secret request options;
- main/reviewer runtime, budget, retry, approval, supervision, hooks, and store
  labels;
- the exact Tool registries for each distinct case profile, including reviewer,
  audio, image, report, and optional synthesis Tools;
- ASR/TTS/vision `fake`/`live`/`disabled` routes;
- fresh and follow-up session persistence modes;
- all non-secret environment-driven values, including max tokens and a
  credential-free endpoint representation.

Product `run`/`resume` and eval attempts execute from that same resolved plan or
from builders driven by the same typed resolved inputs. Snapshot code must not
reconstruct a parallel synthetic registry. Output paths and per-attempt IDs are
excluded from the stable descriptor.

The effective-config schema is versioned forward and records stable case
profiles so a run containing both fresh and follow-up cases represents both
session modes. Credential-bearing URL userinfo/query values, secret-like option
keys, or an enabled component without a stable descriptor fail preflight before
the run directory or model call.

Attempt classification is derived from explicit execution and trajectory
evidence with this precedence:

1. `EventsDropped`, a started stream without a terminal event, early stream
   closure, or failed temporary-store cleanup => `inconclusive`;
2. a retained `RunFailed` terminal or provider/model failure with complete event
   delivery => `execution_failure`;
3. `RunCompleted` plus successful required cleanup => `completed`.

`attempt.json` copies the actual terminal kind and stop reason. Artifact-write
errors are returned and fail the eval loudly; they are never logged and ignored.
Focused runner tests cover success, provider failure, event drop, missing
terminal, early closure, cleanup failure, and fixed four-file output.

### 003 — Resource and comparison gate hardening

Usage counts as present only when the retained object contains recognized
usage fields with valid numeric values. `{}`, unrelated objects, and missing
usage mark resource coverage incomplete. Parent, child, reviewer, and vision
calls remain part of the total, without double-counting reasoning/cache fields.

Compare validates the results artifact against the manifest/corpus contract:
expected case IDs, split membership, repetition count, attempt numbering,
grader completion, and resource coverage must all match. A hand-edited or
truncated `results.json` cannot manufacture eligibility.

Independent tests cover candidate must-pass failure, shared baseline/candidate
failure, inconclusive attempts, resource-incomplete attempts, cardinality
tampering, token and latency gates, and the fully eligible path. CLI integration
asserts the compare exit status and generated JSON/Markdown status.

Run summaries and compare reports display aggregate cost when all required
usage provides it; otherwise they explicitly display `unknown`. Token gates
remain active independently of pricing.

### 004 — Live validation evidence and closeout

Before any formal live run, commit a concise causal hypothesis for one candidate
harness change. The hypothesis is based on the previous v0.16 findings and is
present in Git before both formal labels, allowing baseline and candidate to use
the same source commit. The intended candidate emphasizes a minimal Tool chain
while preserving exact report sections, conflict attribution, and fixture
citations; final wording is limited to `src/harness.rs`.

Run the repaired current harness as the formal optimization+validation baseline
with the configured DeepSeek live model. The baseline must have every must-pass
attempt pass. If it is invalid, diagnose and repair the evaluator or establish a
valid product baseline before running the formal candidate; an invalid baseline
cannot close this repair.

Run at least one candidate from the same commit and effective configuration,
with only the declared harness file dirty. Compare it without changing gates.
An ineligible candidate is a valid experimental outcome and is rejected; an
eligible candidate receives human review and may be accepted. A sealed
scorecard runs only for an accepted eligible candidate.

Raw trajectories and generated outputs remain local and gitignored. A separate
sanitized evidence bundle is committed under `docs/review/evidence/` containing:

- pre-registered hypothesis;
- run identity and original artifact hashes;
- harness and effective-config snapshots after secret scanning;
- case/attempt grader scores without free-text payload evidence;
- resource totals and coverage;
- compare JSON/Markdown with gates and decision;
- commands, timestamps, model identity, commit, cost or `unknown`, and the
  scorecard/sealing state.

The bundle must be sufficient to recompute case, tag, split, token, latency, and
eligibility aggregates. A verifier test or script validates its hashes and
recalculates the published summary.

The validation report is updated to distinguish the historical invalid-baseline
experiment from the repaired formal experiment. Acceptance wording is clarified
without concealing the old result: a valid baseline is mandatory for candidate
eligibility and for this repair rerun, while the original pilot's
`invalid_baseline` remains recorded as historical evidence.

## Error handling and safety

- All schema/config/secret/dirty-path failures happen before run directory
  creation and before model calls.
- No secret value is intentionally read by the trajectory sanitizer.
- Sensitive local artifacts are never added to Git; the evidence exporter uses
  an allowlist and a secret-pattern scan.
- One attempt failure does not stop unrelated cases unless the artifact store is
  unreliable, in which case continuing would falsely imply complete evidence
  and the eval aborts.
- Live API failures preserve local failure artifacts and never become behavior
  scores.

## Testing and verification

Each implementation issue follows red-green-refactor and records the failing
test before production changes. Per-issue review checks both spec compliance and
code quality. Before PR creation, run:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
```

The live run is an additional acceptance gate, not a replacement for offline
tests. After merge, update the roadmap on `main`, archive
`docs/iteration/v0_16/` to `docs/archive/iteration/v0_16/`, archive the hotfix
docs using the repository's current `docs/archive/hotfix/` convention, and push
the closeout commit.

## Workflow and commit structure

- Branch/worktree: `hotfix/2026_08_06b` in the existing isolated worktree.
- Create four GitHub issues before implementation.
- Commit 001–004 exactly once each, in dependency order, with `closes #N`.
- Push one hotfix branch and open one PR to `main`.
- Merge only after all checks and final independent review pass.
- Preserve the historical PR #265 audit trail; record its commit-attribution and
  roadmap-timing deviations as process debt rather than rewriting history.
