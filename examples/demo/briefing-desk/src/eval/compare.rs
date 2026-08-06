//! Baseline vs candidate comparison and acceptance gates.
//!
//! Never auto-edits harness. Outputs machine JSON + human Markdown only.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::artifact::{hex_sha256, RunManifest, MANIFEST_SCHEMA_VERSION};
use super::case::GATING_TAGS;
use super::resource::{mean_gate_tokens, median_latency_ms};
use super::runner::{aggregate_case_repetitions, aggregate_split_scores};

/// Final compare status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareStatus {
    EligibleForReview,
    NotEligible,
    InvalidBaseline,
    Incomparable,
}

impl CompareStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CompareStatus::EligibleForReview => "eligible_for_review",
            CompareStatus::NotEligible => "not_eligible",
            CompareStatus::InvalidBaseline => "invalid_baseline",
            CompareStatus::Incomparable => "incomparable",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub gate_id: String,
    pub passed: bool,
    pub actual: Value,
    pub threshold: Value,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareReport {
    pub schema_version: String,
    pub status: CompareStatus,
    pub baseline_label: String,
    pub candidate_label: String,
    pub baseline_cost: CostSummary,
    pub candidate_cost: CostSummary,
    pub gates: Vec<GateResult>,
    pub mismatches: Vec<String>,
    pub baseline_must_pass_failures: Vec<String>,
    pub effective_config_diff: Option<Value>,
    pub notes: Vec<String>,
}

/// Validation cost is informational. It is intentionally excluded from gates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostSummary {
    pub complete: bool,
    #[serde(with = "value_or_unknown")]
    pub total_usd: Option<f64>,
    #[serde(with = "value_or_unknown")]
    pub mean_usd: Option<f64>,
}

impl CostSummary {
    fn from_results(results: &RunResults) -> Self {
        Self {
            complete: results.validation_cost_complete,
            total_usd: results.validation_total_cost_usd,
            mean_usd: results.validation_mean_cost_usd,
        }
    }

    fn display(&self) -> String {
        match (self.complete, self.total_usd, self.mean_usd) {
            (true, Some(total), Some(mean)) => {
                format!("total=${total:.4}, mean=${mean:.4}")
            }
            _ => "unknown".into(),
        }
    }
}

impl CompareReport {
    #[allow(dead_code)]
    pub fn to_json_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Eval Compare Report\n\n");
        out.push_str(&format!("- **status**: `{}`\n", self.status.as_str()));
        out.push_str(&format!("- **baseline**: `{}`\n", self.baseline_label));
        out.push_str(&format!("- **candidate**: `{}`\n\n", self.candidate_label));
        out.push_str(&format!(
            "- **baseline validation cost**: `{}`\n- **candidate validation cost**: `{}`\n\n",
            self.baseline_cost.display(),
            self.candidate_cost.display(),
        ));

        if !self.mismatches.is_empty() {
            out.push_str("## Comparability mismatches\n\n");
            for m in &self.mismatches {
                out.push_str(&format!("- {m}\n"));
            }
            out.push('\n');
        }

        if !self.baseline_must_pass_failures.is_empty() {
            out.push_str("## Invalid baseline (must-pass failures)\n\n");
            for f in &self.baseline_must_pass_failures {
                out.push_str(&format!("- {f}\n"));
            }
            out.push('\n');
        }

        if let Some(diff) = &self.effective_config_diff {
            out.push_str("## Effective-config field diff\n\n```json\n");
            out.push_str(&serde_json::to_string_pretty(diff).unwrap_or_default());
            out.push_str("\n```\n\n");
        }

        out.push_str("## Gates\n\n");
        out.push_str("| gate | passed | actual | threshold | detail |\n");
        out.push_str("|------|--------|--------|-----------|--------|\n");
        for g in &self.gates {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                g.gate_id,
                if g.passed { "pass" } else { "FAIL" },
                compact(&g.actual),
                compact(&g.threshold),
                g.detail.replace('|', "/")
            ));
        }
        out.push('\n');

        if !self.notes.is_empty() {
            out.push_str("## Notes\n\n");
            for n in &self.notes {
                out.push_str(&format!("- {n}\n"));
            }
            out.push('\n');
        }

        out.push_str("\nThis report never auto-accepts a candidate or edits harness surfaces.\n");
        out
    }
}

fn compact(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "?".into()),
    }
}

/// Loaded run directory for comparison.
#[derive(Debug, Clone)]
pub struct LoadedRun {
    pub label: String,
    pub dir: PathBuf,
    pub manifest: RunManifest,
    pub results: RunResults,
    pub effective_config_bytes: Vec<u8>,
    pub effective_config_hash: String,
    pub harness_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunResults {
    pub schema_version: String,
    pub label: String,
    pub splits: Vec<String>,
    pub cases: Vec<CaseResultRow>,
    pub overall: Option<f64>,
    pub per_tag: BTreeMap<String, f64>,
    pub validation_mean_gate_tokens: Option<f64>,
    pub validation_median_latency_ms: Option<f64>,
    pub validation_attempt_gate_tokens: Vec<u64>,
    pub validation_completed_latencies_ms: Vec<u64>,
    #[serde(with = "value_or_unknown")]
    pub validation_total_cost_usd: Option<f64>,
    #[serde(with = "value_or_unknown")]
    pub validation_mean_cost_usd: Option<f64>,
    pub validation_cost_complete: bool,
    pub any_inconclusive: bool,
    pub any_resource_incomplete: bool,
    pub all_completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResultRow {
    pub case_id: String,
    pub split: String,
    pub must_pass: bool,
    pub weight: f64,
    pub tags: Vec<String>,
    pub passed: Option<bool>,
    pub score: Option<f64>,
    pub attempts: Vec<AttemptResultRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptResultRow {
    pub attempt: u32,
    pub status: String,
    pub passed: Option<bool>,
    pub score: Option<f64>,
    pub wall_latency_ms: u64,
    pub gate_total_tokens: u64,
    pub resource_coverage: String,
    pub cost_usd: Option<f64>,
    pub cost_complete: bool,
    pub grader_status: String,
}

pub const COMPARE_SCHEMA_VERSION: &str = "2";
pub const RESULTS_SCHEMA_VERSION: &str = "2";

mod value_or_unknown {
    use super::*;

    pub fn serialize<S>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer.serialize_f64(*value),
            None => serializer.serialize_str("unknown"),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Number(number) => number
                .as_f64()
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map(Some)
                .ok_or_else(|| D::Error::custom("cost must be a finite nonnegative number")),
            Value::String(value) if value == "unknown" => Ok(None),
            _ => Err(D::Error::custom(
                "cost must be a finite nonnegative number or the string 'unknown'",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ResultsCompleteness {
    pub all_completed: bool,
    pub any_inconclusive: bool,
    pub any_resource_incomplete: bool,
}

pub(super) fn derive_results_completeness(cases: &[CaseResultRow]) -> ResultsCompleteness {
    let mut attempts = cases.iter().flat_map(|case| &case.attempts);
    let all_completed = cases
        .iter()
        .all(|case| case.passed.is_some() && case.score.is_some())
        && attempts.clone().all(|attempt| {
            attempt.status == "completed"
                && attempt.grader_status == "completed"
                && attempt.passed.is_some()
                && attempt.score.is_some()
        });
    let any_inconclusive = attempts
        .clone()
        .any(|attempt| attempt.status == "inconclusive" || attempt.grader_status == "inconclusive");
    let any_resource_incomplete = attempts.any(|attempt| attempt.resource_coverage != "complete");
    ResultsCompleteness {
        all_completed,
        any_inconclusive,
        any_resource_incomplete,
    }
}

fn option_f64_matches(actual: Option<f64>, expected: Option<f64>) -> bool {
    match (actual, expected) {
        (Some(actual), Some(expected)) => (actual - expected).abs() <= 1e-9,
        (None, None) => true,
        _ => false,
    }
}

/// Validate that persisted results exactly represent the manifest's selected
/// corpus. Keep this separate from eligibility: an incomplete run is a valid
/// record, whereas a truncated or fabricated record is not comparable.
#[allow(clippy::too_many_lines)]
pub fn validate_results_contract(
    manifest: &RunManifest,
    results: &RunResults,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        errors.push(format!(
            "manifest schema_version {:?}; expected {:?}",
            manifest.schema_version, MANIFEST_SCHEMA_VERSION
        ));
    }
    if results.schema_version != RESULTS_SCHEMA_VERSION {
        errors.push(format!(
            "results schema_version {:?}; expected {:?}",
            results.schema_version, RESULTS_SCHEMA_VERSION
        ));
    }
    if results.validation_cost_complete
        && (results.validation_total_cost_usd.is_none()
            || results.validation_mean_cost_usd.is_none())
    {
        errors.push("validation cost marked complete without total and mean".into());
    }
    if !results.validation_cost_complete
        && (results.validation_total_cost_usd.is_some()
            || results.validation_mean_cost_usd.is_some())
    {
        errors.push("validation cost marked incomplete but has aggregate values".into());
    }

    let derived = derive_results_completeness(&results.cases);
    if results.all_completed != derived.all_completed {
        errors.push(format!(
            "all_completed is {}; derived {} from case and attempt completion",
            results.all_completed, derived.all_completed
        ));
    }
    if results.any_inconclusive != derived.any_inconclusive {
        errors.push(format!(
            "any_inconclusive is {}; derived {} from attempt and grader statuses",
            results.any_inconclusive, derived.any_inconclusive
        ));
    }
    if results.any_resource_incomplete != derived.any_resource_incomplete {
        errors.push(format!(
            "any_resource_incomplete is {}; derived {} from attempt resource coverage",
            results.any_resource_incomplete, derived.any_resource_incomplete
        ));
    }

    let validation_attempts: Vec<_> = results
        .cases
        .iter()
        .filter(|case| {
            manifest
                .case_policies
                .get(&case.case_id)
                .is_some_and(|policy| policy.split == "validation")
        })
        .flat_map(|case| &case.attempts)
        .collect();
    let derived_tokens: Vec<_> = validation_attempts
        .iter()
        .map(|attempt| attempt.gate_total_tokens)
        .collect();
    if results.validation_attempt_gate_tokens != derived_tokens {
        errors.push(format!(
            "validation_attempt_gate_tokens {:?}; derived {:?} from validation attempts",
            results.validation_attempt_gate_tokens, derived_tokens
        ));
    }
    let derived_mean_tokens = mean_gate_tokens(&derived_tokens);
    if !option_f64_matches(results.validation_mean_gate_tokens, derived_mean_tokens) {
        errors.push(format!(
            "validation_mean_gate_tokens {:?}; derived {:?} from validation attempts",
            results.validation_mean_gate_tokens, derived_mean_tokens
        ));
    }

    let derived_latencies: Vec<_> = validation_attempts
        .iter()
        .filter(|attempt| attempt.status == "completed")
        .map(|attempt| attempt.wall_latency_ms)
        .collect();
    if results.validation_completed_latencies_ms != derived_latencies {
        errors.push(format!(
            "validation_completed_latencies_ms {:?}; derived {:?} from completed validation attempts",
            results.validation_completed_latencies_ms, derived_latencies
        ));
    }
    let derived_median_latency = median_latency_ms(&derived_latencies);
    if !option_f64_matches(results.validation_median_latency_ms, derived_median_latency) {
        errors.push(format!(
            "validation_median_latency_ms {:?}; derived {:?} from completed validation attempts",
            results.validation_median_latency_ms, derived_median_latency
        ));
    }

    let derived_cost_complete = !validation_attempts.is_empty()
        && validation_attempts
            .iter()
            .all(|attempt| attempt.cost_complete && attempt.cost_usd.is_some());
    let derived_total_cost = derived_cost_complete.then(|| {
        validation_attempts
            .iter()
            .filter_map(|attempt| attempt.cost_usd)
            .sum::<f64>()
    });
    let derived_mean_cost =
        derived_total_cost.map(|total| total / validation_attempts.len() as f64);
    if results.validation_cost_complete != derived_cost_complete {
        errors.push(format!(
            "validation_cost_complete is {}; derived {} from validation attempts",
            results.validation_cost_complete, derived_cost_complete
        ));
    }
    if !option_f64_matches(results.validation_total_cost_usd, derived_total_cost) {
        errors.push(format!(
            "validation_total_cost_usd {:?}; derived {:?} from validation attempts",
            results.validation_total_cost_usd, derived_total_cost
        ));
    }
    if !option_f64_matches(results.validation_mean_cost_usd, derived_mean_cost) {
        errors.push(format!(
            "validation_mean_cost_usd {:?}; derived {:?} from validation attempts",
            results.validation_mean_cost_usd, derived_mean_cost
        ));
    }

    let expected_splits: BTreeSet<_> = manifest.splits.iter().collect();
    let actual_splits: BTreeSet<_> = results.splits.iter().collect();
    if expected_splits != actual_splits || results.splits.len() != actual_splits.len() {
        errors.push(format!(
            "results splits {:?}; expected selected splits {:?}",
            results.splits, manifest.splits
        ));
    }

    let expected_cases: BTreeSet<_> = manifest.case_ids.iter().collect();
    if manifest.case_ids.len() != expected_cases.len() {
        errors.push("manifest contains duplicate case_ids".into());
    }
    let manifest_policy_cases: BTreeSet<_> = manifest.case_policies.keys().collect();
    if manifest_policy_cases != expected_cases {
        errors.push("manifest case_policies do not exactly cover case_ids".into());
    }
    let expected_repetition = manifest
        .splits
        .iter()
        .filter_map(|split| match split.as_str() {
            "optimization" => Some(1),
            "validation" | "scorecard" => Some(3),
            _ => None,
        })
        .max();
    if expected_repetition != Some(manifest.repetition) {
        errors.push(format!(
            "manifest repetition {}; expected {:?} for selected splits",
            manifest.repetition, expected_repetition
        ));
    }
    let mut actual_case_counts = BTreeMap::<&str, usize>::new();
    let mut canonical_cases = Vec::new();
    for case in &results.cases {
        *actual_case_counts.entry(&case.case_id).or_default() += 1;
        if !expected_cases.contains(&case.case_id) {
            errors.push(format!("unexpected case result '{}'", case.case_id));
        }
        if !expected_splits.contains(&case.split) {
            errors.push(format!(
                "case '{}' has split {:?} outside selected splits {:?}",
                case.case_id, case.split, manifest.splits
            ));
        }
        let policy = manifest.case_policies.get(&case.case_id);
        if let Some(policy) = policy {
            if case.split != policy.split {
                errors.push(format!(
                    "case '{}' split {:?}; expected split '{}'",
                    case.case_id, case.split, policy.split
                ));
            }
            if case.must_pass != policy.must_pass {
                errors.push(format!(
                    "case '{}' must_pass {}; expected {} from manifest policy",
                    case.case_id, case.must_pass, policy.must_pass
                ));
            }
            if !case.weight.is_finite()
                || !policy.weight.is_finite()
                || (case.weight - policy.weight).abs() > 1e-9
            {
                errors.push(format!(
                    "case '{}' weight {}; expected {} from manifest policy",
                    case.case_id, case.weight, policy.weight
                ));
            }
            if case.tags != policy.tags {
                errors.push(format!(
                    "case '{}' tags {:?}; expected {:?} from manifest policy",
                    case.case_id, case.tags, policy.tags
                ));
            }
        }
        let authoritative_split =
            policy.map_or(case.split.as_str(), |policy| policy.split.as_str());
        let expected_attempts = match authoritative_split {
            "optimization" => Some(1),
            "validation" | "scorecard" => Some(3),
            _ => None,
        };
        let mut canonical_case = policy.map(|policy| {
            let mut canonical = case.clone();
            canonical.split = policy.split.clone();
            canonical.must_pass = policy.must_pass;
            canonical.weight = policy.weight;
            canonical.tags = policy.tags.clone();
            canonical
        });
        if let Some(repetitions) = expected_attempts {
            let expected: Vec<u32> = (1..=repetitions).collect();
            let actual: Vec<u32> = case
                .attempts
                .iter()
                .map(|attempt| attempt.attempt)
                .collect();
            if actual != expected {
                errors.push(format!(
                    "case '{}' expected attempts {:?}; actual {:?}",
                    case.case_id, expected, actual
                ));
            }
            let attempt_passes: Vec<_> =
                case.attempts.iter().map(|attempt| attempt.passed).collect();
            let attempt_scores: Vec<_> =
                case.attempts.iter().map(|attempt| attempt.score).collect();
            let authoritative_must_pass = manifest
                .case_policies
                .get(&case.case_id)
                .map_or(case.must_pass, |policy| policy.must_pass);
            let (derived_passed, derived_score) = aggregate_case_repetitions(
                authoritative_must_pass,
                &attempt_passes,
                &attempt_scores,
                repetitions,
            );
            if case.passed != derived_passed {
                errors.push(format!(
                    "case '{}' passed {:?}; derived {:?} from attempts",
                    case.case_id, case.passed, derived_passed
                ));
            }
            if !option_f64_matches(case.score, derived_score) {
                errors.push(format!(
                    "case '{}' score {:?}; derived {:?} from attempts",
                    case.case_id, case.score, derived_score
                ));
            }
            if let Some(canonical) = &mut canonical_case {
                canonical.passed = derived_passed;
                canonical.score = derived_score;
            }
        }
        for attempt in &case.attempts {
            if !matches!(
                attempt.status.as_str(),
                "completed" | "execution_failure" | "inconclusive"
            ) {
                errors.push(format!(
                    "case '{}' attempt {} has status '{}'",
                    case.case_id, attempt.attempt, attempt.status
                ));
            }
            if !matches!(
                attempt.grader_status.as_str(),
                "completed" | "error" | "inconclusive" | "not_run"
            ) {
                errors.push(format!(
                    "case '{}' attempt {} has grader_status '{}'",
                    case.case_id, attempt.attempt, attempt.grader_status
                ));
            }
            if !matches!(
                attempt.resource_coverage.as_str(),
                "complete" | "incomplete"
            ) {
                errors.push(format!(
                    "case '{}' attempt {} has resource_coverage '{}'",
                    case.case_id, attempt.attempt, attempt.resource_coverage
                ));
            }
            if attempt.cost_complete && attempt.cost_usd.is_none() {
                errors.push(format!(
                    "case '{}' attempt {} marks cost complete without cost_usd",
                    case.case_id, attempt.attempt
                ));
            }
        }
        if let Some(canonical) = canonical_case {
            canonical_cases.push(canonical);
        }
    }
    for case_id in expected_cases {
        match actual_case_counts.get(case_id.as_str()) {
            None => errors.push(format!("missing case result '{case_id}'")),
            Some(count) if *count > 1 => {
                errors.push(format!("duplicate case result '{case_id}' ({count} rows)"));
            }
            _ => {}
        }
    }

    let selected_splits: Vec<_> = manifest
        .splits
        .iter()
        .filter_map(|split| match split.as_str() {
            "optimization" => Some(super::case::EvalSplit::Optimization),
            "validation" => Some(super::case::EvalSplit::Validation),
            "scorecard" => Some(super::case::EvalSplit::Scorecard),
            _ => None,
        })
        .collect();
    let (derived_overall, derived_per_tag) =
        aggregate_split_scores(&canonical_cases, &selected_splits);
    if !option_f64_matches(results.overall, derived_overall) {
        errors.push(format!(
            "overall {:?}; derived {:?} from case aggregates",
            results.overall, derived_overall
        ));
    }
    let tag_names: BTreeSet<_> = results
        .per_tag
        .keys()
        .chain(derived_per_tag.keys())
        .cloned()
        .collect();
    for tag in tag_names {
        let actual = results.per_tag.get(&tag).copied();
        let expected = derived_per_tag.get(&tag).copied();
        if !option_f64_matches(actual, expected) {
            errors.push(format!(
                "per_tag.{tag} {actual:?}; derived {expected:?} from case aggregates"
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Load a completed run directory from disk.
pub fn load_run(runs_root: &Path, label: &str) -> Result<LoadedRun, String> {
    let dir = runs_root.join(label);
    if !dir.is_dir() {
        return Err(format!(
            "run label '{label}' not found at {}",
            dir.display()
        ));
    }
    let manifest: RunManifest = read_json(&dir.join("manifest.json"))?;
    let results: RunResults = read_json(&dir.join("results.json"))?;
    validate_results_contract(&manifest, &results).map_err(|errors| {
        format!(
            "results contract invalid for '{label}': {}",
            errors.join("; ")
        )
    })?;

    let eff_path = dir.join(&manifest.effective_config.path);
    let eff_bytes = fs::read(&eff_path)
        .map_err(|e| format!("reading effective-config {}: {e}", eff_path.display()))?;
    let recomputed = hex_sha256(&eff_bytes);
    let claimed = manifest.effective_config.sha256.clone();
    if recomputed != claimed {
        return Err(format!(
            "effective-config hash mismatch for '{label}': manifest claims {claimed}, disk recomputes {recomputed}"
        ));
    }
    // Also verify hash file if present.
    let hash_file = dir.join("effective-config/snapshot.sha256");
    if hash_file.is_file() {
        let hf = fs::read_to_string(&hash_file)
            .map_err(|e| format!("reading {}: {e}", hash_file.display()))?;
        let hf = hf.trim();
        if hf != recomputed {
            return Err(format!(
                "effective-config/snapshot.sha256 disagrees with file bytes for '{label}'"
            ));
        }
    }

    let harness_path = dir.join(&manifest.harness.path);
    let harness_bytes = fs::read(&harness_path)
        .map_err(|e| format!("reading harness {}: {e}", harness_path.display()))?;
    let harness_hash = hex_sha256(&harness_bytes);
    if harness_hash != manifest.harness.sha256 {
        return Err(format!(
            "harness snapshot hash mismatch for '{label}': manifest {}, disk {}",
            manifest.harness.sha256, harness_hash
        ));
    }

    Ok(LoadedRun {
        label: label.to_string(),
        dir,
        manifest,
        results,
        effective_config_bytes: eff_bytes,
        effective_config_hash: recomputed,
        harness_hash,
    })
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("parsing {}: {e}", path.display()))
}

/// Compare baseline and candidate runs. Does not mutate either directory.
#[allow(clippy::too_many_lines)]
pub fn compare_runs(baseline: &LoadedRun, candidate: &LoadedRun) -> CompareReport {
    let mut mismatches = comparability_mismatches(baseline, candidate);
    for (side, run) in [("baseline", baseline), ("candidate", candidate)] {
        if let Err(errors) = validate_results_contract(&run.manifest, &run.results) {
            mismatches.extend(
                errors
                    .into_iter()
                    .map(|error| format!("{side} results contract: {error}")),
            );
        }
    }
    let mut notes = Vec::new();
    let mut effective_config_diff = None;
    let baseline_cost = CostSummary::from_results(&baseline.results);
    let candidate_cost = CostSummary::from_results(&candidate.results);

    if baseline.effective_config_hash != candidate.effective_config_hash {
        if !mismatches.iter().any(|m| m.contains("effective_config")) {
            mismatches.push(format!(
                "effective_config.sha256: baseline={} candidate={}",
                baseline.effective_config_hash, candidate.effective_config_hash
            ));
        }
        effective_config_diff = Some(field_level_diff(
            &baseline.effective_config_bytes,
            &candidate.effective_config_bytes,
        ));
    }

    // Harness is allowed to differ — note only.
    if baseline.harness_hash != candidate.harness_hash {
        notes.push(format!(
            "harness snapshot differs (expected for candidates): baseline={} candidate={}",
            baseline.harness_hash, candidate.harness_hash
        ));
    }

    if !mismatches.is_empty() {
        return CompareReport {
            schema_version: COMPARE_SCHEMA_VERSION.into(),
            status: CompareStatus::Incomparable,
            baseline_label: baseline.label.clone(),
            candidate_label: candidate.label.clone(),
            baseline_cost,
            candidate_cost,
            gates: vec![],
            mismatches,
            baseline_must_pass_failures: vec![],
            effective_config_diff,
            notes,
        };
    }

    // Baseline must-pass absolute validity.
    let baseline_mp_failures = must_pass_failures(&baseline.results);
    if !baseline_mp_failures.is_empty() {
        return CompareReport {
            schema_version: COMPARE_SCHEMA_VERSION.into(),
            status: CompareStatus::InvalidBaseline,
            baseline_label: baseline.label.clone(),
            candidate_label: candidate.label.clone(),
            baseline_cost,
            candidate_cost,
            gates: vec![GateResult {
                gate_id: "baseline_must_pass".into(),
                passed: false,
                actual: json!(baseline_mp_failures),
                threshold: json!("all must-pass attempts pass"),
                detail:
                    "baseline has failing must-pass attempts; candidate eligibility not computed"
                        .into(),
            }],
            mismatches: vec![],
            baseline_must_pass_failures: baseline_mp_failures,
            effective_config_diff: None,
            notes,
        };
    }

    let mut gates = Vec::new();

    // Candidate must-pass absolute.
    let cand_mp = must_pass_failures(&candidate.results);
    gates.push(GateResult {
        gate_id: "candidate_must_pass".into(),
        passed: cand_mp.is_empty(),
        actual: json!(cand_mp),
        threshold: json!("all must-pass attempts pass"),
        detail: if cand_mp.is_empty() {
            "all candidate must-pass attempts passed".into()
        } else {
            format!("{} must-pass failure(s)", cand_mp.len())
        },
    });

    // Overall +5 on validation weighted score.
    let base_overall = baseline.results.overall;
    let cand_overall = candidate.results.overall;
    let overall_delta = match (base_overall, cand_overall) {
        (Some(b), Some(c)) => Some(c - b),
        _ => None,
    };
    let overall_ok = overall_delta.map(|d| d >= 5.0 - 1e-9).unwrap_or(false);
    gates.push(GateResult {
        gate_id: "overall_plus_5".into(),
        passed: overall_ok,
        actual: json!({
            "baseline": base_overall,
            "candidate": cand_overall,
            "delta": overall_delta,
        }),
        threshold: json!({ "min_delta": 5.0 }),
        detail: match overall_delta {
            Some(d) => format!("validation weighted score delta = {d:.3}"),
            None => "overall score null (incomplete aggregates)".into(),
        },
    });

    // Per-tag non-regression.
    let mut tag_drops = Vec::new();
    for tag in GATING_TAGS {
        let key = tag.as_str().to_string();
        let b = baseline.results.per_tag.get(&key).copied();
        let c = candidate.results.per_tag.get(&key).copied();
        match (b, c) {
            (Some(bv), Some(cv)) if cv + 1e-9 < bv => {
                tag_drops.push(format!("{key}: baseline={bv:.3} candidate={cv:.3}"));
            }
            (Some(_), None) => {
                tag_drops.push(format!("{key}: candidate missing tag score"));
            }
            (None, _) => {
                tag_drops.push(format!("{key}: baseline missing tag score"));
            }
            _ => {}
        }
    }
    gates.push(GateResult {
        gate_id: "per_tag_no_drop".into(),
        passed: tag_drops.is_empty(),
        actual: json!(tag_drops),
        threshold: json!("no validation tag score decreases"),
        detail: if tag_drops.is_empty() {
            "no tag regressions".into()
        } else {
            format!("{} tag drop(s)", tag_drops.len())
        },
    });

    // Tokens ≤ 115%.
    let base_tokens = baseline
        .results
        .validation_mean_gate_tokens
        .or_else(|| mean_gate_tokens(&baseline.results.validation_attempt_gate_tokens));
    let cand_tokens = candidate
        .results
        .validation_mean_gate_tokens
        .or_else(|| mean_gate_tokens(&candidate.results.validation_attempt_gate_tokens));
    let token_ratio = match (base_tokens, cand_tokens) {
        (Some(b), Some(c)) if b > 0.0 => Some(c / b),
        (Some(0.0), Some(0.0)) => Some(1.0),
        (Some(0.0), Some(c)) if c > 0.0 => Some(f64::INFINITY),
        _ => None,
    };
    let tokens_ok = token_ratio.map(|r| r <= 1.15 + 1e-12).unwrap_or(false);
    gates.push(GateResult {
        gate_id: "tokens_le_115pct".into(),
        passed: tokens_ok,
        actual: json!({
            "baseline_mean": base_tokens,
            "candidate_mean": cand_tokens,
            "ratio": token_ratio,
        }),
        threshold: json!({ "max_ratio": 1.15 }),
        detail: match token_ratio {
            Some(r) => format!("token ratio = {r:.4}"),
            None => "token means unavailable".into(),
        },
    });

    // Latency ≤ 130% (median of completed attempts only).
    let base_lat = baseline
        .results
        .validation_median_latency_ms
        .or_else(|| median_latency_ms(&baseline.results.validation_completed_latencies_ms));
    let cand_lat = candidate
        .results
        .validation_median_latency_ms
        .or_else(|| median_latency_ms(&candidate.results.validation_completed_latencies_ms));
    let lat_ratio = match (base_lat, cand_lat) {
        (Some(b), Some(c)) if b > 0.0 => Some(c / b),
        (Some(0.0), Some(0.0)) => Some(1.0),
        (Some(0.0), Some(c)) if c > 0.0 => Some(f64::INFINITY),
        _ => None,
    };
    let lat_ok = lat_ratio.map(|r| r <= 1.30 + 1e-12).unwrap_or(false);
    gates.push(GateResult {
        gate_id: "latency_le_130pct".into(),
        passed: lat_ok,
        actual: json!({
            "baseline_median_ms": base_lat,
            "candidate_median_ms": cand_lat,
            "ratio": lat_ratio,
        }),
        threshold: json!({ "max_ratio": 1.30 }),
        detail: match lat_ratio {
            Some(r) => format!("latency ratio = {r:.4}"),
            None => "latency medians unavailable".into(),
        },
    });

    // Completeness: no inconclusive, all completed, resource complete.
    let completeness_ok = candidate.results.all_completed
        && !candidate.results.any_inconclusive
        && !candidate.results.any_resource_incomplete
        && baseline.results.all_completed
        && !baseline.results.any_inconclusive
        && !baseline.results.any_resource_incomplete;
    gates.push(GateResult {
        gate_id: "no_inconclusive_or_incomplete".into(),
        passed: completeness_ok,
        actual: json!({
            "baseline_all_completed": baseline.results.all_completed,
            "baseline_any_inconclusive": baseline.results.any_inconclusive,
            "baseline_any_resource_incomplete": baseline.results.any_resource_incomplete,
            "candidate_all_completed": candidate.results.all_completed,
            "candidate_any_inconclusive": candidate.results.any_inconclusive,
            "candidate_any_resource_incomplete": candidate.results.any_resource_incomplete,
        }),
        threshold: json!("all cases completed, graded, resource complete"),
        detail: if completeness_ok {
            "all attempts complete with resource coverage".into()
        } else {
            "incomplete, inconclusive, or missing resource coverage present".into()
        },
    });

    // Effective config hash equal (already enforced by comparability, restate).
    gates.push(GateResult {
        gate_id: "effective_config_equal".into(),
        passed: true,
        actual: json!(baseline.effective_config_hash),
        threshold: json!(candidate.effective_config_hash),
        detail: "effective-config hashes match (recomputed from disk)".into(),
    });

    let all_pass = gates.iter().all(|g| g.passed);
    let status = if all_pass {
        CompareStatus::EligibleForReview
    } else {
        CompareStatus::NotEligible
    };

    notes.push(
        "Compare never auto-edits harness or deletes run directories; human review decides acceptance."
            .into(),
    );

    CompareReport {
        schema_version: COMPARE_SCHEMA_VERSION.into(),
        status,
        baseline_label: baseline.label.clone(),
        candidate_label: candidate.label.clone(),
        baseline_cost,
        candidate_cost,
        gates,
        mismatches: vec![],
        baseline_must_pass_failures: vec![],
        effective_config_diff: None,
        notes,
    }
}

fn must_pass_failures(results: &RunResults) -> Vec<String> {
    let mut out = Vec::new();
    for case in &results.cases {
        if !case.must_pass {
            continue;
        }
        for att in &case.attempts {
            let pass = att.passed.unwrap_or(false);
            let completed = att.status == "completed" && att.grader_status == "completed";
            if !completed || !pass {
                out.push(format!(
                    "{} attempt {} status={} grader={} passed={:?}",
                    case.case_id, att.attempt, att.status, att.grader_status, att.passed
                ));
            }
        }
        // Also require case-level pass when present.
        if case.passed != Some(true) {
            // Avoid duplicate if already listed via attempts.
            if out.iter().all(|s| !s.starts_with(&case.case_id)) {
                out.push(format!("{} case_passed={:?}", case.case_id, case.passed));
            }
        }
    }
    out
}

fn comparability_mismatches(baseline: &LoadedRun, candidate: &LoadedRun) -> Vec<String> {
    let mut m = Vec::new();
    let bm = &baseline.manifest;
    let cm = &candidate.manifest;

    if bm.git.commit != cm.git.commit {
        m.push(format!(
            "git.commit: baseline={:?} candidate={:?}",
            bm.git.commit, cm.git.commit
        ));
    }
    // Both must have no non-harness dirty paths — if either side recorded unexpected
    // dirty, treat as incomparable experiment.
    if bm.git.dirty && !only_harness_dirty(&bm.git.dirty_paths) {
        m.push(format!(
            "baseline has non-harness dirty paths: {}",
            bm.git.dirty_paths.join(", ")
        ));
    }
    if cm.git.dirty && !only_harness_dirty(&cm.git.dirty_paths) {
        m.push(format!(
            "candidate has non-harness dirty paths: {}",
            cm.git.dirty_paths.join(", ")
        ));
    }
    if bm.fixture_revision != cm.fixture_revision {
        m.push(format!(
            "fixture_revision: baseline={} candidate={}",
            bm.fixture_revision, cm.fixture_revision
        ));
    }
    if bm.provider != cm.provider || bm.model != cm.model {
        m.push(format!(
            "model identity: baseline={:?}/{:?} candidate={:?}/{:?}",
            bm.provider, bm.model, cm.provider, cm.model
        ));
    }
    if bm.session_seeds != cm.session_seeds {
        m.push("session_seeds hashes differ".into());
    }
    if bm.case_policies != cm.case_policies {
        m.push("case_policies differ".into());
    }
    let b_cases: BTreeSet<_> = bm.case_ids.iter().collect();
    let c_cases: BTreeSet<_> = cm.case_ids.iter().collect();
    if b_cases != c_cases {
        m.push(format!(
            "case_ids differ: only_baseline={:?} only_candidate={:?}",
            b_cases.difference(&c_cases).collect::<Vec<_>>(),
            c_cases.difference(&b_cases).collect::<Vec<_>>()
        ));
    }
    let b_splits: BTreeSet<_> = bm.splits.iter().collect();
    let c_splits: BTreeSet<_> = cm.splits.iter().collect();
    if b_splits != c_splits {
        m.push(format!(
            "splits: baseline={:?} candidate={:?}",
            bm.splits, cm.splits
        ));
    }
    if bm.repetition != cm.repetition {
        m.push(format!(
            "repetition policy: baseline={} candidate={}",
            bm.repetition, cm.repetition
        ));
    }
    if baseline.effective_config_hash != candidate.effective_config_hash {
        m.push(format!(
            "effective_config.sha256: baseline={} candidate={}",
            baseline.effective_config_hash, candidate.effective_config_hash
        ));
    }
    m
}

fn only_harness_dirty(paths: &[String]) -> bool {
    paths
        .iter()
        .all(|p| p.replace('\\', "/").ends_with("src/harness.rs"))
}

fn field_level_diff(a: &[u8], b: &[u8]) -> Value {
    let av: Value = serde_json::from_slice(a).unwrap_or(Value::Null);
    let bv: Value = serde_json::from_slice(b).unwrap_or(Value::Null);
    json!({
        "baseline": av,
        "candidate": bv,
        "note": "full snapshots shown; hashes differ"
    })
}

/// Write compare report next to runs (or cwd).
pub fn write_compare_report(
    out_dir: &Path,
    report: &CompareReport,
) -> Result<(PathBuf, PathBuf), String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("creating {}: {e}", out_dir.display()))?;
    let json_path = out_dir.join(format!(
        "compare-{}-{}.json",
        report.baseline_label, report.candidate_label
    ));
    let md_path = out_dir.join(format!(
        "compare-{}-{}.md",
        report.baseline_label, report.candidate_label
    ));
    let mut bytes = serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?;
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    fs::write(&json_path, bytes).map_err(|e| format!("writing {}: {e}", json_path.display()))?;
    fs::write(&md_path, report.to_markdown())
        .map_err(|e| format!("writing {}: {e}", md_path.display()))?;
    Ok((json_path, md_path))
}

/// Helper used by unit tests to hash arbitrary bytes the same way as artifacts.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::artifact::{
        build_manifest_skeleton, create_run_dir, write_effective_config_snapshot,
        write_harness_snapshot, write_manifest, GitInfo, HarnessSnapshot, ManifestCasePolicy,
        SnapshotRef,
    };
    use crate::eval::effective_config::{sample_input, EffectiveConfigSnapshot};
    use std::collections::BTreeMap;

    fn sample_manifest(label: &str, eff: SnapshotRef, harness: SnapshotRef) -> RunManifest {
        let mut m = build_manifest_skeleton(
            label,
            GitInfo {
                commit: Some("abc".into()),
                dirty: false,
                dirty_paths: vec![],
            },
            "fix1".into(),
            Some("mock".into()),
            Some("scripted".into()),
            json!({}),
            harness,
            BTreeMap::new(),
            eff,
            "1".into(),
            BTreeMap::new(),
            vec!["val-a".into()],
            vec!["validation".into()],
            3,
        );
        m.label = label.into();
        m.case_policies.insert(
            "val-a".into(),
            ManifestCasePolicy {
                split: "validation".into(),
                must_pass: true,
                weight: 1.0,
                tags: GATING_TAGS
                    .iter()
                    .map(|tag| tag.as_str().to_string())
                    .collect(),
            },
        );
        m
    }

    fn make_results(pass_all: bool, overall: f64, tokens: u64, latency: u64) -> RunResults {
        let mut per_tag = BTreeMap::new();
        for t in GATING_TAGS {
            per_tag.insert(t.as_str().to_string(), overall);
        }
        RunResults {
            schema_version: RESULTS_SCHEMA_VERSION.into(),
            label: "x".into(),
            splits: vec!["validation".into()],
            cases: vec![CaseResultRow {
                case_id: "val-a".into(),
                split: "validation".into(),
                must_pass: true,
                weight: 1.0,
                tags: GATING_TAGS
                    .iter()
                    .map(|tag| tag.as_str().to_string())
                    .collect(),
                passed: Some(pass_all),
                score: Some(overall),
                attempts: (1..=3)
                    .map(|i| AttemptResultRow {
                        attempt: i,
                        status: "completed".into(),
                        passed: Some(pass_all),
                        score: Some(overall),
                        wall_latency_ms: latency,
                        gate_total_tokens: tokens,
                        resource_coverage: "complete".into(),
                        cost_usd: None,
                        cost_complete: false,
                        grader_status: "completed".into(),
                    })
                    .collect(),
            }],
            overall: Some(overall),
            per_tag,
            validation_mean_gate_tokens: Some(tokens as f64),
            validation_median_latency_ms: Some(latency as f64),
            validation_attempt_gate_tokens: vec![tokens, tokens, tokens],
            validation_completed_latencies_ms: vec![latency, latency, latency],
            validation_total_cost_usd: None,
            validation_mean_cost_usd: None,
            validation_cost_complete: false,
            any_inconclusive: false,
            any_resource_incomplete: false,
            all_completed: true,
        }
    }

    #[test]
    fn results_contract_accumulates_cardinality_and_coverage_mismatches() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "candidate").unwrap();
        let harness = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let eff = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let eref =
            write_effective_config_snapshot(&run_dir, &eff.normalize_bytes().unwrap()).unwrap();
        let manifest = sample_manifest("candidate", eref, href);
        let mut results = make_results(true, 80.0, 1_000, 100);
        results.cases[0].attempts.pop();
        results.cases[0].attempts[0].attempt = 2;
        results.cases[0].attempts[0].grader_status = "unknown".into();
        results.cases[0].attempts[1].resource_coverage = "missing".into();
        results.cases.push(results.cases[0].clone());
        results.cases.push(CaseResultRow {
            case_id: "unexpected".into(),
            split: "validation".into(),
            must_pass: false,
            weight: 1.0,
            tags: vec![],
            passed: Some(true),
            score: Some(80.0),
            attempts: vec![],
        });

        let errors = validate_results_contract(&manifest, &results).unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.contains("duplicate case result 'val-a'")));
        assert!(errors
            .iter()
            .any(|error| error.contains("unexpected case result 'unexpected'")));
        assert!(errors
            .iter()
            .any(|error| error.contains("expected attempts [1, 2, 3]")));
        assert!(errors
            .iter()
            .any(|error| error.contains("grader_status 'unknown'")));
        assert!(errors
            .iter()
            .any(|error| error.contains("resource_coverage 'missing'")));
    }

    #[test]
    fn results_contract_rejects_a_case_assigned_to_the_wrong_selected_split() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "candidate").unwrap();
        let harness = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let eff = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let eref =
            write_effective_config_snapshot(&run_dir, &eff.normalize_bytes().unwrap()).unwrap();
        let mut manifest = sample_manifest("candidate", eref, href);
        manifest.splits.push("scorecard".into());
        manifest.case_policies.get_mut("val-a").unwrap().split = "validation".into();
        let mut results = make_results(true, 80.0, 1_000, 100);
        results.splits.push("scorecard".into());
        results.cases[0].split = "scorecard".into();

        let errors = validate_results_contract(&manifest, &results).unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.contains("expected split 'validation'")));
    }

    #[test]
    fn results_contract_rejects_green_summaries_over_a_non_must_pass_grader_error() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "candidate").unwrap();
        let harness = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let eff = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let eref =
            write_effective_config_snapshot(&run_dir, &eff.normalize_bytes().unwrap()).unwrap();
        let manifest = sample_manifest("candidate", eref, href);
        let mut results = make_results(true, 80.0, 1_000, 100);
        results.cases[0].must_pass = false;
        results.cases[0].attempts[0].grader_status = "error".into();
        results.cases[0].attempts[0].passed = None;
        results.cases[0].attempts[0].score = None;

        let errors = validate_results_contract(&manifest, &results).unwrap_err();
        assert!(errors
            .iter()
            .any(|error| { error.contains("all_completed") && error.contains("derived false") }));
    }

    #[test]
    fn results_contract_rejects_case_score_and_pass_not_supported_by_attempts() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "candidate").unwrap();
        let harness = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let eff = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let eref =
            write_effective_config_snapshot(&run_dir, &eff.normalize_bytes().unwrap()).unwrap();
        let manifest = sample_manifest("candidate", eref, href);
        let mut results = make_results(false, 20.0, 1_000, 100);
        results.cases[0].passed = Some(true);
        results.cases[0].score = Some(100.0);

        let errors = validate_results_contract(&manifest, &results).unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.contains("case 'val-a' passed")));
        assert!(errors
            .iter()
            .any(|error| error.contains("case 'val-a' score")));
    }

    #[test]
    fn results_contract_rejects_overall_and_tag_scores_not_supported_by_cases() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "candidate").unwrap();
        let harness = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let eff = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let eref =
            write_effective_config_snapshot(&run_dir, &eff.normalize_bytes().unwrap()).unwrap();
        let manifest = sample_manifest("candidate", eref, href);
        let mut results = make_results(true, 20.0, 1_000, 100);
        results.overall = Some(100.0);
        results.per_tag.insert("tool_selection".into(), 100.0);

        let errors = validate_results_contract(&manifest, &results).unwrap_err();
        assert!(errors.iter().any(|error| error.contains("overall")));
        assert!(errors
            .iter()
            .any(|error| error.contains("per_tag.tool_selection")));
    }

    #[test]
    fn results_contract_rejects_case_policy_copied_from_mutable_result_rows() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "candidate").unwrap();
        let harness = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let eff = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let eref =
            write_effective_config_snapshot(&run_dir, &eff.normalize_bytes().unwrap()).unwrap();
        let manifest = sample_manifest("candidate", eref, href);
        let mut results = make_results(true, 80.0, 1_000, 100);
        results.cases[0].must_pass = false;
        results.cases[0].weight = 99.0;
        results.cases[0].tags = vec!["citation_quality".into()];

        let errors = validate_results_contract(&manifest, &results).unwrap_err();
        for field in ["must_pass", "weight", "tags"] {
            assert!(
                errors.iter().any(|error| error.contains(field)),
                "missing {field} mismatch in {errors:?}"
            );
        }
    }

    #[test]
    fn tampered_score_summaries_never_reach_eligibility_gates() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1_000, 100, true);
        let mut cand = write_run(root.path(), "candidate", 70.0, 1_000, 100, true);
        cand.results.cases[0].passed = Some(true);
        cand.results.cases[0].score = Some(100.0);
        cand.results.overall = Some(100.0);
        cand.results.per_tag.insert("tool_selection".into(), 100.0);

        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::Incomparable);
        assert!(report.gates.is_empty());
        for field in ["case 'val-a' score", "overall", "per_tag.tool_selection"] {
            assert!(
                report
                    .mismatches
                    .iter()
                    .any(|mismatch| mismatch.contains(field)),
                "missing {field} mismatch in {:?}",
                report.mismatches
            );
        }
    }

    #[test]
    fn results_contract_accumulates_stale_token_latency_and_cost_summaries() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "candidate").unwrap();
        let harness = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let eff = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let eref =
            write_effective_config_snapshot(&run_dir, &eff.normalize_bytes().unwrap()).unwrap();
        let manifest = sample_manifest("candidate", eref, href);
        let mut results = make_results(true, 80.0, 1_000, 100);
        for attempt in &mut results.cases[0].attempts {
            attempt.cost_complete = true;
            attempt.cost_usd = Some(0.25);
        }
        results.validation_attempt_gate_tokens = vec![1];
        results.validation_mean_gate_tokens = Some(1.0);
        results.validation_completed_latencies_ms = vec![1];
        results.validation_median_latency_ms = Some(1.0);

        let errors = validate_results_contract(&manifest, &results).unwrap_err();
        for expected in [
            "validation_attempt_gate_tokens",
            "validation_mean_gate_tokens",
            "validation_completed_latencies_ms",
            "validation_median_latency_ms",
            "validation_cost_complete",
            "validation_total_cost_usd",
            "validation_mean_cost_usd",
        ] {
            assert!(
                errors.iter().any(|error| error.contains(expected)),
                "missing {expected} mismatch in {errors:?}"
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_run(
        root: &Path,
        label: &str,
        overall: f64,
        tokens: u64,
        latency: u64,
        pass: bool,
    ) -> LoadedRun {
        let run_dir = create_run_dir(root, label).unwrap();
        let harness = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let eff = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let bytes = eff.normalize_bytes().unwrap();
        let eref = write_effective_config_snapshot(&run_dir, &bytes).unwrap();
        let manifest = sample_manifest(label, eref, href);
        write_manifest(&run_dir, &manifest).unwrap();
        let results = make_results(pass, overall, tokens, latency);
        let mut rbytes = serde_json::to_vec_pretty(&results).unwrap();
        rbytes.push(b'\n');
        fs::write(run_dir.join("results.json"), rbytes).unwrap();
        load_run(root, label).unwrap()
    }

    #[test]
    fn eligible_when_all_gates_pass() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        let cand = write_run(root.path(), "candidate", 80.0, 1100, 120, true);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::EligibleForReview);
        assert_eq!(
            gate_ids(&report),
            vec![
                "candidate_must_pass",
                "overall_plus_5",
                "per_tag_no_drop",
                "tokens_le_115pct",
                "latency_le_130pct",
                "no_inconclusive_or_incomplete",
                "effective_config_equal",
            ]
        );
        assert!(failed_gate_ids(&report).is_empty());
    }

    #[test]
    fn compare_report_renders_unknown_cost_without_affecting_eligibility() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1_000, 100, true);
        let cand = write_run(root.path(), "candidate", 80.0, 1_100, 120, true);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::EligibleForReview);
        assert!(!report.baseline_cost.complete);
        assert_eq!(report.baseline_cost.total_usd, None);
        assert!(report.to_markdown().contains("`unknown`"));
        let json = report.to_json_value();
        assert_eq!(json["baseline_cost"]["total_usd"], "unknown");
        assert_eq!(json["baseline_cost"]["mean_usd"], "unknown");
        let results_json = serde_json::to_value(&base.results).unwrap();
        assert_eq!(results_json["validation_total_cost_usd"], "unknown");
        assert_eq!(results_json["validation_mean_cost_usd"], "unknown");
    }

    #[test]
    fn compare_report_renders_known_cost_as_numbers_and_never_as_a_gate() {
        let root = tempfile::tempdir().unwrap();
        let mut base = write_run(root.path(), "baseline", 70.0, 1_000, 100, true);
        let mut cand = write_run(root.path(), "candidate", 80.0, 1_100, 120, true);
        set_complete_cost(&mut base.results, 0.10);
        set_complete_cost(&mut cand.results, 0.20);

        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::EligibleForReview);
        assert!(report
            .gates
            .iter()
            .all(|gate| !gate.gate_id.contains("cost")));
        let json = report.to_json_value();
        assert!((json["baseline_cost"]["total_usd"].as_f64().unwrap() - 0.3).abs() < 1e-9);
        assert!((json["baseline_cost"]["mean_usd"].as_f64().unwrap() - 0.1).abs() < 1e-9);
        assert!((json["candidate_cost"]["total_usd"].as_f64().unwrap() - 0.6).abs() < 1e-9);
        assert!(report
            .to_markdown()
            .contains("baseline validation cost**: `total=$0.3000, mean=$0.1000`"));
    }

    fn set_complete_cost(results: &mut RunResults, per_attempt: f64) {
        for case in &mut results.cases {
            for attempt in &mut case.attempts {
                attempt.cost_complete = true;
                attempt.cost_usd = Some(per_attempt);
            }
        }
        let attempt_count = results.cases.iter().flat_map(|case| &case.attempts).count();
        results.validation_cost_complete = true;
        results.validation_total_cost_usd = Some(per_attempt * attempt_count as f64);
        results.validation_mean_cost_usd = Some(per_attempt);
    }

    #[test]
    fn invalid_baseline_skips_eligibility() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, false);
        let cand = write_run(root.path(), "candidate", 90.0, 1000, 100, true);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::InvalidBaseline);
        assert!(!report.baseline_must_pass_failures.is_empty());
        assert!(!gate(&report, "baseline_must_pass").passed);
    }

    #[test]
    fn shared_must_pass_failure_is_an_invalid_baseline_before_candidate_gates() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1_000, 100, false);
        let cand = write_run(root.path(), "candidate", 80.0, 1_000, 100, false);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::InvalidBaseline);
        assert_eq!(gate_ids(&report), vec!["baseline_must_pass"]);
        assert_eq!(failed_gate_ids(&report), vec!["baseline_must_pass"]);
    }

    #[test]
    fn candidate_must_pass_failure_has_its_own_gate() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1_000, 100, true);
        let cand = write_run(root.path(), "candidate", 80.0, 1_000, 100, false);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::NotEligible);
        assert_eq!(failed_gate_ids(&report), vec!["candidate_must_pass"]);
    }

    #[test]
    fn inconclusive_attempt_fails_completeness_gate() {
        let root = tempfile::tempdir().unwrap();
        let mut base = write_run(root.path(), "baseline", 70.0, 1_000, 100, true);
        let mut cand = write_run(root.path(), "candidate", 80.0, 1_000, 100, true);
        base.manifest
            .case_policies
            .get_mut("val-a")
            .unwrap()
            .must_pass = false;
        base.results.cases[0].must_pass = false;
        cand.manifest
            .case_policies
            .get_mut("val-a")
            .unwrap()
            .must_pass = false;
        cand.results.cases[0].must_pass = false;
        cand.results.cases[0].attempts[0].status = "inconclusive".into();
        cand.results.cases[0].attempts[0].grader_status = "inconclusive".into();
        cand.results.cases[0].attempts[0].passed = None;
        cand.results.cases[0].attempts[0].score = None;
        refresh_derived_results(&mut base.results);
        refresh_derived_results(&mut cand.results);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::NotEligible);
        assert!(!gate(&report, "no_inconclusive_or_incomplete").passed);
    }

    #[test]
    fn non_must_pass_grader_error_fails_completeness_gate() {
        let root = tempfile::tempdir().unwrap();
        let mut base = write_run(root.path(), "baseline", 70.0, 1_000, 100, true);
        let mut cand = write_run(root.path(), "candidate", 80.0, 1_000, 100, true);
        base.manifest
            .case_policies
            .get_mut("val-a")
            .unwrap()
            .must_pass = false;
        base.results.cases[0].must_pass = false;
        cand.manifest
            .case_policies
            .get_mut("val-a")
            .unwrap()
            .must_pass = false;
        cand.results.cases[0].must_pass = false;
        cand.results.cases[0].attempts[0].grader_status = "error".into();
        cand.results.cases[0].attempts[0].passed = None;
        cand.results.cases[0].attempts[0].score = None;
        refresh_derived_results(&mut base.results);
        refresh_derived_results(&mut cand.results);

        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::NotEligible);
        assert!(!gate(&report, "no_inconclusive_or_incomplete").passed);
    }

    #[test]
    fn incomplete_resource_fails_completeness_gate() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1_000, 100, true);
        let mut cand = write_run(root.path(), "candidate", 80.0, 1_000, 100, true);
        cand.results.any_resource_incomplete = true;
        cand.results.cases[0].attempts[0].resource_coverage = "incomplete".into();
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::NotEligible);
        assert_eq!(
            failed_gate_ids(&report),
            vec!["no_inconclusive_or_incomplete"]
        );
    }

    #[test]
    fn missing_result_row_is_incomparable_before_gates() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1_000, 100, true);
        let mut cand = write_run(root.path(), "candidate", 80.0, 1_000, 100, true);
        cand.results.cases.clear();
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::Incomparable);
        assert!(report.gates.is_empty());
        assert!(report
            .mismatches
            .iter()
            .any(|mismatch| mismatch.contains("missing case result 'val-a'")));
    }

    #[test]
    fn missing_case_is_rejected_by_load_and_compare() {
        assert_contract_mutation_rejected("missing case result 'val-a'", |results| {
            results.cases.clear();
        });
    }

    #[test]
    fn duplicate_case_is_rejected_by_load_and_compare() {
        assert_contract_mutation_rejected("duplicate case result 'val-a'", |results| {
            results.cases.push(results.cases[0].clone());
        });
    }

    #[test]
    fn extra_case_is_rejected_by_load_and_compare() {
        assert_contract_mutation_rejected("unexpected case result 'extra'", |results| {
            let mut extra = results.cases[0].clone();
            extra.case_id = "extra".into();
            results.cases.push(extra);
        });
    }

    #[test]
    fn wrong_repetition_is_rejected_by_load_and_compare() {
        assert_contract_mutation_rejected("expected attempts [1, 2, 3]", |results| {
            results.cases[0].attempts.pop();
        });
    }

    fn assert_contract_mutation_rejected(
        expected_mismatch: &str,
        mutate: impl Fn(&mut RunResults),
    ) {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1_000, 100, true);
        let mut candidate = write_run(root.path(), "candidate", 80.0, 1_000, 100, true);
        mutate(&mut candidate.results);

        let report = compare_runs(&base, &candidate);
        assert_eq!(report.status, CompareStatus::Incomparable);
        assert!(report.gates.is_empty());
        assert!(
            report
                .mismatches
                .iter()
                .any(|mismatch| mismatch.contains(expected_mismatch)),
            "missing {expected_mismatch:?} in {:?}",
            report.mismatches
        );

        fs::write(
            candidate.dir.join("results.json"),
            serde_json::to_vec_pretty(&candidate.results).unwrap(),
        )
        .unwrap();
        let load_error = load_run(root.path(), "candidate").unwrap_err();
        assert!(
            load_error.contains(expected_mismatch),
            "missing {expected_mismatch:?} in {load_error}"
        );
    }

    #[test]
    fn overall_insufficient_not_eligible() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        let cand = write_run(root.path(), "candidate", 74.0, 1000, 100, true);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::NotEligible);
        assert_eq!(failed_gate_ids(&report), vec!["overall_plus_5"]);
    }

    #[test]
    fn token_over_115_fails() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        let cand = write_run(root.path(), "candidate", 80.0, 1200, 100, true);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::NotEligible);
        assert_eq!(failed_gate_ids(&report), vec!["tokens_le_115pct"]);
    }

    #[test]
    fn latency_over_130_fails() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        let cand = write_run(root.path(), "candidate", 80.0, 1000, 140, true);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::NotEligible);
        assert_eq!(failed_gate_ids(&report), vec!["latency_le_130pct"]);
    }

    #[test]
    fn tag_drop_fails() {
        let root = tempfile::tempdir().unwrap();
        let mut base = write_run(root.path(), "baseline", 80.0, 1000, 100, true);
        let mut cand = write_run(root.path(), "candidate", 90.0, 1000, 100, true);
        configure_tag_drop_fixture(&mut base, 80.0, 80.0);
        configure_tag_drop_fixture(&mut cand, 100.0, 79.0);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::NotEligible);
        assert_eq!(failed_gate_ids(&report), vec!["per_tag_no_drop"]);
    }

    fn configure_tag_drop_fixture(run: &mut LoadedRun, other_score: f64, tool_score: f64) {
        let tool_selection = "tool_selection".to_string();
        let other_tags: Vec<_> = GATING_TAGS
            .iter()
            .map(|tag| tag.as_str().to_string())
            .filter(|tag| tag != &tool_selection)
            .collect();
        run.results.cases[0].tags = other_tags.clone();
        set_attempt_scores(&mut run.results.cases[0], other_score);
        run.manifest.case_policies.get_mut("val-a").unwrap().tags = other_tags;

        let mut tool_case = run.results.cases[0].clone();
        tool_case.case_id = "val-b".into();
        tool_case.tags = vec![tool_selection.clone()];
        set_attempt_scores(&mut tool_case, tool_score);
        run.results.cases.push(tool_case);
        run.manifest.case_ids.push("val-b".into());
        run.manifest.case_policies.insert(
            "val-b".into(),
            ManifestCasePolicy {
                split: "validation".into(),
                must_pass: true,
                weight: 1.0,
                tags: vec![tool_selection],
            },
        );
        refresh_derived_results(&mut run.results);
    }

    fn set_attempt_scores(case: &mut CaseResultRow, score: f64) {
        for attempt in &mut case.attempts {
            attempt.passed = Some(true);
            attempt.score = Some(score);
        }
    }

    fn refresh_derived_results(results: &mut RunResults) {
        for case in &mut results.cases {
            let repetitions = match case.split.as_str() {
                "optimization" => 1,
                "validation" | "scorecard" => 3,
                _ => 0,
            };
            let passes: Vec<_> = case.attempts.iter().map(|attempt| attempt.passed).collect();
            let scores: Vec<_> = case.attempts.iter().map(|attempt| attempt.score).collect();
            (case.passed, case.score) =
                aggregate_case_repetitions(case.must_pass, &passes, &scores, repetitions);
        }
        let splits: Vec<_> = results
            .splits
            .iter()
            .filter_map(|split| match split.as_str() {
                "optimization" => Some(super::super::case::EvalSplit::Optimization),
                "validation" => Some(super::super::case::EvalSplit::Validation),
                "scorecard" => Some(super::super::case::EvalSplit::Scorecard),
                _ => None,
            })
            .collect();
        (results.overall, results.per_tag) = aggregate_split_scores(&results.cases, &splits);
        let validation_attempts: Vec<_> = results
            .cases
            .iter()
            .filter(|case| case.split == "validation")
            .flat_map(|case| &case.attempts)
            .collect();
        results.validation_attempt_gate_tokens = validation_attempts
            .iter()
            .map(|attempt| attempt.gate_total_tokens)
            .collect();
        results.validation_mean_gate_tokens =
            mean_gate_tokens(&results.validation_attempt_gate_tokens);
        results.validation_completed_latencies_ms = validation_attempts
            .iter()
            .filter(|attempt| attempt.status == "completed")
            .map(|attempt| attempt.wall_latency_ms)
            .collect();
        results.validation_median_latency_ms =
            median_latency_ms(&results.validation_completed_latencies_ms);
        let completeness = derive_results_completeness(&results.cases);
        results.all_completed = completeness.all_completed;
        results.any_inconclusive = completeness.any_inconclusive;
        results.any_resource_incomplete = completeness.any_resource_incomplete;
    }

    #[test]
    fn effective_config_mismatch_incomparable() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        // Candidate with different effective config.
        let run_dir = create_run_dir(root.path(), "candidate").unwrap();
        let harness = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let mut input = sample_input();
        input.main_runtime.max_steps = 99;
        let eff = EffectiveConfigSnapshot::from_input(input).unwrap();
        let bytes = eff.normalize_bytes().unwrap();
        let eref = write_effective_config_snapshot(&run_dir, &bytes).unwrap();
        let manifest = sample_manifest("candidate", eref, href);
        write_manifest(&run_dir, &manifest).unwrap();
        let results = make_results(true, 80.0, 1000, 100);
        fs::write(
            run_dir.join("results.json"),
            serde_json::to_vec_pretty(&results).unwrap(),
        )
        .unwrap();
        let cand = load_run(root.path(), "candidate").unwrap();
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::Incomparable);
        assert!(report
            .mismatches
            .iter()
            .any(|m| m.contains("effective_config")));
        assert!(report.effective_config_diff.is_some());
    }

    #[test]
    fn harness_diff_allowed() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        let run_dir = create_run_dir(root.path(), "candidate").unwrap();
        let mut map = BTreeMap::new();
        map.insert("main.system_prompt".into(), "CHANGED PROMPT".into());
        let harness = HarnessSnapshot::from_map(map);
        let href = write_harness_snapshot(&run_dir, &harness).unwrap();
        let eff = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let bytes = eff.normalize_bytes().unwrap();
        let eref = write_effective_config_snapshot(&run_dir, &bytes).unwrap();
        let manifest = sample_manifest("candidate", eref, href);
        write_manifest(&run_dir, &manifest).unwrap();
        fs::write(
            run_dir.join("results.json"),
            serde_json::to_vec_pretty(&make_results(true, 80.0, 1000, 100)).unwrap(),
        )
        .unwrap();
        let cand = load_run(root.path(), "candidate").unwrap();
        let report = compare_runs(&base, &cand);
        assert_ne!(report.status, CompareStatus::Incomparable);
        assert!(report
            .notes
            .iter()
            .any(|n| n.contains("harness snapshot differs")));
    }

    #[test]
    fn tampered_hash_file_rejected_on_load() {
        let root = tempfile::tempdir().unwrap();
        let _ = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        // Tamper hash file.
        fs::write(
            root.path()
                .join("baseline/effective-config/snapshot.sha256"),
            "deadbeef\n",
        )
        .unwrap();
        let err = load_run(root.path(), "baseline").unwrap_err();
        assert!(err.contains("sha256") || err.contains("disagrees") || err.contains("mismatch"));
    }

    fn gate<'a>(report: &'a CompareReport, gate_id: &str) -> &'a GateResult {
        report
            .gates
            .iter()
            .find(|gate| gate.gate_id == gate_id)
            .unwrap_or_else(|| panic!("missing gate {gate_id}"))
    }

    fn gate_ids(report: &CompareReport) -> Vec<&str> {
        report
            .gates
            .iter()
            .map(|gate| gate.gate_id.as_str())
            .collect()
    }

    fn failed_gate_ids(report: &CompareReport) -> Vec<&str> {
        report
            .gates
            .iter()
            .filter(|gate| !gate.passed)
            .map(|gate| gate.gate_id.as_str())
            .collect()
    }
}
