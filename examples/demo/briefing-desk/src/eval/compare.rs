//! Baseline vs candidate comparison and acceptance gates.
//!
//! Never auto-edits harness. Outputs machine JSON + human Markdown only.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::artifact::{hex_sha256, RunManifest};
use super::case::GATING_TAGS;
use super::resource::{mean_gate_tokens, median_latency_ms};

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
    pub gates: Vec<GateResult>,
    pub mismatches: Vec<String>,
    pub baseline_must_pass_failures: Vec<String>,
    pub effective_config_diff: Option<Value>,
    pub notes: Vec<String>,
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
    pub grader_status: String,
}

pub const COMPARE_SCHEMA_VERSION: &str = "1";
pub const RESULTS_SCHEMA_VERSION: &str = "1";

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
    let mut notes = Vec::new();
    let mut effective_config_diff = None;

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
        write_harness_snapshot, write_manifest, GitInfo, HarnessSnapshot, SnapshotRef,
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
                tags: vec!["tool_selection".into()],
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
            any_inconclusive: false,
            any_resource_incomplete: false,
            all_completed: true,
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
        assert!(report.gates.iter().all(|g| g.passed));
    }

    #[test]
    fn invalid_baseline_skips_eligibility() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, false);
        let cand = write_run(root.path(), "candidate", 90.0, 1000, 100, true);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::InvalidBaseline);
        assert!(!report.baseline_must_pass_failures.is_empty());
    }

    #[test]
    fn overall_insufficient_not_eligible() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        let cand = write_run(root.path(), "candidate", 74.0, 1000, 100, true);
        let report = compare_runs(&base, &cand);
        assert_eq!(report.status, CompareStatus::NotEligible);
        let g = report
            .gates
            .iter()
            .find(|g| g.gate_id == "overall_plus_5")
            .unwrap();
        assert!(!g.passed);
    }

    #[test]
    fn token_over_115_fails() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        let cand = write_run(root.path(), "candidate", 80.0, 1200, 100, true);
        let report = compare_runs(&base, &cand);
        let g = report
            .gates
            .iter()
            .find(|g| g.gate_id == "tokens_le_115pct")
            .unwrap();
        assert!(!g.passed);
    }

    #[test]
    fn latency_over_130_fails() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 70.0, 1000, 100, true);
        let cand = write_run(root.path(), "candidate", 80.0, 1000, 140, true);
        let report = compare_runs(&base, &cand);
        let g = report
            .gates
            .iter()
            .find(|g| g.gate_id == "latency_le_130pct")
            .unwrap();
        assert!(!g.passed);
    }

    #[test]
    fn tag_drop_fails() {
        let root = tempfile::tempdir().unwrap();
        let base = write_run(root.path(), "baseline", 80.0, 1000, 100, true);
        let mut cand = write_run(root.path(), "candidate", 90.0, 1000, 100, true);
        // Drop one tag score after load.
        cand.results.per_tag.insert("tool_selection".into(), 10.0);
        let report = compare_runs(&base, &cand);
        let g = report
            .gates
            .iter()
            .find(|g| g.gate_id == "per_tag_no_drop")
            .unwrap();
        assert!(!g.passed);
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
}
