# v0.16 Eval Contract Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the merged Briefing Desk Eval Lab satisfy every checked v0.16 contract, prove it with a repaired live experiment, and finish Workflow closeout.

**Architecture:** Keep the repair application-local. A typed resolved execution description becomes the single source for both product/eval execution and effective-config snapshots; deterministic graders and compare gates fail closed on missing evidence; a separate allowlisted evidence bundle preserves numerical live proof without committing raw trajectories or generated content.

**Tech Stack:** Rust 2021, Tokio, Serde/serde_json, SHA-256, clap, Orchest runtime APIs, GitHub CLI, existing Briefing Desk scripted/live model seams.

## Global Constraints

- Do not modify `crates/orchest*`, `orchest-py`, or `orchest-node`.
- Do not add an LLM judge, embedding grader, remote eval service, generic eval crate, or automatic prompt writer.
- Candidate edits are limited to text in `examples/demo/briefing-desk/src/harness.rs`.
- Corpus, graders, gates, fixtures, and evidence schema are fixed before the formal live pair.
- Baseline and candidate use the same commit and effective configuration; only the harness snapshot may differ.
- A formal baseline must pass every must-pass attempt before candidate eligibility is evaluated.
- Missing terminal, cleanup, artifact, grader, or usage evidence never becomes completed/pass.
- Never commit raw `evals/runs/`, Tool payloads, generated reports, credentials, or hidden reasoning.
- One task equals one issue equals one implementation commit, in order #289 → #290 → #291 → #292.
- TDD is mandatory: add each regression test, run it and observe the expected failure, then write production code.

---

### Task 1: Corpus and grader contract repair (#289)

**Files:**
- Modify: `examples/demo/briefing-desk/src/eval/case.rs`
- Modify: `examples/demo/briefing-desk/src/eval/grader/mod.rs`
- Modify: `examples/demo/briefing-desk/src/eval/grader/content.rs`
- Modify: `examples/demo/briefing-desk/src/eval/grader/tool_flow.rs`
- Modify/add fixtures: `examples/demo/briefing-desk/evals/test-fixtures/`
- Source contract: `docs/archive/hotfix/2026_08_06b/issues/001-corpus-grader-contract-repair/spec.md`

**Interfaces:**
- Produce `validate_raw_case_enums(value: &serde_json::Value) -> Result<(), CaseLoadError>` in `case.rs`, called by `load_corpus` before `serde_json::from_value`.
- Produce citation parsing that retains the raw relative path until root-safety validation succeeds.
- Produce a successful-completion Tool predicate used by modality graders.
- Produce observable follow-up resume evidence in trajectory/grader input; do not trust seed metadata alone.

- [ ] **Step 1: Add contextual raw corpus failures**

Add tests that mutate a real case to `tags: ["unknown_behavior"]` and to
`split: "unknown_split"`, load through the public corpus loader, and assert the
error contains both `case_id=<id>` and `field=tags`/`field=split`.

```rust
assert!(message.contains("case_id=opt-tool-selection-basic"));
assert!(message.contains("field=tags"));
assert!(message.contains("unknown_behavior"));
```

- [ ] **Step 2: Run the raw-corpus tests and verify RED**

Run:

```bash
cargo test -p briefing-desk-demo eval::case::tests::rejects_unknown_tag_with_case_context
cargo test -p briefing-desk-demo eval::case::tests::rejects_unknown_split_with_case_context
```

Expected: FAIL because typed Serde decoding reports neither the correct field nor case ID.

- [ ] **Step 3: Implement raw contextual validation**

Parse corpus JSON into `serde_json::Value`, iterate `cases`, read `case_id`
first, validate raw `split` and each `tags` value against `EvalSplit` and
`BehaviorTag`, then deserialize the already-validated value into `CaseCorpus`.
Preserve the existing schema/version and all post-deserialization validation.

```rust
fn validate_raw_case_enums(value: &Value) -> Result<(), CaseLoadError> {
    let cases = value.get("cases").and_then(Value::as_array)
        .ok_or_else(|| CaseLoadError::new("cases must be an array").with_field("cases"))?;
    for raw in cases {
        let case_id = raw.get("case_id").and_then(Value::as_str).unwrap_or("<unknown>");
        validate_raw_split(raw, case_id)?;
        validate_raw_tags(raw, case_id)?;
    }
    Ok(())
}
```

- [ ] **Step 4: Add traversal, failed-modality, and metadata-only follow-up tests**

Add tests asserting:

```rust
// A real fixture basename does not make parent traversal safe.
output = "## Sources\n- ../001-retention-dashboard-notes.md";
assert!(!citation_grade.passed);

// Started/failed describe_image or transcribe_audio is not coverage.
assert!(!modality_grade.passed);

// Matching seed fields with no retained resume evidence is not grounding.
assert!(!followup_grade.passed);
```

- [ ] **Step 5: Run the grader regressions and verify RED**

Run the three exact new test names. Expected failures: traversal is accepted,
failed media calls count as selected, and seed metadata alone passes.

- [ ] **Step 6: Implement safe citations and successful lifecycle predicates**

Validate path shape/canonical root membership before basename matching. Add a
predicate that requires `tool_call_completed` for modality coverage. Require a
retained resume marker produced by the runner/session path in addition to seed
ID/hash for follow-up grounding.

- [ ] **Step 7: Execute every committed grader fixture**

Replace the fixture-presence test with a table containing fixture paths, target
grader IDs, expected pass/fail, and any boundary expectation. Load the files,
construct `GraderInput`, invoke the production grader, and assert the table.

- [ ] **Step 8: Verify GREEN and package hygiene**

Run:

```bash
cargo test -p briefing-desk-demo eval::case
cargo test -p briefing-desk-demo eval::grader
cargo clippy -p briefing-desk-demo -- -D warnings
cargo fmt --check
```

Expected: all pass, no warnings or formatting diff.

- [ ] **Step 9: Commit issue #289 once**

```bash
git add examples/demo/briefing-desk/src/eval/case.rs \
        examples/demo/briefing-desk/src/eval/grader \
        examples/demo/briefing-desk/evals/test-fixtures
git commit -m "fix: repair eval corpus and grader contracts (closes #289)"
```

---

### Task 2: Resolved execution config and attempt lifecycle (#290)

**Files:**
- Create: `examples/demo/briefing-desk/src/execution.rs`
- Modify: `examples/demo/briefing-desk/src/main.rs`
- Modify: `examples/demo/briefing-desk/src/app.rs`
- Modify: `examples/demo/briefing-desk/src/eval/effective_config.rs`
- Modify: `examples/demo/briefing-desk/src/eval/runner.rs`
- Modify: `examples/demo/briefing-desk/src/eval/trajectory.rs`
- Modify: `examples/demo/briefing-desk/src/eval/session.rs`
- Modify: `examples/demo/briefing-desk/tests/eval_cli.rs`
- Source contract: `docs/archive/hotfix/2026_08_06b/issues/002-resolved-config-attempt-lifecycle/spec.md`

**Interfaces:**
- Produce `ResolvedChatModel`, which contains the adapter plus provider/model,
  sanitized request options, and credential-free endpoint metadata.
- Produce `ResolvedExecutionEnvironment`, which resolves ASR/TTS/vision routes
  once and keeps secrets only in execution-only variants.
- Produce `prepare_run(...) -> Result<PreparedRun, DemoError>` and
  `prepare_resume(...) -> Result<PreparedResume, DemoError>`; each prepared
  value contains the actual config/registry and a stable `ExecutionProfile`.
- Advance `EFFECTIVE_CONFIG_SCHEMA_VERSION` and replace a single synthetic
  session mode with stable case profiles.
- Produce a pure `classify_attempt(evidence: &AttemptEvidence) -> AttemptStatus`
  used by the runner.

- [ ] **Step 1: Add failing actual-config snapshot tests**

Tests must build profiles through the product preparation functions and prove
that reviewer/media Tool names are present, fresh/follow-up modes are distinct,
vision uses the chat route, and max tokens/endpoints are represented.

```rust
assert!(tool_names.contains("review_report"));
assert!(tool_names.contains("describe_image"));
assert!(snapshot.case_profiles.iter().any(|p| p.session_mode == FollowUpFromSeed));
assert_eq!(snapshot.main.request_options["max_tokens"], 4096);
```

- [ ] **Step 2: Verify actual-config tests RED**

Run the new focused tests. Expected: FAIL because current runner fingerprints
only search/read/write and hard-codes empty/fake/none values.

- [ ] **Step 3: Implement typed resolution and preparation**

Move product-pipeline construction into `execution.rs`. `app.rs` remains CLI
orchestration and calls the preparation API. Execution-only capability variants
may own API keys/adapters, but their `ExecutionProfile` conversion exposes only
stable non-secret labels.

```rust
pub struct PreparedRun {
    pub config: AgentConfig,
    pub input: RunInput,
    pub registry: ToolRegistry,
    pub profile: ExecutionProfile,
}

pub struct ResolvedChatModel {
    pub adapter: Arc<dyn ModelAdapter>,
    pub provider: String,
    pub model: String,
    pub request_options: Value,
    pub endpoint: Option<String>,
}
```

Both normal CLI and eval runner must use these functions; delete the synthetic
registry in `runner::build_effective_config`.

- [ ] **Step 4: Version and populate effective-config profiles**

Represent each distinct selected case profile with stable case IDs, Tool
fingerprints, routes, and session mode. Sort profiles, case IDs, and tools before
normalization. Reject credentials and secret-like fields before run directory
creation.

- [ ] **Step 5: Verify config hash sensitivity GREEN**

Run focused effective-config tests proving max tokens, endpoint, route, Tool
schema, runtime, and session mode each change the hash while harness-only text
does not.

- [ ] **Step 6: Add failing attempt-classification tests**

Construct explicit evidence cases for completed, retained `RunFailed`, event
drop, started-without-terminal, early closure, and cleanup failure. Assert the
required status, terminal kind, and stop reason.

- [ ] **Step 7: Verify classification tests RED**

Expected: current logic maps missing terminal to `execution_failure`, ignores
cleanup failure, and leaves stop reason empty.

- [ ] **Step 8: Implement evidence-based lifecycle and error propagation**

Extract retained terminal information from `TrajectoryRecorder`. Apply
inconclusive precedence for dropped/missing/early/cleanup evidence. Change
attempt artifact writing to return `Result`; abort the eval on an unreliable
artifact store rather than printing an error and returning success.

- [ ] **Step 9: Add runner/CLI failure-path coverage**

Use scripted/fake adapters and direct event fixtures to prove success,
provider failure, drop, no terminal, early closure, cleanup failure, label
conflict, and four-file output. Assert model call count remains zero for every
preflight error.

- [ ] **Step 10: Verify GREEN and unchanged smoke behavior**

Run:

```bash
cargo test -p briefing-desk-demo eval::effective_config
cargo test -p briefing-desk-demo eval::runner
cargo test -p briefing-desk-demo --test eval_cli
cargo test -p briefing-desk-demo --test smoke
cargo clippy -p briefing-desk-demo -- -D warnings
cargo fmt --check
```

- [ ] **Step 11: Commit issue #290 once**

```bash
git add examples/demo/briefing-desk/src/execution.rs \
        examples/demo/briefing-desk/src/main.rs \
        examples/demo/briefing-desk/src/app.rs \
        examples/demo/briefing-desk/src/eval \
        examples/demo/briefing-desk/tests/eval_cli.rs
git commit -m "fix: resolve eval config and attempt lifecycle (closes #290)"
```

---

### Task 3: Resource and comparison gate hardening (#291)

**Files:**
- Modify: `examples/demo/briefing-desk/src/eval/resource.rs`
- Modify: `examples/demo/briefing-desk/src/eval/compare.rs`
- Modify: `examples/demo/briefing-desk/src/eval/runner.rs`
- Modify: `examples/demo/briefing-desk/src/eval/artifact.rs`
- Modify: `examples/demo/briefing-desk/tests/eval_cli.rs`
- Source contract: `docs/archive/hotfix/2026_08_06b/issues/003-resource-compare-hardening/spec.md`

**Interfaces:**
- Make usage parsing return whether recognized numeric evidence was consumed.
- Add `validate_results_contract(manifest: &RunManifest, results: &RunResults) -> Result<(), Vec<String>>` and run it during load/compare.
- Add cost completeness/aggregate fields to `RunResults` with a schema-version update.

- [ ] **Step 1: Add failing usage-evidence tests**

Assert `{}`, unrelated objects, strings, and negative-only values increment
`missing_usage_calls`; recognized zero-valued token fields count as explicit
usage only when the protocol field is present and numeric.

- [ ] **Step 2: Verify usage tests RED, then implement strict parsing**

Replace `usage.is_object()` acceptance with explicit recognized-field parsing.
Keep saturating token addition and existing no-double-count formula.

- [ ] **Step 3: Add failing results-cardinality tests**

Create baseline/candidate fixtures with missing, duplicate, extra, and
wrong-repetition attempts. Assert load/compare rejects each with all detected
mismatches rather than computing gates.

- [ ] **Step 4: Implement results-contract validation**

Validate exact manifest case set, selected split, per-split repetition policy,
attempt sequence `1..=N`, grader status, and resource coverage. Accumulate all
mismatches for one report.

- [ ] **Step 5: Complete independent gate tests**

Add one focused test each for candidate must-pass, shared failure,
inconclusive, resource incomplete, score delta, tag drop, token, latency, and
eligible paths. Each test asserts its exact `gate_id` and status.

- [ ] **Step 6: Add value-or-unknown cost reporting**

Aggregate cost only when every required known call reports it. Store
`validation_total_cost_usd: Option<f64>` plus completeness, then render a
formatted value or literal `unknown` in run JSON/Markdown and compare reports.
Do not include cost in the eligibility formula.

- [ ] **Step 7: Strengthen CLI integration**

The scripted eligible-path test must assert exit success,
`status == "eligible_for_review"` in JSON, the same status in Markdown, and no
mutation of either run directory.

- [ ] **Step 8: Verify GREEN**

Run:

```bash
cargo test -p briefing-desk-demo eval::resource
cargo test -p briefing-desk-demo eval::compare
cargo test -p briefing-desk-demo --test eval_cli
cargo test -p briefing-desk-demo
cargo clippy -p briefing-desk-demo -- -D warnings
cargo fmt --check
```

- [ ] **Step 9: Commit issue #291 once**

```bash
git add examples/demo/briefing-desk/src/eval \
        examples/demo/briefing-desk/tests/eval_cli.rs
git commit -m "fix: harden eval resource and comparison gates (closes #291)"
```

---

### Task 4: Live validation evidence and closeout (#292)

**Files:**
- Create: `examples/demo/briefing-desk/src/eval/evidence.rs`
- Modify: `examples/demo/briefing-desk/src/eval/mod.rs`
- Modify: `examples/demo/briefing-desk/src/eval/cli.rs`
- Modify: `examples/demo/briefing-desk/src/main.rs`
- Modify conditionally: `examples/demo/briefing-desk/src/harness.rs`
- Create: `docs/review/evidence/v0_16_eval_repair/`
- Modify: `docs/review/v0_16_eval_lab.md`
- Modify: `examples/demo/briefing-desk/README.md`
- Source contract: `docs/archive/hotfix/2026_08_06b/issues/004-live-evidence-closeout/spec.md`

**Interfaces:**
- Add an allowlisted `EvidenceBundle` schema containing only stable IDs, hashes,
  numeric scores/resources, safe config, compare gates, and decisions.
- Add an export command/function that never reads `trajectory.jsonl` or
  `output.md` and strips free-text grader evidence/failure fields.
- Add `verify_evidence_bundle(&EvidenceBundle) -> Result<VerifiedEvidence, EvidenceError>` that recomputes aggregates and checks hashes.

- [ ] **Step 1: Add failing evidence allowlist/verifier tests**

Using synthetic run directories, assert the export contains no trajectory,
output, Tool payload, prompt reasoning, authorization, cookie, API key, or
free-text grader evidence. Tamper a numeric row and hash and assert verification
fails.

- [ ] **Step 2: Verify evidence tests RED, then implement exporter/verifier**

Read only manifest, safe snapshots, results, and numeric projections from
scores/attempt records. Recompute case/tag/split, token, latency, and compare
decision using the same production aggregation functions. Write canonical JSON
plus SHA-256 and a short Markdown index.

- [ ] **Step 3: Freeze and verify the live run commit**

Before running, ensure:

```bash
git status --short
cargo test -p briefing-desk-demo
```

Expected: clean tracked worktree; package tests pass. Record `git rev-parse
HEAD`. Set `BRIEFING_DESK_CHAT_MODEL=deepseek/deepseek-v4-flash` and the
non-secret max-token option used by the historical experiment.

- [ ] **Step 4: Run the formal baseline**

```bash
cargo run -p briefing-desk-demo -- eval run \
  --label repair-baseline \
  --split optimization,validation \
  --record-sensitive
```

Verify fixed four-file artifacts, complete resources, effective-config profile
contents, and every baseline must-pass attempt. If baseline is invalid, diagnose
with TDD, amend the owning issue commit before any formal candidate, delete only
the invalid local label, and rerun from a clean commit.

- [ ] **Step 5: Apply only the pre-registered candidate harness change**

Add one sentence to the main prompt requiring the smallest sufficient Tool
chain and reuse of already-read evidence while preserving exact report/citation
requirements. Confirm `git status --short` lists only `src/harness.rs`.

- [ ] **Step 6: Run and compare the formal candidate**

```bash
cargo run -p briefing-desk-demo -- eval run \
  --label repair-candidate-1 \
  --split optimization,validation \
  --record-sensitive
cargo run -p briefing-desk-demo -- eval compare repair-baseline repair-candidate-1
```

Record every gate. Accept only `eligible_for_review` after inspecting the diff,
one improved case, and one unchanged/regressed case. Otherwise restore the
baseline harness. Run sealed scorecard only if the candidate is eligible and
accepted.

- [ ] **Step 7: Export and commit sanitized evidence**

Run the evidence exporter for the two labels, secret-scan the output, run the
deterministic verifier, and place the canonical bundle under
`docs/review/evidence/v0_16_eval_repair/`. Confirm `git status` never includes
`evals/runs/`.

- [ ] **Step 8: Update report and README**

Separate historical and repaired experiments; record provider/model/commit,
hashes, baseline validity, candidate gates, token/cost/latency, human decision,
scorecard state, evidence path, exact current test counts, and limitations.

- [ ] **Step 9: Run complete verification**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
```

Expected: all four commands exit 0.

- [ ] **Step 10: Commit issue #292 once**

```bash
git add examples/demo/briefing-desk/src/eval/evidence.rs \
        examples/demo/briefing-desk/src/eval/mod.rs \
        examples/demo/briefing-desk/src/eval/cli.rs \
        examples/demo/briefing-desk/src/main.rs \
        examples/demo/briefing-desk/src/harness.rs \
        examples/demo/briefing-desk/README.md \
        docs/review/v0_16_eval_lab.md \
        docs/review/evidence/v0_16_eval_repair
git commit -m "docs: rerun eval validation and close out v0.16 (closes #292)"
```

---

## Final review, PR, merge, and archive

- [ ] Generate a full branch review package from merge-base to HEAD and obtain
      independent spec/code-quality review. Fix all Critical/Important findings
      in the owning issue commit without creating unattributed implementation
      commits.
- [ ] Re-run the four workspace checks after the final fix.
- [ ] Push `hotfix/2026_08_06b` and open one PR to `main` describing #289–#292,
      live evidence, and all verification commands.
- [ ] Confirm GitHub CI passes and merge the PR.
- [ ] On updated `main`, add the completed hotfix row/capability note to
      `docs/iteration/roadmap.md`.
- [ ] Move `docs/iteration/v0_16/` to
      `docs/archive/iteration/v0_16/` and `docs/hotfix/2026_08_06b/` to
      `docs/archive/hotfix/2026_08_06b/`.
- [ ] Commit and push the direct-main closeout as
      `docs: archive completed v0.16 eval repair docs`.
- [ ] Verify #289–#292 are closed, the PR is merged, main is clean, roadmap links
      resolve, and no sensitive run directory is tracked.
