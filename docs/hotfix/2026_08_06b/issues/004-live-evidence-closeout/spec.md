# 004 — Live Validation Evidence and Closeout

## Background

The original v0.16 report correctly recorded `invalid_baseline`, but then kept
the candidate as a product default without establishing a valid formal baseline.
Its local artifacts and pre-run hypothesis are no longer available, and later
grader/aggregation changes mean the report does not validate the final code.

## Pre-registered candidate hypothesis

Add one concise instruction to the existing main harness: use the smallest
sufficient Tool chain and reuse already-read evidence, while preserving every
required report section, conflict attribution, and real fixture citation.

Expected effect: reduce repeated Tool/model work and validation token usage
without decreasing any behavior tag or must-pass result. Only
`examples/demo/briefing-desk/src/harness.rs` may differ between formal baseline
and candidate. If the candidate is not eligible, reject it and restore the
baseline harness.

## Goal and scope

Run a new formal live experiment on the repaired implementation, retain safe
evidence, update the public conclusion, and complete repository closeout.

## Acceptance criteria

- [ ] The hypothesis above is committed before both formal run labels.
- [ ] Baseline and candidate use the same source commit, provider/model,
      request options, fixtures, cases, seed hashes, repetitions, and
      effective-config hash; only harness snapshot/hash may differ.
- [ ] The formal baseline passes every must-pass attempt. An invalid baseline is
      repaired or rerun before candidate eligibility is evaluated.
- [ ] At least one candidate is run after modifying only `src/harness.rs`.
- [ ] Candidate comparison preserves all registered gates and artifacts; an
      ineligible candidate is rejected, while an eligible candidate is reviewed
      using the diff plus improved and unchanged/regressed cases.
- [ ] Sealed scorecard runs only if an eligible candidate is accepted; otherwise
      the report explicitly records `not run`.
- [ ] Raw trajectories, Tool payloads, generated reports, credentials, and
      hidden reasoning remain local and gitignored.
- [ ] A committed allowlist-sanitized evidence bundle contains hypothesis, run
      identity/hashes, safe snapshots, per-attempt numeric grader/resource rows,
      compare gates, decision, timing, model/commit, cost or `unknown`, and
      sealing state.
- [ ] A deterministic verifier recomputes case/tag/split, token, latency, and
      eligibility aggregates from the committed bundle and checks all hashes.
- [ ] `docs/review/v0_16_eval_lab.md` clearly separates the historical
      invalid-baseline pilot from this repaired formal experiment and reports
      the final harness decision without overstating eligibility.
- [ ] Current test counts and all four workspace checks are recorded accurately.
- [ ] PR review and CI pass; after merge, roadmap lists the hotfix and v0.16 plus
      hotfix docs are moved to the repository's archive layout on `main`.

## Implementation plan

1. Add an allowlist evidence schema/exporter and a verifier test using a
   synthetic committed fixture; prove raw payload/evidence fields cannot enter.
2. Run a live optimization+validation baseline with the repaired current harness
   and require absolute must-pass validity.
3. Apply only the pre-registered harness sentence, run the candidate from the
   same commit, and compare.
4. Accept or reject from formal gates plus human review; run scorecard only when
   permitted.
5. Export and secret-scan the sanitized evidence bundle, run its deterministic
   verifier, and update the report.
6. Run full checks, self-review, and commit issue 004 once.
7. Obtain final independent review, push, open/merge the PR, then perform roadmap
   and archive closeout on `main`.
