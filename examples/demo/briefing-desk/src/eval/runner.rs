//! Eval run orchestration: preflight, repetitions, attempt lifecycle, grading.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use orchest::budget::BudgetConfig;
use orchest::model::ModelAdapter;
use orchest::run::SupervisionStrategy;
use orchest::tool::registry::ToolRegistry;
use serde_json::{json, Value};

use super::artifact::{
    attempt_record_template, build_manifest_skeleton, collect_git_info, create_run_dir,
    default_runs_dir, fixture_revision, now_unix_ms, preflight_dirty_paths,
    require_record_sensitive, write_attempt_artifacts, write_effective_config_snapshot,
    write_harness_snapshot, write_manifest, ArtifactError, AttemptRecord, AttemptStatus,
    HarnessSnapshot, ScoresPlaceholder, StoreCleanupRecord,
};
use super::case::{
    default_cases_path, default_fixtures_dir, default_seeds_dir, load_corpus, load_session_seed,
    CaseCorpus, EvalCase, EvalSplit, RunMode, SessionSeed, GATING_TAGS,
};
use super::compare::{AttemptResultRow, CaseResultRow, RunResults, RESULTS_SCHEMA_VERSION};
use super::effective_config::{
    fingerprint_registry, runtime_with_max_steps, CapabilityRoute, EffectiveConfigInput,
    EffectiveConfigSnapshot, SessionPersistenceMode, EFFECTIVE_CONFIG_SCHEMA_VERSION,
};
use super::resource::{collect_resources, mean_gate_tokens, median_latency_ms, ResourceCoverage};
use super::session::AttemptSession;
use super::trajectory::TrajectoryRecorder;
use crate::app::{self, ResumeArgs, RunArgs};
use crate::media;
use crate::tools::{ReadFixtureTool, SearchFixturesTool, WriteReportTool};

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

    let run_dir = create_run_dir(&req.runs_root, &req.label)?;
    let harness = HarnessSnapshot::capture_current();
    let harness_ref = write_harness_snapshot(&run_dir, &harness)?;
    let surface_hashes = harness.surface_hashes();

    let model: Arc<dyn ModelAdapter> = match &req.model {
        Some(m) => Arc::clone(m),
        None => app::live_chat_model().map_err(|e| EvalRunError::Other(e.to_string()))?,
    };

    let effective = build_effective_config(&req, &model)?;
    let eff_bytes = effective.normalize_bytes()?;
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
        json!({}),
        harness_ref,
        surface_hashes,
        eff_ref,
        EFFECTIVE_CONFIG_SCHEMA_VERSION.to_string(),
        session_seeds,
        case_ids,
        split_names,
        manifest_repetition,
    );
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
    let mut any_inconclusive = false;
    let mut any_resource_incomplete = false;
    let mut all_completed = true;

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
                Arc::clone(&model),
                &run_dir,
                &fixture_inventory,
            )
            .await;

            if outcome.resource.coverage == ResourceCoverage::Incomplete {
                any_resource_incomplete = true;
            }
            match outcome.record.status {
                AttemptStatus::Completed => {
                    if case.split == EvalSplit::Validation {
                        validation_gate_tokens.push(outcome.gate_total_tokens);
                        validation_completed_latencies.push(outcome.record.wall_latency_ms);
                    }
                }
                AttemptStatus::Inconclusive => {
                    any_inconclusive = true;
                    all_completed = false;
                    if case.split == EvalSplit::Validation {
                        validation_gate_tokens.push(outcome.gate_total_tokens);
                    }
                }
                AttemptStatus::ExecutionFailure => {
                    all_completed = false;
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
                grader_status: outcome.grader_status,
            });
        }

        let (case_passed, case_score) =
            aggregate_case_repetitions(case.must_pass, &attempt_passes, &attempt_scores, reps);
        if case_score.is_none() {
            all_completed = false;
        }

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
        any_inconclusive,
        any_resource_incomplete,
        all_completed,
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

#[allow(clippy::too_many_arguments)]
async fn execute_attempt(
    req: &EvalRunRequest,
    case: &EvalCase,
    attempt_no: u32,
    model: Arc<dyn ModelAdapter>,
    run_dir: &Path,
    fixture_inventory: &BTreeSet<String>,
) -> AttemptOutcome {
    let mut traj = TrajectoryRecorder::new();
    let started_unix = now_unix_ms();
    let outer_wall = Instant::now();

    let attempt_out = run_dir
        .join("cases")
        .join(&case.case_id)
        .join(attempt_no.to_string());
    let _ = fs::create_dir_all(&attempt_out);
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
            let result = app::run_with_model(args, model, |ev| traj.observe(ev)).await;
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
                            model,
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

    if traj.is_inconclusive() {
        record.status = AttemptStatus::Inconclusive;
        record.terminal_kind = Some("events_dropped_or_incomplete".into());
    }

    let output_md = match exec_result {
        Ok(text) => {
            if record.status != AttemptStatus::Inconclusive {
                record.status = AttemptStatus::Completed;
            }
            record.terminal_kind = record
                .terminal_kind
                .clone()
                .or(Some("run_completed".into()));
            text
        }
        Err(e) => {
            if record.status != AttemptStatus::Inconclusive {
                record.status = AttemptStatus::ExecutionFailure;
            }
            record.terminal_kind = Some("run_failed".into());
            record.error = Some(json!({"message": e}));
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

    if let Err(e) = write_attempt_artifacts(
        run_dir,
        &case.case_id,
        attempt_no,
        &traj,
        &output_md,
        &record,
        &scores,
    ) {
        eprintln!("[eval] failed writing attempt artifacts: {e}");
    }

    AttemptOutcome {
        record,
        score,
        passed,
        grader_status,
        gate_total_tokens: resource.totals.gate_total_tokens(),
        resource,
    }
}

#[allow(clippy::too_many_arguments)]
async fn materialize_and_resume(
    seed: &SessionSeed,
    question: &str,
    output_path: &Path,
    model: Arc<dyn ModelAdapter>,
    traj: &mut TrajectoryRecorder,
    store_cleanup: &mut StoreCleanupRecord,
) -> Result<(String, u64), String> {
    let base = app::main_agent_config().map_err(|e| e.to_string())?;
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
    let result = app::resume_with_model(snapshot, args, model, |ev| local.observe(ev)).await;
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
    // Call graders when module is wired; otherwise not_run.
    if super::grader_available() {
        return grade_with_grader(
            case,
            events,
            output_md,
            record,
            fixtures_dir,
            fixture_inventory,
        );
    }
    (ScoresPlaceholder::not_run(), None, None, "not_run".into())
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
pub fn aggregate_case_repetitions(
    must_pass: bool,
    passes: &[Option<bool>],
    scores: &[Option<f64>],
    expected_reps: u32,
) -> (Option<bool>, Option<f64>) {
    if passes.len() != expected_reps as usize || scores.len() != expected_reps as usize {
        return (None, None);
    }
    if passes.iter().any(|p| p.is_none()) || scores.iter().any(|s| s.is_none()) {
        return (None, None);
    }
    let pass_vals: Vec<bool> = passes.iter().map(|p| p.unwrap()).collect();
    let score_vals: Vec<f64> = scores.iter().map(|s| s.unwrap()).collect();
    let pass_count = pass_vals.iter().filter(|p| **p).count();
    let case_pass = if must_pass {
        pass_count == expected_reps as usize
    } else if expected_reps == 1 {
        pass_count == 1
    } else {
        pass_count * 3 >= 2 * expected_reps as usize
    };
    let mean = score_vals.iter().sum::<f64>() / score_vals.len() as f64;
    (Some(case_pass), Some(mean))
}

fn aggregate_split_scores(
    cases: &[CaseResultRow],
    splits: &[EvalSplit],
) -> (Option<f64>, BTreeMap<String, f64>) {
    let focus = if splits.contains(&EvalSplit::Validation) {
        Some(EvalSplit::Validation)
    } else {
        splits.first().copied()
    };
    let focus_str = focus.map(|s| s.as_str());

    let mut overall = None;
    if let Some(fs) = focus_str {
        let mut wsum = 0.0;
        let mut ssum = 0.0;
        let mut any_null = false;
        let mut any = false;
        for c in cases {
            if c.split != fs {
                continue;
            }
            any = true;
            match c.score {
                Some(s) => {
                    wsum += c.weight;
                    ssum += s * c.weight;
                }
                None => any_null = true,
            }
        }
        if any && !any_null && wsum > 0.0 {
            overall = Some(ssum / wsum);
        } else if any_null {
            overall = None;
        }
    }

    let mut per_tag = BTreeMap::new();
    for tag in GATING_TAGS {
        let key = tag.as_str();
        let mut wsum = 0.0;
        let mut ssum = 0.0;
        let mut any_null = false;
        let mut any = false;
        for c in cases {
            if c.split != EvalSplit::Validation.as_str() {
                continue;
            }
            if !c.tags.iter().any(|t| t == key) {
                continue;
            }
            any = true;
            match c.score {
                Some(s) => {
                    wsum += c.weight;
                    ssum += s * c.weight;
                }
                None => any_null = true,
            }
        }
        if any && !any_null && wsum > 0.0 {
            per_tag.insert(key.to_string(), ssum / wsum);
        }
    }
    (overall, per_tag)
}

fn build_effective_config(
    req: &EvalRunRequest,
    model: &Arc<dyn ModelAdapter>,
) -> Result<EffectiveConfigSnapshot, EvalRunError> {
    let corpus =
        media::discover(&req.materials_dir).map_err(|e| EvalRunError::Other(e.to_string()))?;
    let text_entries: Vec<(PathBuf, String)> = corpus
        .text
        .iter()
        .map(|path| {
            let content = fs::read_to_string(path)
                .map_err(|e| EvalRunError::Other(format!("reading {}: {e}", path.display())))?;
            Ok((path.clone(), content))
        })
        .collect::<Result<_, EvalRunError>>()?;

    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(SearchFixturesTool::new(text_entries)))
        .map_err(|e| EvalRunError::Other(e.to_string()))?;
    registry
        .register(Arc::new(ReadFixtureTool::new(corpus.text.clone())))
        .map_err(|e| EvalRunError::Other(e.to_string()))?;
    registry
        .register(Arc::new(WriteReportTool::new(PathBuf::from(
            "/tmp/briefing-desk-eval-fingerprint.md",
        ))))
        .map_err(|e| EvalRunError::Other(e.to_string()))?;

    let tools = fingerprint_registry(&registry)?;
    let provider = model.provider_name().to_string();
    let model_name = model.model_name().to_string();

    let input = EffectiveConfigInput {
        main_model_provider: provider.clone(),
        main_model_name: model_name.clone(),
        main_request_options: json!({}),
        main_runtime: runtime_with_max_steps(10),
        main_budget: BudgetConfig::default(),
        main_retry: None,
        main_supervision: SupervisionStrategy::Stop,
        main_hooks_label: "none".into(),
        main_session_store_label: "none_or_seed".into(),
        reviewer_model_provider: provider,
        reviewer_model_name: model_name,
        reviewer_request_options: json!({}),
        reviewer_max_steps: 2,
        reviewer_budget: BudgetConfig::default(),
        reviewer_retry: None,
        reviewer_supervision: SupervisionStrategy::Stop,
        tools,
        asr: CapabilityRoute::Fake,
        tts: CapabilityRoute::Disabled,
        vision: if corpus.images.is_empty() {
            CapabilityRoute::Disabled
        } else {
            CapabilityRoute::Fake
        },
        session_mode: SessionPersistenceMode::None,
        env_options: BTreeMap::from([
            (
                "chat_model".into(),
                json!(format!("{}/{}", model.provider_name(), model.model_name())),
            ),
            ("chat_api_url_set".into(), json!(false)),
            ("eval_no_tts".into(), json!(true)),
        ]),
    };
    EffectiveConfigSnapshot::from_input(input).map_err(EvalRunError::from)
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
        "- overall: {}\n- all_completed: {}\n- any_inconclusive: {}\n- any_resource_incomplete: {}\n",
        results
            .overall
            .map(|o| format!("{o:.2}"))
            .unwrap_or_else(|| "null".into()),
        results.all_completed,
        results.any_inconclusive,
        results.any_resource_incomplete,
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
