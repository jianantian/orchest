# 005 implementation plan

## Files to read

- `docs/iteration/v0_11/finding-evidence-contract-design.md`
- `docs/iteration/v0_11/prd.md`
- `docs/iteration/v0_11/design-decisions.md`
- `docs/iteration/roadmap.md`

## Files to change

- `examples/demo/research-pipeline/src/findings.rs`
- `examples/demo/research-pipeline/src/bin/seam-report.rs`
- `examples/demo/research-pipeline/findings.json`
- `examples/demo/research-pipeline/tests/findings_contract.rs`
- `examples/demo/research-pipeline/tests/report_render.rs`
- `docs/iteration/v0_11/seam-gap-analysis.md`
- `docs/iteration/v1_0/` issue documents as required
- `docs/iteration/roadmap.md`

## Steps

1. Finish strict validation of schema, enums, ids, refs, paths, bounded
   excerpts, lifecycle rules, and readiness consistency.
2. Review duplicate symptoms and add evidence to existing finding ids rather
   than minting replacements.
3. Assign final classifications and bind every open blocker to owner, action,
   and issue reference.
4. Run deterministic fixture, test, and smoke commands and record only
   commands actually executed.
5. Run the provider scenario when credentials exist. Otherwise preserve the
   required run as `not-run` with reason and set readiness to `unverified`.
6. Record a separate v1.0 gate decision. Do not infer it from iteration
   closure.
7. Render the report, then run the exact staleness check.
8. Update downstream roadmap, v1.0, and Multivac references from the reviewed
   canonical result.

## Verification

```bash
cargo test -p research-pipeline-demo
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json
cargo run -p research-pipeline-demo --bin seam-report -- render \
  --findings examples/demo/research-pipeline/findings.json \
  --out docs/iteration/v0_11/seam-gap-analysis.md
cargo run -p research-pipeline-demo --bin seam-report -- check \
  --findings examples/demo/research-pipeline/findings.json \
  --report docs/iteration/v0_11/seam-gap-analysis.md
```

After rendering, review that every canonical finding appears and that missing
live evidence is visible in both the run section and readiness verdict.
