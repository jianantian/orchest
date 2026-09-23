//! Eval Lab CLI: `eval run` and `eval compare`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use super::artifact::default_runs_dir;
use super::case::{default_cases_path, default_fixtures_dir, default_seeds_dir};
use super::compare::{compare_runs, load_run, write_compare_report, CompareStatus};
use super::evidence::{export_evidence_bundle, verify_evidence_file, write_evidence_bundle};
use super::runner::{parse_splits, run_eval, EvalRunRequest};
use super::scripted_model::ScriptedModel;

#[derive(Debug, Clone)]
pub struct EvalRunCli {
    pub label: String,
    pub split: String,
    pub record_sensitive: bool,
    pub confirm_sealed: bool,
    pub runs_dir: Option<PathBuf>,
    pub cases: Option<PathBuf>,
    pub fixtures: Option<PathBuf>,
    pub seeds: Option<PathBuf>,
    pub materials: Option<PathBuf>,
    pub repo_root: Option<PathBuf>,
    /// Test-only: use scripted model instead of live env.
    pub scripted: bool,
}

#[derive(Debug, Clone)]
pub struct EvalCompareCli {
    pub baseline: String,
    pub candidate: String,
    pub runs_dir: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct EvalExportEvidenceCli {
    pub baseline: String,
    pub candidate: String,
    pub runs_dir: Option<PathBuf>,
    pub out_dir: PathBuf,
    pub decision: String,
    pub scorecard_state: String,
}

pub fn cmd_eval_export_evidence(args: EvalExportEvidenceCli) -> ExitCode {
    let runs_root = args.runs_dir.unwrap_or_else(default_runs_dir);
    let bundle = match export_evidence_bundle(
        &runs_root,
        &args.baseline,
        &args.candidate,
        &args.decision,
        &args.scorecard_state,
    ) {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("eval evidence export failed: {error}");
            return ExitCode::from(1);
        }
    };
    match write_evidence_bundle(&args.out_dir, &bundle) {
        Ok((json, hash, index)) => {
            println!("[eval] wrote {}", json.display());
            println!("[eval] wrote {}", hash.display());
            println!("[eval] wrote {}", index.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("eval evidence export failed: {error}");
            ExitCode::from(1)
        }
    }
}

pub fn cmd_eval_verify_evidence(bundle: PathBuf) -> ExitCode {
    match verify_evidence_file(&bundle) {
        Ok(verified) => {
            println!(
                "[eval] evidence verified status={} baseline_overall={:?} candidate_overall={:?}",
                match verified.status {
                    CompareStatus::EligibleForReview => "eligible_for_review",
                    CompareStatus::NotEligible => "not_eligible",
                    CompareStatus::InvalidBaseline => "invalid_baseline",
                    CompareStatus::Incomparable => "incomparable",
                },
                verified.baseline_overall,
                verified.candidate_overall,
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("eval evidence verify failed: {error}");
            ExitCode::from(1)
        }
    }
}

pub async fn cmd_eval_run(args: EvalRunCli) -> ExitCode {
    let splits = match parse_splits(&args.split) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("eval run preflight failed: {e}");
            return ExitCode::from(2);
        }
    };

    let mut req = EvalRunRequest {
        label: args.label,
        splits,
        record_sensitive: args.record_sensitive,
        confirm_sealed: args.confirm_sealed,
        runs_root: args.runs_dir.unwrap_or_else(default_runs_dir),
        cases_path: args.cases.unwrap_or_else(default_cases_path),
        fixtures_dir: args.fixtures.unwrap_or_else(default_fixtures_dir),
        seeds_dir: args.seeds.unwrap_or_else(default_seeds_dir),
        materials_dir: args.materials.unwrap_or_else(default_fixtures_dir),
        repo_root: args.repo_root.unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from("."))
        }),
        model: None,
        quiet: false,
    };

    if args.scripted {
        let report = sample_report_markdown();
        req.model = Some(Arc::new(ScriptedModel::briefing_happy_path(&report)));
    }

    match run_eval(req).await {
        Ok(summary) => {
            println!(
                "[eval] completed label={} cases={} attempts={} sealed={} dir={}",
                summary.label,
                summary.case_count,
                summary.attempt_count,
                summary.sealed,
                summary.run_dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("eval run failed: {e}");
            ExitCode::from(1)
        }
    }
}

pub fn cmd_eval_compare(args: EvalCompareCli) -> ExitCode {
    let runs_root = args.runs_dir.unwrap_or_else(default_runs_dir);
    let baseline = match load_run(&runs_root, &args.baseline) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("eval compare failed loading baseline: {e}");
            return ExitCode::from(1);
        }
    };
    let candidate = match load_run(&runs_root, &args.candidate) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("eval compare failed loading candidate: {e}");
            return ExitCode::from(1);
        }
    };

    let report = compare_runs(&baseline, &candidate);
    let out_dir = args.out_dir.unwrap_or_else(|| runs_root.join("_compare"));
    match write_compare_report(&out_dir, &report) {
        Ok((json_path, md_path)) => {
            println!("[eval] compare status={}", report.status.as_str());
            println!("[eval] wrote {}", json_path.display());
            println!("[eval] wrote {}", md_path.display());
        }
        Err(e) => {
            eprintln!("eval compare failed writing report: {e}");
            return ExitCode::from(1);
        }
    }

    match report.status {
        CompareStatus::EligibleForReview => ExitCode::SUCCESS,
        status @ (CompareStatus::NotEligible
        | CompareStatus::InvalidBaseline
        | CompareStatus::Incomparable) => {
            // Surface the reason on stderr so integration tests and CI logs are
            // not left with an empty stderr when exit status is non-zero.
            let failed: Vec<&str> = report
                .gates
                .iter()
                .filter(|gate| !gate.passed)
                .map(|gate| gate.gate_id.as_str())
                .collect();
            if failed.is_empty() {
                eprintln!(
                    "eval compare status={} mismatches={}",
                    status.as_str(),
                    report.mismatches.len()
                );
            } else {
                eprintln!(
                    "eval compare status={} failed_gates={}",
                    status.as_str(),
                    failed.join(",")
                );
            }
            ExitCode::from(1)
        }
    }
}

fn sample_report_markdown() -> String {
    r#"# Executive Summary

Loom referral retention is 42% in Q3 per finance-reconciled dashboard and chart.

# Recommendation

Continue Q4 investment with clear conflict reconciliation.

# Evidence

- 42% from 001-retention-dashboard-notes.md and chart.png
- Conflicting 35% from 002-support-ticket-summary.md is unreconciled

# Risks

- Competitor pricing still TBD in 003-competitor-scan.md

# Sources

- 001-retention-dashboard-notes.md
- 002-support-ticket-summary.md
- chart.png
- interview.wav
"#
    .to_string()
}
