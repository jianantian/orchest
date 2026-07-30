# Stable Seam Finding Evidence Design

## Status and Scope

This design defines the Orchest v0.11 Research Pipeline evidence contract.
It does not add a new iteration or change the Demo B product goal. The PRD,
design decisions, implementation overview, and issue-local specs and plans
use this contract.

## Problem

The superseded v0.11 plan accumulated seam gaps in free-form Markdown, then
asked issue 005 to de-duplicate, classify, and rewrite them into the final
report.

That route does not provide a stable identity, evidence, lifecycle, or
cross-reference contract. It makes these mistakes easy:

- the same gap is recorded twice under different descriptions;
- a planned command is presented as executed evidence;
- an implemented fix is described as verified;
- a skipped smoke or live-provider run is treated as a pass;
- a finding disappears while the final report is edited;
- the Markdown report and the v1.0 triage drift apart.

Demo B exists to find and preserve inconvenient seam evidence before v1.0
freezes the public API. Its evidence chain therefore needs a machine-validated
canonical source.

## Goal

Replace the free-form findings-to-report handoff with:

```text
examples/demo/research-pipeline/findings.json
    -> validate
    -> deterministically render
    -> docs/iteration/v0_11/seam-gap-analysis.md
    -> check for staleness
```

`findings.json` is the canonical source for the seam API checklist, validation
runs, findings, classifications, verification state, executive summary, and
readiness verdict.

Issues 001–004 update the same file as implementation exposes evidence. Issue
005 validates, completes triage, runs the required verification, renders the
report, and updates v1.0 and Multivac M2 readiness decisions.

## Ownership Boundary

The schema, validator, and renderer belong to the Research Pipeline demo.

They do not enter:

- `orchest`;
- `orchest-protocol`;
- `orchest-provider` or an implementation provider crate;
- Python or Node bindings;
- a public SDK schema or package.

The first consumer is one iteration-specific demo. Promotion to reusable
infrastructure requires a second proven consumer and a separate design
decision. v0.11 does not create a generic finding framework.

## Canonical File

The canonical file is:

```text
examples/demo/research-pipeline/findings.json
```

Its top-level contract is:

```text
kind
schemaVersion
iteration
subject
executiveSummary
readinessVerdict
apiChecklist[]
runs[]
findings[]
```

`kind` has one stable v1 value. `schemaVersion` is an integer and unknown
versions fail closed. The validator rejects unknown enum values and broken
references.

All repository paths are repository-relative POSIX paths in canonical output.
The file never contains machine-local absolute paths or credentials.

## Report Metadata

### Executive Summary

`executiveSummary` is reviewer-authored reader copy. It summarizes only
evidence and findings present in the same file. The deterministic renderer
does not compose, expand, or reinterpret it.

### Readiness Verdict

`readinessVerdict` uses:

- `ready`;
- `conditional`;
- `blocked`;
- `unverified`.

Consistency rules:

- an open seam blocker or release blocker forbids `ready`;
- a required live-provider run with status `not-run` forbids `ready`;
- `ready` requires every required checklist item to be exercised or explicitly
  verified not applicable;
- `conditional`, `blocked`, and `unverified` must include a bounded reason and
  relevant finding or run refs.

The validator checks structural consistency. It does not choose the verdict.

## Seam API Checklist

Each `apiChecklist` entry identifies one public seam that v0.11 must exercise:

```text
id
apiSurface
publicPath
requirement
status
evidenceRefs[]
findingRefs[]
```

Checklist status uses:

- `planned`;
- `exercised`;
- `failed`;
- `blocked`;
- `not-applicable`.

`exercised` requires at least one executed test, smoke, or live-run evidence
reference. Source existence alone does not establish exercised behavior.

`not-applicable` requires an evidence-backed reason; it is not a substitute
for unavailable evidence.

## Run Evidence

Each `runs` entry records one fixture, test, smoke, or live-provider execution:

```text
id
kind
status
command
date
revision
provider
model
summary
diagnosticExcerpt
evidenceRefs[]
```

Run kind uses:

- `fixture`;
- `test`;
- `smoke`;
- `live-provider`.

Run status uses:

- `passed`;
- `failed`;
- `not-run`;
- `partial`.

Rules:

- `command` is an argument-preserving human-readable command that was actually
  executed; a planned command is not evidence.
- `revision` binds the result to a Git revision when available.
- provider and model are required for `live-provider`, and omitted when they
  do not apply.
- a required run that could not execute remains `not-run` with a reason in
  `summary`.
- `diagnosticExcerpt` is bounded and redacted. It may retain the exact relevant
  error excerpt after secret removal, but never a full provider response,
  prompt, tool payload, or machine-local path.
- fixture and smoke results cannot substitute for a missing live-provider
  result.

## Finding Contract

Each finding contains:

```text
id
title
apiSurface
description
observedConsequence
workaround
classification
status
evidenceRefs[]
action
verification
```

### Identity

`id` is stable from first observation through repair and verification. A
second reproduction of the same gap adds evidence to the original finding; it
does not create a duplicate with a different severity.

Existing pre-seeded finding identities may be retained when they are already
referenced by v0.11 design material. New ids follow one documented repository
convention selected in issue 001.

### Classification

`classification` uses:

- `untriaged`;
- `seam-blocker`;
- `release-blocker`;
- `post-1.0-backlog`.

Classification describes impact on the SDK seam and release, not repair
progress.

Issues 001–004 may leave a finding `untriaged`. Issue 005 assigns the final
classification from reviewed evidence. The renderer does not infer or change
classification.

### Status

`status` uses:

- `open`;
- `implemented`;
- `verified`;
- `deferred`.

Status and classification are independent:

- `open` means the observed gap remains;
- `implemented` means a repair exists but its declared verifier has not yet
  passed on the relevant state;
- `verified` requires a passed verifier and evidence refs;
- `deferred` requires the post-1.0 or explicit release decision that owns it.

Original observation evidence remains after implementation and verification.
The record does not rewrite history to imply the gap never existed.

### Action

`action` records the smallest owned follow-up:

```text
owner
summary
issueRef
revision
```

An open seam blocker or release blocker requires an owner and action summary.
`issueRef` is required after issue 005 files or binds the release work.

The schema does not create GitHub issues or run mutations.

### Verification

`verification` records:

```text
status
commands[]
evidenceRefs[]
summary
```

Verification status uses:

- `not-run`;
- `passed`;
- `failed`;
- `not-applicable`.

`verified` finding status requires `verification.status = passed` and at least
one evidence ref to the executed verifier. `implemented` is not rendered as a
verified fix.

## Evidence References

Every important finding claim and checklist result references evidence:

```text
id
kind
summary
path
symbol
command
runRef
result
```

Evidence kind uses:

- `source`;
- `test`;
- `smoke-run`;
- `live-run`;
- `documentation`;
- `runtime-output`.

Fields are present only when they apply. At least one stable locator is
required: repository-relative `path`, `symbol`, or `runRef`.

Rules:

- repository paths are relative and traversal-free;
- source evidence identifies a stable symbol together with its repository
  path; line numbers may be reviewer hints but are never canonical locators;
- commands are evidence only after execution;
- source and documentation prove existence or contract, not runtime behavior;
- smoke and live behavior remain distinct;
- runtime output is a bounded, redacted excerpt or result summary;
- no prompt, secret, raw provider body, absolute path, or complete tool
  input/output is stored;
- a finding may accumulate evidence from multiple v0.11 issues and runs.

The validator checks references and boundaries. It does not decide whether the
human interpretation of evidence is correct.

## Collection Workflow

### Issue 001

Issue 001 creates:

- the initial canonical `findings.json`;
- the v1 serde contract and validator;
- the seam API checklist;
- pre-seeded findings as `open/untriaged`;
- the report CLI skeleton;
- contract fixtures and invalid cases;
- README instructions for updating findings.

No free-form findings file is created. All iteration documents name the
canonical JSON workflow before implementation begins.

### Issues 002–004

Each issue updates the same canonical file while exercising its seam:

- new gaps receive stable ids;
- repeated gaps add evidence to existing ids;
- actual workarounds are recorded;
- preliminary classification may remain `untriaged`;
- skipped tests remain `not-run`;
- a repair completed during the issue reaches at most `implemented` until its
  declared verifier passes;
- negative findings are not removed to make the report appear clean.

Concurrent fragment aggregation is intentionally out of scope. v0.11 issues
are ordered, so one canonical file is simpler and keeps the evidence chain
visible.

### Issue 005

Issue 005:

1. validates the canonical file;
2. reviews and de-duplicates finding identities;
3. assigns final classification;
4. runs required fixture and smoke verification;
5. performs the live-provider run or records why it remains `not-run`;
6. updates finding verification and status;
7. binds open blockers to action owners and issue refs;
8. writes the executive summary and readiness verdict;
9. deterministically renders the Markdown report;
10. checks the committed report for staleness;
11. updates v1.0 scope and Multivac M2 readiness from the reviewed report.

## Validator and Renderer

The Research Pipeline demo owns a narrow binary:

```text
cargo run -p research-pipeline-demo --bin seam-report -- <command>
```

### Validate

```bash
cargo run -p research-pipeline-demo --bin seam-report -- \
  validate \
  --findings examples/demo/research-pipeline/findings.json
```

Validation is read-only. It checks:

- schema version and enums;
- required fields and bounded strings;
- unique finding, evidence, checklist, and run ids;
- checklist, run, evidence, and finding reference integrity;
- path safety and canonical path shape;
- classification, status, verification, and readiness consistency;
- privacy-shaped exclusions that can be checked deterministically.

### Render

```bash
cargo run -p research-pipeline-demo --bin seam-report -- \
  render \
  --findings examples/demo/research-pipeline/findings.json \
  --out docs/iteration/v0_11/seam-gap-analysis.md
```

Render is the explicit write path. It produces deterministic Markdown and
does not change `findings.json`.

The report contains:

1. executive summary;
2. readiness verdict;
3. seam API checklist;
4. findings table;
5. one detail section per finding;
6. verification and run evidence;
7. live-provider result or explicit unavailable boundary;
8. v1.0 and Multivac M2 implications.

All canonical findings appear. The renderer cannot filter out negative or
unverified rows.

### Check

```bash
cargo run -p research-pipeline-demo --bin seam-report -- \
  check \
  --findings examples/demo/research-pipeline/findings.json \
  --report docs/iteration/v0_11/seam-gap-analysis.md
```

Check renders in memory, compares exact normalized output, writes nothing, and
fails when the committed report is stale.

All commands use stable non-zero exits for invalid input, unsafe output paths,
or stale reports. They do not run the demo, access the network, apply fixes, or
create issues.

## Issue Adjustments

### 001 · Demo Spec, Scaffold, and Finding Contract

Add the canonical JSON, serde contract, validator, CLI skeleton, seam
checklist, fixtures, and pre-seeded findings. Establish the canonical JSON
workflow before later issues collect evidence.

### 002 · Worker Agent and Tool Set

Record worker Tool, error, and event-surface gaps directly in the canonical
file with source and executed-test evidence.

### 003 · Supervisor, Watcher, and ContextMode

Record public import depth, the start/attach race, gated deterministic
attachment, best-effort live attachment, Fresh/Fork, event visibility, and
private-source workarounds. Repeated symptoms sharing one API gap reuse the
same finding id.

### 004 · Steering, Recovery, and Completion Gate

Record injection targeting, per-watcher event FIFO, no-drop cross-watcher
sequence equivalence, the missing cross-watcher action-order guarantee, fault
injection, recovery, completion, and dropped-event boundaries. Preserve the
ordered evidence chain from failure through recovery and completion.

### 005 · Seam Gap Analysis and Release Triage

Complete validator and renderer behavior, final classification, verification,
report generation, staleness checks, blocker ownership, and downstream
readiness updates.

No issues are added or renumbered.

## Error Handling

- Invalid or unknown schema fails before rendering.
- Broken refs list each owner and missing target.
- Unsafe or absolute paths fail validation.
- Duplicate ids fail rather than being silently merged.
- A required live run that cannot execute remains a valid `not-run` record,
  but may prevent a `ready` verdict.
- Malformed diagnostic excerpts fail bounded/privacy checks when
  deterministically detectable; human review remains required for semantic
  secrets.
- Render refuses an output path outside the repository or the declared v0.11
  report owner.
- Check never rewrites a stale report.

## Test Strategy

### Contract Tests

- minimal valid v1 file;
- unknown schema version;
- every invalid enum;
- missing required fields;
- duplicate finding, evidence, run, and checklist ids;
- broken forward and reverse references;
- invalid `verified` without passed verification;
- invalid `ready` with an open blocker;
- invalid `ready` with a required live run marked `not-run`;
- valid explicit unavailable and conditional states.

### Safety and Portability

- POSIX canonical repository paths;
- Windows absolute and drive-relative path rejection;
- Unix absolute path and traversal rejection;
- bounded diagnostic excerpts;
- representative secret-shaped value rejection;
- stable behavior on Windows, macOS, and Linux.

### Rendering

- deterministic Markdown independent of JSON object insertion order;
- stable finding ordering;
- every finding appears in the report;
- implemented and verified wording remains distinct;
- absent live evidence remains visible;
- stale report detection;
- render followed by check passes byte-for-byte.

### Demo Evidence

- worker source and tests add attributable evidence;
- watcher startup, ContextMode, steering, per-watcher FIFO, no-drop delivery
  equivalence, action-order gaps, failure, recovery, and completion runs bind
  to checklist and finding refs;
- fixture, smoke, and live-provider states remain separate;
- the final run records exact command, revision, date, provider, model, result,
  and redacted diagnostic excerpt when applicable.

## Non-Goals

v0.11 does not:

- publish a reusable finding crate or public schema;
- move the contract into `orchest-protocol`;
- automatically classify findings;
- infer findings from logs or tests;
- execute repairs;
- create or update GitHub issues;
- replace `RuntimeEvent`, tracing, or metrics;
- share types or implementation with Multivac Task Evidence Brief;
- introduce historical or cross-iteration trend analysis.

## Completion Condition

v0.11 is complete only when:

- the existing iteration documents consistently name `findings.json` as the
  canonical source;
- issues 001–004 preserve every observed seam gap with attributable evidence;
- issue 005 validates and renders the canonical report;
- the checked-in Markdown passes the staleness check;
- blocker status and verification are internally consistent;
- required unavailable live evidence remains explicit;
- unavailable required live evidence forces readiness to `unverified`;
- issue 005 records, separately, whether unverified live evidence blocks v1.0
  or is accepted by a named decision owner with rationale;
- v1.0 scope and Multivac M2 readiness are updated from the reviewed report.

Closing the iteration after deterministic evidence collection does not imply
that v1.0 is ready. The iteration completion record and release-gate decision
are separate outputs.
