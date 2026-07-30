# 001 implementation plan

## Files to read

- `Cargo.toml`
- `examples/demo/briefing-desk/Cargo.toml`
- `examples/demo/briefing-desk/README.md`
- `docs/iteration/v0_11/finding-evidence-contract-design.md`

## Files to change

- `examples/demo/research-pipeline/Cargo.toml`
- `examples/demo/research-pipeline/README.md`
- `examples/demo/research-pipeline/findings.json`
- `examples/demo/research-pipeline/src/main.rs`
- `examples/demo/research-pipeline/src/findings.rs`
- `examples/demo/research-pipeline/src/bin/seam-report.rs`
- `examples/demo/research-pipeline/tests/findings_contract.rs`
- `examples/demo/research-pipeline/tests/fixtures/findings/*.json`

## Steps

1. Add the demo package and the `research-pipeline` and `seam-report`
   binaries with workspace path dependencies.
2. Add the fixture corpus link or copy and document its provenance.
3. Implement the closed v1 enums and top-level serde model described in
   `finding-evidence-contract-design.md`.
4. Add read-only validation for schema version, required fields, unique ids,
   references, path safety, and readiness consistency.
5. Add `validate`, `render`, and `check` CLI shapes. `render` may initially
   produce a minimal deterministic document; later behavior belongs to issue
   005.
6. Seed the seam checklist and known findings. Mark planned execution as
   planned or `not-run`, never passed.
7. Document how issues 002–004 add evidence without changing finding ids.

## Verification

```bash
cargo check -p research-pipeline-demo
cargo test -p research-pipeline-demo --test findings_contract
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json
```

Contract tests cover at least unknown schema versions, duplicate ids, broken
refs, unsafe paths, verified-without-verification, ready-with-open-blocker,
and ready-with-live-not-run.
