# 003 — Resource and Comparison Gate Hardening

## Background

Resource collection currently accepts any JSON object, including `{}`, as usage
evidence. Compare trusts `results.json` cardinality and lacks independent tests
for several gates. Cost is serialized at attempt level when available but is not
shown in run/compare summaries when missing or known.

## Goal and scope

Make missing usage and malformed/truncated results fail closed, complete the gate
test matrix, and expose cost without changing the existing token/latency limits.

## Acceptance criteria

- [ ] Usage is present only when recognized token/cost fields contain valid
      numeric values; `{}`, unrelated objects, negative values, and missing
      objects mark the known call incomplete.
- [ ] Parent, child/reviewer, and vision usage is collected once; reasoning,
      cache, and diagnostic details remain excluded from `gate_total_tokens`.
- [ ] Compare validates result case IDs, split membership, attempt numbering,
      expected repetitions, grader completion, and resource coverage against
      manifest/corpus expectations before computing eligibility.
- [ ] Missing, duplicate, extra, or truncated case/attempt results make the runs
      incomparable or not eligible with a concrete mismatch.
- [ ] Independent tests cover candidate must-pass failure, shared must-pass
      failure, inconclusive, resource incomplete, cardinality tampering, score,
      per-tag, token, latency, and fully eligible paths.
- [ ] CLI integration asserts successful eligible compare exit status and exact
      JSON/Markdown status, not merely membership in a set of possible states.
- [ ] Run and compare summaries display total/mean cost when complete; otherwise
      display the literal `unknown`, while token gates still run.
- [ ] Existing thresholds remain: score delta at least 5, no tag decrease,
      tokens at most 115%, latency at most 130%.
- [ ] `cargo test -p briefing-desk-demo`, focused clippy, and fmt pass.

## Implementation plan

1. Add failing empty/unrelated/negative usage tests and tighten recognized usage
   parsing.
2. Add result-contract validation tests and validate cardinality before gates.
3. Add each missing independent gate test and strengthen the CLI integration
   eligible-path assertions.
4. Add cost aggregation/status fields and render value-or-`unknown` in both
   report formats.
5. Run package tests, clippy/fmt, self-review, and commit once.

