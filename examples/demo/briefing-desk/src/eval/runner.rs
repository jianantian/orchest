//! Eval run orchestration: preflight, repetitions, attempt lifecycle, grading.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use orchest::model::ModelAdapter;
use serde_json::{json, Value};

use super::artifact::{
    attempt_record_template, build_manifest_skeleton, collect_git_info, create_run_dir,
    default_runs_dir, fixture_revision, now_unix_ms, preflight_dirty_paths,
    require_record_sensitive, write_attempt_artifacts, write_effective_config_snapshot,
    write_harness_snapshot, write_manifest, ArtifactError, AttemptRecord, AttemptStatus,
    HarnessSnapshot, ManifestCasePolicy, ScoresPlaceholder, StoreCleanupRecord,
};
use super::case::{
    default_cases_path, default_fixtures_dir, default_seeds_dir, load_corpus, load_session_seed,
    BehaviorTag, CaseCorpus, EvalCase, EvalSplit, RunMode, SessionSeed, GATING_TAGS,
};

use super::compare::{
    derive_results_completeness, AttemptResultRow, CaseResultRow, RunResults,
    RESULTS_SCHEMA_VERSION,
};
use super::effective_config::{EffectiveConfigSnapshot, EFFECTIVE_CONFIG_SCHEMA_VERSION};
use super::resource::{collect_resources, mean_gate_tokens, median_latency_ms};
use super::session::AttemptSession;
use super::trajectory::{sanitize_free_text, TrajectoryRecorder};
use crate::app::{self, ResumeArgs, RunArgs};
use crate::execution::{self, ResolvedChatModel, ResolvedExecutionEnvironment};

/// Harness surface path suffix allowed to be dirty during eval.
pub const HARNESS_PATH_SUFFIX: &str = "src/harness.rs";

/// Default repetitions per split (contract).
pub fn default_repetitions(split: EvalSplit) -> u32 {
    match split {
        EvalSplit::Optimization => 1,
        EvalSplit::Validation | EvalSplit::Scorecard => 3,
    }
}

#[derive(Clone)]
pub struct EvalRunRequest {
    pub label: String,
    pub splits: Vec<EvalSplit>,
    pub record_sensitive: bool,
    pub confirm_sealed: bool,
    pub runs_root: PathBuf,
    pub cases_path: PathBuf,
    pub fixtures_dir: PathBuf,
    pub seeds_dir: PathBuf,
    pub materials_dir: PathBuf,
    pub repo_root: PathBuf,
    pub model: Option<Arc<dyn ModelAdapter>>,
    pub quiet: bool,
}

impl Default for EvalRunRequest {
    fn default() -> Self {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Self {
            label: String::new(),
            splits: vec![EvalSplit::Optimization, EvalSplit::Validation],
            record_sensitive: false,
            confirm_sealed: false,
            runs_root: default_runs_dir(),
            cases_path: default_cases_path(),
            fixtures_dir: default_fixtures_dir(),
            seeds_dir: default_seeds_dir(),
            materials_dir: default_fixtures_dir(),
            repo_root: manifest_dir
                .join("../../..")
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(".")),
            model: None,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EvalRunSummary {
    pub label: String,
    pub run_dir: PathBuf,
    pub case_count: usize,
    pub attempt_count: usize,
    pub sealed: bool,
    pub results: RunResults,
}

#[derive(Debug)]
pub enum EvalRunError {
    Preflight(String),
    Artifact(ArtifactError),
    Other(String),
}

impl std::fmt::Display for EvalRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalRunError::Preflight(m) | EvalRunError::Other(m) => write!(f, "{m}"),
            EvalRunError::Artifact(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EvalRunError {}

impl From<ArtifactError> for EvalRunError {
    fn from(e: ArtifactError) -> Self {
        EvalRunError::Artifact(e)
    }
}

pub fn parse_splits(raw: &str) -> Result<Vec<EvalSplit>, EvalRunError> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let split = match p {
            "optimization" => EvalSplit::Optimization,
            "validation" => EvalSplit::Validation,
            "scorecard" => EvalSplit::Scorecard,
            other => {
                return Err(EvalRunError::Preflight(format!(
                    "unknown split '{other}'; expected optimization, validation, scorecard"
                )));
            }
        };
        if !out.contains(&split) {
            out.push(split);
        }
    }
    if out.is_empty() {
        return Err(EvalRunError::Preflight(
            "at least one --split is required".into(),
        ));
    }
    Ok(out)
}

pub fn preflight(req: &EvalRunRequest) -> Result<CaseCorpus, EvalRunError> {
    require_record_sensitive(req.record_sensitive)
        .map_err(|e| EvalRunError::Preflight(e.to_string()))?;

    if req.splits.contains(&EvalSplit::Scorecard) && !req.confirm_sealed {
        return Err(EvalRunError::Preflight(
            "scorecard split requires explicit --confirm-sealed (sealed execution process contract)"
                .into(),
        ));
    }
    if req.splits.contains(&EvalSplit::Scorecard) && req.model.is_some() {
        // Injected/scripted models must not produce sealed scorecard artifacts.
        // Live scorecard requires env-constructed model (req.model is None here).
        return Err(EvalRunError::Preflight(
            "scorecard split rejects injected/scripted models; sealed runs require a live model"
                .into(),
        ));
    }

    let git = collect_git_info(&req.repo_root);
    preflight_dirty_paths(&git.dirty_paths, HARNESS_PATH_SUFFIX)
        .map_err(|e| EvalRunError::Preflight(e.to_string()))?;

    let existing = req.runs_root.join(&req.label);
    if existing.exists() {
        return Err(EvalRunError::Preflight(format!(
            "run label '{}' already exists at {}; refusing to overwrite",
            req.label,
            existing.display()
        )));
    }

    let corpus = load_corpus(&req.cases_path, &req.fixtures_dir, &req.seeds_dir)
        .map_err(|e| EvalRunError::Preflight(format!("corpus validation failed: {e}")))?;

    if req.model.is_none() {
        match std::env::var(app::CHAT_MODEL_ENV) {
            Ok(v) if !v.trim().is_empty() => {}
            _ => {
                return Err(EvalRunError::Preflight(format!(
                    "eval run requires {} (or an injected scripted model in tests); live eval refuses to skip",
                    app::CHAT_MODEL_ENV
                )));
            }
        }
    }

    Ok(corpus)
}

#[allow(clippy::too_many_lines)]
pub async fn run_eval(req: EvalRunRequest) -> Result<EvalRunSummary, EvalRunError> {
    let corpus = preflight(&req)?;
    let sealed = req.splits.contains(&EvalSplit::Scorecard) && req.confirm_sealed;

    let manifest_repetition = req
        .splits
        .iter()
        .map(|s| default_repetitions(*s))
        .max()
        .unwrap_or(1);

    let cases: Vec<EvalCase> = corpus
        .cases
        .iter()
        .filter(|c| req.splits.contains(&c.split))
        .cloned()
        .collect();
    if cases.is_empty() {
        return Err(EvalRunError::Preflight(
            "no cases match the requested splits".into(),
        ));
    }

    let env = match &req.model {
        Some(model) => ResolvedExecutionEnvironment::offline(
            ResolvedChatModel::injected(Arc::clone(model), json!({"max_tokens": 4096}), None)
                .map_err(|e| EvalRunError::Preflight(e.to_string()))?,
        ),
        None => ResolvedExecutionEnvironment::from_env()
            .map_err(|e| EvalRunError::Preflight(e.to_string()))?,
    };
    let effective = build_effective_config(&req, &cases, &env).await?;
    let eff_bytes = effective.normalize_bytes()?;
    let model = Arc::clone(&env.chat.adapter);

    let run_dir = create_run_dir(&req.runs_root, &req.label)?;
    let harness = HarnessSnapshot::capture_current();
    let harness_ref = write_harness_snapshot(&run_dir, &harness)?;
    let surface_hashes = harness.surface_hashes();

    let eff_ref = write_effective_config_snapshot(&run_dir, &eff_bytes)?;

    let mut session_seeds = BTreeMap::new();
    for case in &cases {
        if let RunMode::FollowUp {
            session_seed_id,
            session_seed_hash,
            ..
        } = &case.run
        {
            session_seeds.insert(session_seed_id.clone(), session_seed_hash.clone());
        }
    }

    let git = collect_git_info(&req.repo_root);
    let fix_rev = fixture_revision(&req.fixtures_dir)?;
    let case_ids: Vec<String> = cases.iter().map(|c| c.case_id.clone()).collect();
    let split_names: Vec<String> = req.splits.iter().map(|s| s.as_str().to_string()).collect();

    let mut manifest = build_manifest_skeleton(
        &req.label,
        git,
        fix_rev,
        Some(model.provider_name().to_string()),
        Some(model.model_name().to_string()),
        env.chat.request_options.clone(),
        harness_ref,
        surface_hashes,
        eff_ref,
        EFFECTIVE_CONFIG_SCHEMA_VERSION.to_string(),
        session_seeds,
        case_ids,
        split_names,
        manifest_repetition,
    );
    manifest.case_policies = cases
        .iter()
        .map(|case| {
            (
                case.case_id.clone(),
                ManifestCasePolicy {
                    split: case.split.as_str().to_string(),
                    must_pass: case.must_pass,
                    weight: case.weight,
                    tags: case
                        .tags
                        .iter()
                        .map(|tag| tag.as_str().to_string())
                        .collect(),
                },
            )
        })
        .collect();
    if sealed {
        manifest.request_options = json!({
            "sealed_scorecard": true,
            "confirm_sealed": true,
        });
    }
    write_manifest(&run_dir, &manifest)?;

    let fixture_inventory = list_fixture_basenames(&req.fixtures_dir)?;

    let mut case_rows: Vec<CaseResultRow> = Vec::new();
    let mut attempt_count = 0usize;
    let mut validation_gate_tokens: Vec<u64> = Vec::new();
    let mut validation_completed_latencies: Vec<u64> = Vec::new();
    let mut validation_costs: Vec<Option<f64>> = Vec::new();
    let mut validation_cost_complete = true;

    for case in &cases {
        let reps = default_repetitions(case.split);
        let mut attempts: Vec<AttemptResultRow> = Vec::new();
        let mut attempt_scores: Vec<Option<f64>> = Vec::new();
        let mut attempt_passes: Vec<Option<bool>> = Vec::new();

        for attempt_no in 1..=reps {
            attempt_count += 1;
            if !req.quiet {
                println!(
                    "[eval] case={} attempt={}/{}",
                    case.case_id, attempt_no, reps
                );
            }

            let outcome = execute_attempt(
                &req,
                case,
                attempt_no,
                env.clone(),
                &run_dir,
                &fixture_inventory,
            )
            .await?;

            if case.split == EvalSplit::Validation {
                validation_cost_complete &= outcome.resource.cost_complete;
                validation_costs.push(outcome.resource.totals.cost_usd);
            }
            match outcome.record.status {
                AttemptStatus::Completed => {
                    if case.split == EvalSplit::Validation {
                        validation_gate_tokens.push(outcome.gate_total_tokens);
                        validation_completed_latencies.push(outcome.record.wall_latency_ms);
                    }
                }
                AttemptStatus::Inconclusive => {
                    if case.split == EvalSplit::Validation {
                        validation_gate_tokens.push(outcome.gate_total_tokens);
                    }
                }
                AttemptStatus::ExecutionFailure => {
                    if case.split == EvalSplit::Validation {
                        validation_gate_tokens.push(outcome.gate_total_tokens);
                    }
                }
            }

            attempt_scores.push(outcome.score);
            attempt_passes.push(outcome.passed);
            attempts.push(AttemptResultRow {
                attempt: attempt_no,
                status: match outcome.record.status {
                    AttemptStatus::Completed => "completed".into(),
                    AttemptStatus::ExecutionFailure => "execution_failure".into(),
                    AttemptStatus::Inconclusive => "inconclusive".into(),
                },
                passed: outcome.passed,
                score: outcome.score,
                wall_latency_ms: outcome.record.wall_latency_ms,
                gate_total_tokens: outcome.gate_total_tokens,
                resource_coverage: outcome.resource.coverage.as_str().into(),
                cost_usd: outcome.resource.totals.cost_usd,
                cost_complete: outcome.resource.cost_complete,
                grader_status: outcome.grader_status,
            });
        }

        let (case_passed, case_score) =
            aggregate_case_repetitions(case.must_pass, &attempt_passes, &attempt_scores, reps);
        case_rows.push(CaseResultRow {
            case_id: case.case_id.clone(),
            split: case.split.as_str().into(),
            must_pass: case.must_pass,
            weight: case.weight,
            tags: case.tags.iter().map(|t| t.as_str().to_string()).collect(),
            passed: case_passed,
            score: case_score,
            attempts,
        });
    }

    let (overall, per_tag) = aggregate_split_scores(&case_rows, &req.splits);
    validation_cost_complete &=
        !validation_costs.is_empty() && validation_costs.iter().all(Option::is_some);
    let validation_total_cost_usd = validation_cost_complete.then(|| {
        validation_costs
            .iter()
            .filter_map(|cost| *cost)
            .sum::<f64>()
    });
    let validation_mean_cost_usd =
        validation_total_cost_usd.map(|total| total / validation_costs.len() as f64);
    let completeness = derive_results_completeness(&case_rows);

    let results = RunResults {
        schema_version: RESULTS_SCHEMA_VERSION.into(),
        label: req.label.clone(),
        splits: req.splits.iter().map(|s| s.as_str().to_string()).collect(),
        cases: case_rows,
        overall,
        per_tag,
        validation_mean_gate_tokens: mean_gate_tokens(&validation_gate_tokens),
        validation_median_latency_ms: median_latency_ms(&validation_completed_latencies),
        validation_attempt_gate_tokens: validation_gate_tokens,
        validation_completed_latencies_ms: validation_completed_latencies,
        validation_total_cost_usd,
        validation_mean_cost_usd,
        validation_cost_complete,
        any_inconclusive: completeness.any_inconclusive,
        any_resource_incomplete: completeness.any_resource_incomplete,
        all_completed: completeness.all_completed,
    };

    write_results_and_summary(&run_dir, &results, sealed)?;

    Ok(EvalRunSummary {
        label: req.label,
        run_dir,
        case_count: cases.len(),
        attempt_count,
        sealed,
        results,
    })
}

struct AttemptOutcome {
    record: AttemptRecord,
    score: Option<f64>,
    passed: Option<bool>,
    grader_status: String,
    gate_total_tokens: u64,
    resource: super::resource::ResourceReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEvidence {
    pub kind: String,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptEvidence {
    pub stream_started: bool,
    pub terminal: Option<TerminalEvidence>,
    pub events_dropped: bool,
    pub stream_closed_early: bool,
    pub cleanup_required: bool,
    pub cleanup_succeeded: bool,
    pub pre_run_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptClassification {
    pub status: AttemptStatus,
    pub terminal_kind: Option<String>,
    pub stop_reason: Option<String>,
}

/// Conservatively classify retained evidence without consulting execution text.
pub fn classify_attempt(evidence: &AttemptEvidence) -> AttemptClassification {
    let retained_kind = evidence.terminal.as_ref().map(|t| t.kind.clone());
    let retained_stop = evidence
        .terminal
        .as_ref()
        .and_then(|t| t.stop_reason.clone());
    if evidence.events_dropped {
        return AttemptClassification {
            status: AttemptStatus::Inconclusive,
            terminal_kind: retained_kind.or_else(|| Some("events_dropped".into())),
            stop_reason: retained_stop,
        };
    }
    if evidence.cleanup_required && !evidence.cleanup_succeeded {
        return AttemptClassification {
            status: AttemptStatus::Inconclusive,
            terminal_kind: retained_kind.or_else(|| Some("cleanup_failed".into())),
            stop_reason: retained_stop,
        };
    }
    if evidence.stream_closed_early {
        return AttemptClassification {
            status: AttemptStatus::Inconclusive,
            terminal_kind: retained_kind.or_else(|| Some("early_stream_closure".into())),
            stop_reason: retained_stop,
        };
    }
    if let Some(terminal) = &evidence.terminal {
        let status = if terminal.kind == "run_completed" {
            AttemptStatus::Completed
        } else {
            AttemptStatus::ExecutionFailure
        };
        return AttemptClassification {
            status,
            terminal_kind: Some(terminal.kind.clone()),
            stop_reason: terminal.stop_reason.clone(),
        };
    }
    if evidence.stream_started {
        return AttemptClassification {
            status: AttemptStatus::Inconclusive,
            terminal_kind: Some("missing_terminal".into()),
            stop_reason: None,
        };
    }
    if evidence.pre_run_failure {
        return AttemptClassification {
            status: AttemptStatus::ExecutionFailure,
            terminal_kind: Some("pre_run_failure".into()),
            stop_reason: None,
        };
    }
    AttemptClassification {
        status: AttemptStatus::Inconclusive,
        terminal_kind: Some("missing_terminal".into()),
        stop_reason: None,
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_attempt(
    req: &EvalRunRequest,
    case: &EvalCase,
    attempt_no: u32,
    env: ResolvedExecutionEnvironment,
    run_dir: &Path,
    fixture_inventory: &BTreeSet<String>,
) -> Result<AttemptOutcome, EvalRunError> {
    let mut traj = TrajectoryRecorder::new();
    let started_unix = now_unix_ms();
    let outer_wall = Instant::now();

    let attempt_out = run_dir
        .join("cases")
        .join(&case.case_id)
        .join(attempt_no.to_string());
    fs::create_dir_all(&attempt_out).map_err(|e| {
        EvalRunError::Artifact(ArtifactError::io(format!(
            "creating attempt directory {}: {e}",
            attempt_out.display()
        )))
    })?;
    let output_path = attempt_out.join("report.md");

    let mut record = attempt_record_template(&case.case_id, attempt_no, AttemptStatus::Completed);
    record.started_unix_ms = started_unix;

    let mut seed_id = None;
    let mut seed_hash = None;
    let mut store_cleanup = StoreCleanupRecord {
        attempted: false,
        succeeded: true,
        detail: None,
    };

    let exec_result: Result<String, String> = match &case.run {
        RunMode::Fresh { question } => {
            let args = RunArgs {
                materials: req.materials_dir.clone(),
                question: question.clone(),
                output: output_path.clone(),
                session: None,
                no_tts: true,
            };
            let wall_start = Instant::now();
            let result = app::run_with_environment(args, env, |ev| traj.observe(ev)).await;
            record.wall_latency_ms = wall_start.elapsed().as_millis() as u64;
            match result {
                Ok(o) => {
                    if output_path.is_file() {
                        Ok(fs::read_to_string(&output_path).unwrap_or(o.final_text))
                    } else {
                        Ok(o.final_text)
                    }
                }
                Err(e) => Err(e.to_string()),
            }
        }
        RunMode::FollowUp {
            question,
            session_seed_id,
            session_seed_hash,
        } => {
            seed_id = Some(session_seed_id.clone());
            seed_hash = Some(session_seed_hash.clone());
            let seed_path = req.seeds_dir.join(format!("{session_seed_id}.json"));
            match load_session_seed(&seed_path) {
                Err(e) => Err(format!("load seed: {e}")),
                Ok(seed) => match seed.content_hash() {
                    Err(e) => Err(format!("seed hash: {e}")),
                    Ok(actual) if actual != *session_seed_hash => Err(format!(
                        "session seed hash mismatch: expected {session_seed_hash}, got {actual}"
                    )),
                    Ok(_) => {
                        match materialize_and_resume(
                            &seed,
                            question,
                            &output_path,
                            &req.materials_dir,
                            env,
                            &mut traj,
                            &mut store_cleanup,
                        )
                        .await
                        {
                            Ok((text, wall_ms)) => {
                                record.wall_latency_ms = wall_ms;
                                Ok(text)
                            }
                            Err(e) => Err(e),
                        }
                    }
                },
            }
        }
    };

    if record.wall_latency_ms == 0 {
        record.wall_latency_ms = outer_wall.elapsed().as_millis() as u64;
    }
    record.ended_unix_ms = now_unix_ms();
    record.session_seed_id = seed_id;
    record.session_seed_hash = seed_hash;
    record.store_cleanup = store_cleanup;

    let pre_run_failure = exec_result.is_err()
        && !traj
            .events()
            .iter()
            .any(|event| event.kind == "run_started");
    let stream_closed_early = exec_result
        .as_ref()
        .err()
        .is_some_and(|error| error.contains("ended without producing output"));
    let evidence = attempt_evidence(
        &traj,
        matches!(case.run, RunMode::FollowUp { .. }),
        record.store_cleanup.succeeded,
        pre_run_failure,
        stream_closed_early,
    );
    let classification = classify_attempt(&evidence);
    record.status = classification.status;
    record.terminal_kind = classification.terminal_kind;
    record.stop_reason = classification.stop_reason;

    let output_md = match exec_result {
        Ok(text) => text,
        Err(e) => {
            record.error = Some(attempt_error_payload(&e));
            String::new()
        }
    };

    let resource = collect_resources(traj.events());
    record.tokens = resource.totals.to_json();
    record.resource_coverage = resource.to_json();

    let (scores, score, passed, grader_status) = grade_attempt_for_case(
        case,
        traj.events(),
        &output_md,
        &record,
        &req.fixtures_dir,
        fixture_inventory,
    );

    write_attempt_artifacts(
        run_dir,
        &case.case_id,
        attempt_no,
        &traj,
        &output_md,
        &record,
        &scores,
    )?;

    Ok(AttemptOutcome {
        record,
        score,
        passed,
        grader_status,
        gate_total_tokens: resource.totals.gate_total_tokens(),
        resource,
    })
}

/// Build a persisted execution-error payload through the shared redactor.
fn attempt_error_payload(message: &str) -> Value {
    json!({"message": sanitize_free_text(message)})
}

fn attempt_evidence(
    trajectory: &TrajectoryRecorder,
    cleanup_required: bool,
    cleanup_succeeded: bool,
    pre_run_failure: bool,
    stream_closed_early: bool,
) -> AttemptEvidence {
    let terminal = trajectory.terminal().map(|terminal| TerminalEvidence {
        kind: terminal.kind,
        stop_reason: terminal.stop_reason,
    });
    AttemptEvidence {
        stream_started: trajectory.stream_started(),
        terminal,
        events_dropped: trajectory.events_dropped(),
        stream_closed_early,
        cleanup_required,
        cleanup_succeeded,
        pre_run_failure,
    }
}

#[allow(clippy::too_many_arguments)]
async fn materialize_and_resume(
    seed: &SessionSeed,
    question: &str,
    output_path: &Path,
    materials_dir: &Path,
    env: ResolvedExecutionEnvironment,
    traj: &mut TrajectoryRecorder,
    store_cleanup: &mut StoreCleanupRecord,
) -> Result<(String, u64), String> {
    let base = execution::prepare_run(
        RunArgs {
            materials: materials_dir.to_path_buf(),
            question: "session seed preparation".into(),
            output: output_path.to_path_buf(),
            session: None,
            no_tts: true,
        },
        env.clone(),
    )
    .map_err(|e| e.to_string())?
    .config;
    let mut session = AttemptSession::materialize_async(seed, base)
        .await
        .map_err(|e| e.to_string())?;
    let snapshot = session.resume_snapshot();
    let args = ResumeArgs {
        session: session.session_id.clone(),
        question: question.to_string(),
        output: output_path.to_path_buf(),
        no_tts: true,
    };
    let wall_start = Instant::now();
    let mut local = TrajectoryRecorder::new();
    local.record_followup_session_resumed(&session.seed_id, &session.seed_hash);
    let result = app::resume_with_environment(snapshot, args, env, |ev| local.observe(ev)).await;
    let wall_ms = wall_start.elapsed().as_millis() as u64;
    *traj = local;
    *store_cleanup = session.cleanup().await;

    match result {
        Ok(o) => {
            let text = if output_path.is_file() {
                fs::read_to_string(output_path).unwrap_or(o.final_text)
            } else {
                o.final_text
            };
            Ok((text, wall_ms))
        }
        Err(e) => Err(e.to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
fn grade_attempt_for_case(
    case: &EvalCase,
    events: &[super::trajectory::TrajectoryEvent],
    output_md: &str,
    record: &AttemptRecord,
    fixtures_dir: &Path,
    fixture_inventory: &BTreeSet<String>,
) -> (ScoresPlaceholder, Option<f64>, Option<bool>, String) {
    grade_with_grader(
        case,
        events,
        output_md,
        record,
        fixtures_dir,
        fixture_inventory,
    )
}

#[allow(clippy::too_many_arguments)]
fn grade_with_grader(
    case: &EvalCase,
    events: &[super::trajectory::TrajectoryEvent],
    output_md: &str,
    record: &AttemptRecord,
    fixtures_dir: &Path,
    fixture_inventory: &BTreeSet<String>,
) -> (ScoresPlaceholder, Option<f64>, Option<bool>, String) {
    use super::grader::{grade_attempt, AttemptGraderStatus, GraderInput};

    if !matches!(record.status, AttemptStatus::Completed) {
        return (
            ScoresPlaceholder::not_run(),
            None,
            None,
            AttemptGraderStatus::NotRun.as_str().into(),
        );
    }

    let input = GraderInput {
        case,
        trajectory: events,
        output_md,
        fixtures_dir,
        fixture_inventory,
        attempt_status: record.status,
        session_seed_id: record.session_seed_id.as_deref(),
        session_seed_hash: record.session_seed_hash.as_deref(),
    };
    let grade = grade_attempt(&input);
    let scores = ScoresPlaceholder {
        grader_status: grade.grader_status.as_str().into(),
        aggregate: grade
            .aggregate
            .as_ref()
            .and_then(|a| serde_json::to_value(a).ok()),
        graders: grade
            .graders
            .iter()
            .filter_map(|g| serde_json::to_value(g).ok())
            .collect(),
    };
    let (score, passed) = match (&grade.grader_status, &grade.aggregate) {
        (AttemptGraderStatus::Completed, Some(agg)) => (Some(agg.score), Some(agg.passed)),
        _ => (None, None),
    };
    (scores, score, passed, grade.grader_status.as_str().into())
}

/// Case aggregation per PRD: must-pass requires all attempts pass; normal 2/3.
///
/// Thin adapter over [`crate::eval::grader::aggregate_case`] so the formula has
/// a single source of truth.
pub fn aggregate_case_repetitions(
    must_pass: bool,
    passes: &[Option<bool>],
    scores: &[Option<f64>],
    expected_reps: u32,
) -> (Option<bool>, Option<f64>) {
    use super::grader::{aggregate_case, AttemptAggregate, AttemptScoreInput};
    use crate::eval::case::{BehaviorTag, EvalCase, EvalSplit, RunMode, ToolConstraints};

    if passes.len() != expected_reps as usize || scores.len() != expected_reps as usize {
        return (None, None);
    }

    let attempts: Vec<AttemptScoreInput> = passes
        .iter()
        .zip(scores.iter())
        .map(|(p, s)| match (p, s) {
            (Some(passed), Some(score)) => AttemptScoreInput {
                grading_completed: true,
                attempt_completed: true,
                aggregate: Some(AttemptAggregate {
                    case_id: String::new(),
                    passed: *passed,
                    score: *score,
                    total_weight: 1.0,
                    required_passed: *passed,
                    grader_count: 1,
                }),
            },
            _ => AttemptScoreInput {
                grading_completed: false,
                attempt_completed: false,
                aggregate: None,
            },
        })
        .collect();

    // Only fields read by aggregate_case: must_pass, weight, tags, split, case_id.
    let case = EvalCase {
        case_id: String::new(),
        scenario_family: String::new(),
        tags: vec![BehaviorTag::ToolSelection],
        split: EvalSplit::Validation,
        run: RunMode::Fresh {
            question: "n/a".into(),
        },
        must_pass,
        weight: 1.0,
        tools: ToolConstraints::default(),
        order: vec![],
        expected_facts: vec![],
        expected_conflict: None,
        report: Default::default(),
        fixture_refs: vec![],
        graders: vec![],
        notes: None,
    };

    match aggregate_case(&case, &attempts, expected_reps as usize) {
        Ok(Some(agg)) => (Some(agg.passed), Some(agg.score)),
        Ok(None) | Err(_) => (None, None),
    }
}

pub(super) fn aggregate_split_scores(
    cases: &[CaseResultRow],
    splits: &[EvalSplit],
) -> (Option<f64>, BTreeMap<String, f64>) {
    use super::grader::{aggregate_split, aggregate_tag, CaseAggregate};

    let focus = if splits.contains(&EvalSplit::Validation) {
        Some(EvalSplit::Validation)
    } else {
        splits.first().copied()
    };

    let mut case_aggs = Vec::new();
    let mut any_null = false;
    for c in cases {
        let Some(split) = parse_split_label(&c.split) else {
            continue;
        };
        let (Some(score), Some(passed)) = (c.score, c.passed) else {
            any_null = true;
            continue;
        };
        let tags: Vec<BehaviorTag> = c
            .tags
            .iter()
            .filter_map(|t| BehaviorTag::parse(t))
            .collect();
        case_aggs.push(CaseAggregate {
            case_id: c.case_id.clone(),
            must_pass: c.must_pass,
            case_weight: c.weight,
            tags,
            split,
            passed,
            score,
            attempts_total: c.attempts.len(),
            attempts_passed: c.attempts.iter().filter(|a| a.passed == Some(true)).count(),
            attempts_present: c.attempts.len(),
        });
    }

    let overall = if any_null {
        None
    } else if let Some(split) = focus {
        aggregate_split(split, &case_aggs)
            .ok()
            .flatten()
            .map(|s| s.score)
    } else {
        None
    };

    let mut per_tag = BTreeMap::new();
    let val_null = cases
        .iter()
        .filter(|c| c.split == EvalSplit::Validation.as_str())
        .any(|c| c.score.is_none() || c.passed.is_none());
    if !val_null {
        let val_aggs: Vec<_> = case_aggs
            .iter()
            .filter(|c| c.split == EvalSplit::Validation)
            .cloned()
            .collect();
        for tag in GATING_TAGS {
            if let Ok(Some(tag_agg)) = aggregate_tag(tag, &val_aggs) {
                per_tag.insert(tag.as_str().to_string(), tag_agg.score);
            }
        }
    }
    (overall, per_tag)
}

fn parse_split_label(s: &str) -> Option<EvalSplit> {
    match s {
        "optimization" => Some(EvalSplit::Optimization),
        "validation" => Some(EvalSplit::Validation),
        "scorecard" => Some(EvalSplit::Scorecard),
        _ => None,
    }
}

async fn build_effective_config(
    req: &EvalRunRequest,
    cases: &[EvalCase],
    env: &ResolvedExecutionEnvironment,
) -> Result<EffectiveConfigSnapshot, EvalRunError> {
    let mut profiles = Vec::with_capacity(cases.len());
    for case in cases {
        let output = PathBuf::from("briefing-desk-eval-preflight.md");
        let mut profile = match &case.run {
            RunMode::Fresh { question } => {
                execution::prepare_run(
                    RunArgs {
                        materials: req.materials_dir.clone(),
                        question: question.clone(),
                        output,
                        session: None,
                        no_tts: true,
                    },
                    env.clone(),
                )
                .map_err(|e| EvalRunError::Preflight(e.to_string()))?
                .profile
            }
            RunMode::FollowUp {
                question,
                session_seed_id,
                session_seed_hash,
            } => {
                let seed_path = req.seeds_dir.join(format!("{session_seed_id}.json"));
                let seed = load_session_seed(&seed_path)
                    .map_err(|e| EvalRunError::Preflight(format!("load seed: {e}")))?;
                let actual = seed
                    .content_hash()
                    .map_err(|e| EvalRunError::Preflight(format!("seed hash: {e}")))?;
                if actual != *session_seed_hash {
                    return Err(EvalRunError::Preflight(format!(
                        "session seed hash mismatch: expected {session_seed_hash}, got {actual}"
                    )));
                }
                let base = execution::prepare_run(
                    RunArgs {
                        materials: req.materials_dir.clone(),
                        question: "session seed preparation".into(),
                        output: output.clone(),
                        session: None,
                        no_tts: true,
                    },
                    env.clone(),
                )
                .map_err(|e| EvalRunError::Preflight(e.to_string()))?
                .config;
                let mut session = AttemptSession::materialize_async(&seed, base)
                    .await
                    .map_err(EvalRunError::from)?;
                let prepared = execution::prepare_resume(
                    session.resume_snapshot(),
                    ResumeArgs {
                        session: session.session_id.clone(),
                        question: question.clone(),
                        output,
                        no_tts: true,
                    },
                    env.clone(),
                )
                .map_err(|e| EvalRunError::Preflight(e.to_string()))?;
                let cleanup = session.cleanup().await;
                if !cleanup.succeeded {
                    return Err(EvalRunError::Preflight(format!(
                        "preflight session cleanup failed: {:?}",
                        cleanup.detail
                    )));
                }
                prepared.profile
            }
        };
        profile.case_ids = vec![case.case_id.clone()];
        profiles.push(profile);
    }
    EffectiveConfigSnapshot::from_profiles(profiles, execution::resolved_env_options(&env.chat))
        .map_err(EvalRunError::from)
}

fn write_results_and_summary(
    run_dir: &Path,
    results: &RunResults,
    sealed: bool,
) -> Result<(), EvalRunError> {
    let mut bytes = serde_json::to_vec_pretty(results)
        .map_err(|e| EvalRunError::Other(format!("serialize results: {e}")))?;
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    fs::write(run_dir.join("results.json"), &bytes)
        .map_err(|e| EvalRunError::Other(format!("write results.json: {e}")))?;

    let mut md = String::new();
    md.push_str(&format!("# Eval Run `{}`\n\n", results.label));
    if sealed {
        md.push_str(
            "**Sealed scorecard execution** (process contract; not a security boundary).\n\n",
        );
    }
    md.push_str(&format!(
        "- overall: {}\n- all_completed: {}\n- any_inconclusive: {}\n- any_resource_incomplete: {}\n- validation cost: {}\n",
        results
            .overall
            .map(|o| format!("{o:.2}"))
            .unwrap_or_else(|| "null".into()),
        results.all_completed,
        results.any_inconclusive,
        results.any_resource_incomplete,
        format_run_cost(results),
    ));
    md.push_str("\n## Cases\n\n");
    for c in &results.cases {
        md.push_str(&format!(
            "- `{}` split={} must_pass={} passed={:?} score={:?}\n",
            c.case_id, c.split, c.must_pass, c.passed, c.score
        ));
    }
    fs::write(run_dir.join("summary.md"), md)
        .map_err(|e| EvalRunError::Other(format!("write summary.md: {e}")))?;
    Ok(())
}

fn format_run_cost(results: &RunResults) -> String {
    match (
        results.validation_cost_complete,
        results.validation_total_cost_usd,
        results.validation_mean_cost_usd,
    ) {
        (true, Some(total), Some(mean)) => format!("total=${total:.4}, mean=${mean:.4}"),
        _ => "unknown".into(),
    }
}

fn list_fixture_basenames(dir: &Path) -> Result<BTreeSet<String>, EvalRunError> {
    let mut out = BTreeSet::new();
    let rd = fs::read_dir(dir)
        .map_err(|e| EvalRunError::Other(format!("reading fixtures {}: {e}", dir.display())))?;
    for ent in rd {
        let ent = ent.map_err(|e| EvalRunError::Other(format!("fixtures entry: {e}")))?;
        if ent.file_type().map(|t| t.is_file()).unwrap_or(false) {
            out.insert(ent.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::credential::text_has_credentials;
    use crate::eval::scripted_model::{ScriptedModel, ScriptedTurn};

    fn completed_evidence() -> AttemptEvidence {
        AttemptEvidence {
            stream_started: true,
            terminal: Some(TerminalEvidence {
                kind: "run_completed".into(),
                stop_reason: Some("end_turn".into()),
            }),
            events_dropped: false,
            stream_closed_early: false,
            cleanup_required: false,
            cleanup_succeeded: true,
            pre_run_failure: false,
        }
    }

    #[test]
    fn classify_completed_retains_terminal_and_stop_reason() {
        let result = classify_attempt(&completed_evidence());
        assert_eq!(result.status, AttemptStatus::Completed);
        assert_eq!(result.terminal_kind.as_deref(), Some("run_completed"));
        assert_eq!(result.stop_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn classify_retained_run_failed_as_execution_failure() {
        let mut evidence = completed_evidence();
        evidence.terminal = Some(TerminalEvidence {
            kind: "run_failed".into(),
            stop_reason: None,
        });
        let result = classify_attempt(&evidence);
        assert_eq!(result.status, AttemptStatus::ExecutionFailure);
        assert_eq!(result.terminal_kind.as_deref(), Some("run_failed"));
        assert_eq!(result.stop_reason, None);
    }

    #[test]
    fn classify_drop_missing_terminal_early_close_and_cleanup_as_inconclusive() {
        let mut dropped = completed_evidence();
        dropped.events_dropped = true;
        assert_eq!(
            classify_attempt(&dropped).status,
            AttemptStatus::Inconclusive
        );

        let missing = AttemptEvidence {
            terminal: None,
            ..completed_evidence()
        };
        let result = classify_attempt(&missing);
        assert_eq!(result.status, AttemptStatus::Inconclusive);
        assert_eq!(result.terminal_kind.as_deref(), Some("missing_terminal"));

        let mut early = completed_evidence();
        early.stream_closed_early = true;
        assert_eq!(classify_attempt(&early).status, AttemptStatus::Inconclusive);

        let mut cleanup = completed_evidence();
        cleanup.cleanup_required = true;
        cleanup.cleanup_succeeded = false;
        let result = classify_attempt(&cleanup);
        assert_eq!(result.status, AttemptStatus::Inconclusive);
        assert_eq!(result.terminal_kind.as_deref(), Some("run_completed"));
        assert_eq!(result.stop_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn classify_pre_run_failure_without_started_stream_as_execution_failure() {
        let evidence = AttemptEvidence {
            stream_started: false,
            terminal: None,
            events_dropped: false,
            stream_closed_early: false,
            cleanup_required: false,
            cleanup_succeeded: true,
            pre_run_failure: true,
        };
        let result = classify_attempt(&evidence);
        assert_eq!(result.status, AttemptStatus::ExecutionFailure);
        assert_eq!(result.terminal_kind.as_deref(), Some("pre_run_failure"));
    }

    #[test]
    fn runner_error_payloads_redact_provider_model_and_pre_run_canaries() {
        let canary = "CREDENTIAL-CANARY-9e97d2";
        for message in [
            format!("provider failed: --api-key {canary}"),
            format!("model failed: Authorization: Bearer {canary}"),
            format!(
                "pre-run failed: https://api.example.test/run?carrier=--client-secret%20{canary}"
            ),
        ] {
            let payload = attempt_error_payload(&message);
            let message = payload["message"].as_str().unwrap();
            assert!(!message.contains(canary), "canary leaked in {message}");
            assert!(
                !text_has_credentials(message),
                "detector still sees credentials in {message}"
            );
        }
    }

    #[tokio::test]
    async fn provider_failure_retains_run_failed_terminal() {
        let materials = tempfile::tempdir().unwrap();
        std::fs::write(materials.path().join("notes.md"), "evidence").unwrap();
        let model = Arc::new(ScriptedModel::new(vec![ScriptedTurn::Error {
            message: "provider unavailable".into(),
        }]));
        let adapter: Arc<dyn ModelAdapter> = model.clone();
        let chat = ResolvedChatModel::injected(adapter, json!({"max_tokens": 4096}), None).unwrap();
        let mut trajectory = TrajectoryRecorder::new();
        let result = app::run_with_environment(
            RunArgs {
                materials: materials.path().to_path_buf(),
                question: "question".into(),
                output: materials.path().join("report.md"),
                session: None,
                no_tts: true,
            },
            ResolvedExecutionEnvironment::offline(chat),
            |event| trajectory.observe(event),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(model.call_count(), 1);
        let evidence = attempt_evidence(&trajectory, false, true, false, false);
        let classified = classify_attempt(&evidence);
        assert_eq!(classified.status, AttemptStatus::ExecutionFailure);
        assert_eq!(classified.terminal_kind.as_deref(), Some("run_failed"));
    }

    #[tokio::test]
    async fn sensitive_preflight_failure_keeps_model_call_count_zero() {
        let model = Arc::new(ScriptedModel::followup_text("unused"));
        let adapter: Arc<dyn ModelAdapter> = model.clone();
        let req = EvalRunRequest {
            label: "preflight-no-sensitive".into(),
            record_sensitive: false,
            model: Some(adapter),
            ..EvalRunRequest::default()
        };
        let result = run_eval(req).await;
        assert!(result.is_err());
        assert_eq!(model.call_count(), 0);
    }

    #[test]
    fn parse_splits_ok() {
        let s = parse_splits("optimization,validation").unwrap();
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn parse_splits_unknown() {
        assert!(parse_splits("train").is_err());
    }

    #[test]
    fn case_agg_must_pass_requires_all() {
        let (p, s) = aggregate_case_repetitions(
            true,
            &[Some(true), Some(true), Some(false)],
            &[Some(100.0), Some(100.0), Some(0.0)],
            3,
        );
        assert_eq!(p, Some(false));
        assert!(s.is_some());
    }

    #[test]
    fn case_agg_normal_two_of_three() {
        let (p, _) = aggregate_case_repetitions(
            false,
            &[Some(true), Some(true), Some(false)],
            &[Some(100.0), Some(100.0), Some(0.0)],
            3,
        );
        assert_eq!(p, Some(true));
    }

    #[test]
    fn case_agg_null_when_incomplete() {
        let (p, s) = aggregate_case_repetitions(
            false,
            &[Some(true), None, Some(true)],
            &[Some(100.0), None, Some(100.0)],
            3,
        );
        assert!(p.is_none());
        assert!(s.is_none());
    }

    #[test]
    fn preflight_requires_sensitive() {
        let mut req = EvalRunRequest {
            label: "t".into(),
            record_sensitive: false,
            ..EvalRunRequest::default()
        };
        req.model = Some(Arc::new(
            super::super::scripted_model::ScriptedModel::followup_text("x"),
        ));
        let err = preflight(&req).unwrap_err();
        assert!(err.to_string().contains("record-sensitive"));
    }

    #[test]
    fn preflight_scorecard_needs_sealed() {
        let mut req = EvalRunRequest {
            label: "t".into(),
            record_sensitive: true,
            confirm_sealed: false,
            splits: vec![EvalSplit::Scorecard],
            ..EvalRunRequest::default()
        };
        req.model = Some(Arc::new(
            super::super::scripted_model::ScriptedModel::followup_text("x"),
        ));
        let err = preflight(&req).unwrap_err();
        assert!(err.to_string().contains("confirm-sealed"));
    }
}
