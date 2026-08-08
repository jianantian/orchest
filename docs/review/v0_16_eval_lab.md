# Briefing Desk Eval Lab — v0.16 Validation Report

Application-layer, human-driven harness evaluation for Briefing Desk. The
success criterion is a reproducible and auditable loop:

`run → record → grade → compare → human decision`

Finding an accepted prompt candidate is optional; preserving the registered
gates is not.

## Bottom line

The repaired loop completed end-to-end against a live DeepSeek model. The
formal baseline is valid: all 14 optimization/validation cases and all 22
attempts completed, every must-pass attempt passed, graders completed, resource
coverage is complete, and every attempt has the fixed four artifacts.

The pre-registered candidate was **not eligible** and was rejected. It preserved
quality at 100, but the mandatory `overall_plus_5` gate failed because the valid
baseline was already 100. It also increased validation tokens by 6.75% and
median latency by 4.14%, contrary to the resource-reduction hypothesis. The
candidate sentence was removed and the repaired baseline harness remains the
product default. The sealed scorecard was **not run**.

A committed allowlist-only evidence bundle is available at
[`evidence/v0_16_eval_repair/`](./evidence/v0_16_eval_repair/README.md). Its
verifier reconstructs both runs, revalidates result contracts, recomputes the
production compare gates, and checks the bundle/source hash chain.

## Historical pilot (2026-08-01)

The original v0.16 pilot correctly returned `invalid_baseline`: the baseline
missed required headings, fixture citations, and conflict attribution. Its
candidate scored higher and was retained as a product choice, but it was never
formally eligible because eligibility is not computed from an invalid baseline.
Those local artifacts are not canonical evidence for the final code and are not
committed.

This repaired experiment supersedes the pilot's product conclusion. It does not
rewrite the historical outcome: it establishes a new valid baseline on the
finalized grader/config/resource contracts, then evaluates one new,
pre-registered candidate.

## Repaired formal experiment

| Field | Value |
|------|-------|
| Date | 2026-08-08 (Asia/Shanghai) |
| Experiment git commit (both runs) | `b6883ab802df1d3ebde7a2beb3411c3992c39607` |
| Provider / model | `deepseek` / `deepseek-v4-flash` |
| Request max tokens | `4096` |
| Splits | optimization + validation |
| Repetitions | optimization 1; validation 3 |
| Cases / attempts | 14 / 22 |
| ASR / TTS | public fakes; not under test |
| Cost | `unknown` (provider calls did not report cost) |

### Identity and snapshot hashes

| Artifact | baseline | candidate-1 |
|----------|----------|-------------|
| harness snapshot | `8a4675a75237dfb20021811373adf4d257ec5fb8d531cbbedb86478cf2f9901b` | `0d9c4ecb7c6e3026251740f3e0db5dded66d350d6deee8a2bd8ef2d3659a1b0d` |
| effective config | `04d83841c79a22136f4d6f3503a906770ab5cc8171595e79daebab68a8bb56fe` | same |
| evidence bundle | `9832efbbf06ea93a52e5382688b8429ccaaa1793d83446766543538b9acd6906` | one bundle covers both |

Baseline and candidate have the same commit, provider/model, request options,
fixtures, case policies, session-seed hashes, repetitions, and effective-config
hash. The candidate manifest records only
`examples/demo/briefing-desk/src/harness.rs` as dirty, and only its harness hash
differs.

## Baseline repair and validity

Two diagnostic runs were rejected before the retained formal baseline:

1. Citation extraction treated the Markdown separator between two backtick
   citations as an absolute `/` path, and a valid conflict case exhausted the
   product's ten-step ceiling before its final write.
2. Citation extraction treated an extensionless metric identifier as a fixture
   path; stochastic reports could also omit a body-mentioned fixture from
   Sources or omit the intermediate chart value.

Each cause was reproduced with a failing test before repair. The retained
baseline runs on the resulting clean commit. Its validity is absolute:

| Metric | Baseline |
|--------|----------|
| Overall validation score | 100.0 |
| Must-pass failures | 0 |
| All attempts completed | true |
| Inconclusive attempts | 0 |
| Resource-incomplete attempts | 0 |
| Validation mean `gate_total_tokens` | 42,566.5 |
| Validation median wall latency | 53,760.5 ms |
| Validation cost | `unknown` |

All 14 case aggregates are 100.0. Self-compare reaches the normal gate path
(`not_eligible` only because a run cannot improve itself by +5), proving the
baseline is not `invalid_baseline` and is contract-comparable.

## Candidate-1

### Pre-registered hypothesis

Add one concise instruction to the existing main harness:

> Use the smallest sufficient Tool chain and reuse already-read evidence while
> preserving every required report section, conflict attribution, and real
> fixture citation.

Expected effect: reduce repeated Tool/model work and validation tokens without
decreasing any behavior tag or must-pass result. Only `src/harness.rs` differed.

### Formal comparison

| Metric | baseline | candidate-1 | candidate / baseline |
|--------|----------|-------------|----------------------|
| Overall validation score | 100.0 | 100.0 | delta 0.0 |
| Validation mean gate tokens | 42,566.5 | 45,439.67 | 1.0675 |
| Validation median latency | 53,760.5 ms | 55,987.5 ms | 1.0414 |
| Validation cost | `unknown` | `unknown` | unknown |

| Gate | Result |
|------|--------|
| candidate must-pass | pass |
| overall +5 | **FAIL** (`0.0 < 5.0`) |
| no per-tag decrease | pass |
| tokens ≤115% | pass |
| latency ≤130% | pass |
| no inconclusive/resource incomplete | pass |
| effective config equal | pass |

Formal status: `not_eligible`. There are no comparability mismatches and no
baseline or candidate must-pass failures. Since eligibility failed, the
candidate was rejected and its sentence was removed. No scorecard was run.

## Sanitized evidence contract

The committed bundle contains only typed, allowlisted data:

- stable run/case IDs, provider/model identity, safe request/effective config;
- fixture, harness, surface, seed, config, and source-identity hashes;
- per-attempt numeric score/resource/timing rows and run aggregates;
- compare gate IDs, numeric/structured actuals and thresholds;
- human decision (`not_eligible`) and scorecard state (`not_run`).

It does not contain trajectories, generated reports, Tool payloads, prompt
reasoning, grader evidence/failure text, credentials, or local filesystem paths.
The exporter never opens per-attempt raw files. Tampering with a numeric row,
config/source hash, comparison gate, bundle hash, or process decision makes
verification fail.

```bash
cargo run -p briefing-desk-demo -- eval verify-evidence \
  docs/review/evidence/v0_16_eval_repair/bundle.json
```

Raw `evals/runs/` directories remain local and gitignored.

## Verification

Briefing Desk package tests at closeout:

```text
158 unit + 8 eval_cli + 5 smoke
```

All required branch closeout checks passed on 2026-08-08:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
```

## Findings and limitations

- No `crates/`, provider, protocol, or binding code changed; all repairs are
  demo-local.
- The live run revealed real app-level limits: a ten-step product ceiling was
  insufficient for the required search/read/review/write sequence, and citation
  extraction must distinguish fixture filenames from Markdown separators and
  extensionless metric identifiers.
- Cost remains `unknown`; token gates still operate independently.
- This is one model and one fixture domain. It does not establish cross-domain
  generalization or prose quality beyond deterministic contracts.
- ASR/TTS adapters were not live-tested in this experiment.

The eval machinery should remain demo-local until another product reuses these
contracts.
