//! Allowlist-only export and deterministic verification for committed eval evidence.
//!
//! This module deliberately reads run-level manifests, numeric results, and the
//! credential-free effective-config snapshot only. It never opens per-attempt
//! `trajectory.jsonl`, `output.md`, `attempt.json`, or `scores.json` files.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use orchest::model::RequestOptions;

use super::artifact::{hex_sha256, GitInfo, ManifestCasePolicy, RunManifest, SnapshotRef};
use super::compare::{compare_runs, load_run, CompareStatus, GateResult, LoadedRun, RunResults};
use super::credential::value_has_credentials;
use super::effective_config::EffectiveConfigSnapshot;

pub const EVIDENCE_SCHEMA_VERSION: &str = "1";
pub const FORMAL_HYPOTHESIS: &str = "Use the smallest sufficient Tool chain and reuse already-read evidence while preserving every required report section, conflict attribution, and real fixture citation.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceError {
    pub message: String,
}

impl EvidenceError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for EvidenceError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceBundle {
    pub schema_version: String,
    pub hypothesis: String,
    pub baseline: EvidenceRun,
    pub candidate: EvidenceRun,
    pub comparison: EvidenceComparison,
    pub human_decision: String,
    pub scorecard_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRun {
    pub label: String,
    pub created_unix_ms: u64,
    pub git_commit: Option<String>,
    pub git_dirty: bool,
    pub git_dirty_paths: Vec<String>,
    pub fixture_revision: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub request_options: RequestOptions,
    pub harness_sha256: String,
    pub harness_surface_hashes: BTreeMap<String, String>,
    pub effective_config_sha256: String,
    pub effective_config_schema_version: String,
    pub effective_config: EffectiveConfigSnapshot,
    pub session_seed_hashes: BTreeMap<String, String>,
    pub case_ids: Vec<String>,
    pub case_policies: BTreeMap<String, ManifestCasePolicy>,
    pub splits: Vec<String>,
    pub repetition: u32,
    pub manifest_schema_version: String,
    pub trajectory_schema_version: String,
    pub attempt_schema_version: String,
    pub results: RunResults,
    /// Hash chain over every source identity/hash retained in this safe record.
    pub source_identity_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceComparison {
    pub schema_version: String,
    pub status: CompareStatus,
    pub gates: Vec<EvidenceGate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceGate {
    pub gate_id: String,
    pub passed: bool,
    pub actual: Value,
    pub threshold: Value,
}

impl From<&GateResult> for EvidenceGate {
    fn from(value: &GateResult) -> Self {
        Self {
            gate_id: value.gate_id.clone(),
            passed: value.passed,
            actual: value.actual.clone(),
            threshold: value.threshold.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedEvidence {
    pub status: CompareStatus,
    pub baseline_overall: Option<f64>,
    pub candidate_overall: Option<f64>,
    pub baseline_validation_tokens: Option<f64>,
    pub candidate_validation_tokens: Option<f64>,
}

pub fn export_evidence_bundle(
    runs_root: &Path,
    baseline_label: &str,
    candidate_label: &str,
    human_decision: &str,
    scorecard_state: &str,
) -> Result<EvidenceBundle, EvidenceError> {
    let baseline = load_run(runs_root, baseline_label).map_err(EvidenceError::new)?;
    let candidate = load_run(runs_root, candidate_label).map_err(EvidenceError::new)?;
    let report = compare_runs(&baseline, &candidate);

    let bundle = EvidenceBundle {
        schema_version: EVIDENCE_SCHEMA_VERSION.into(),
        hypothesis: FORMAL_HYPOTHESIS.into(),
        baseline: EvidenceRun::from_loaded(&baseline)?,
        candidate: EvidenceRun::from_loaded(&candidate)?,
        comparison: EvidenceComparison {
            schema_version: report.schema_version,
            status: report.status,
            gates: report.gates.iter().map(EvidenceGate::from).collect(),
        },
        human_decision: validate_decision(human_decision)?.into(),
        scorecard_state: validate_scorecard_state(scorecard_state)?.into(),
    };

    let value = serde_json::to_value(&bundle)
        .map_err(|error| EvidenceError::new(format!("serialize evidence: {error}")))?;
    if value_has_credentials(&value) {
        return Err(EvidenceError::new(
            "evidence allowlist contains credential material; refusing export",
        ));
    }
    verify_evidence_bundle(&bundle)?;
    Ok(bundle)
}

impl EvidenceRun {
    fn from_loaded(run: &LoadedRun) -> Result<Self, EvidenceError> {
        let effective_config: EffectiveConfigSnapshot =
            serde_json::from_slice(&run.effective_config_bytes).map_err(|error| {
                EvidenceError::new(format!(
                    "parse effective config for '{}': {error}",
                    run.label
                ))
            })?;
        let request_options = serde_json::from_value(run.manifest.request_options.clone())
            .map_err(|error| {
                EvidenceError::new(format!(
                    "parse request options for '{}': {error}",
                    run.label
                ))
            })?;
        let mut evidence = Self {
            label: run.label.clone(),
            created_unix_ms: run.manifest.created_unix_ms,
            git_commit: run.manifest.git.commit.clone(),
            git_dirty: run.manifest.git.dirty,
            git_dirty_paths: run.manifest.git.dirty_paths.clone(),
            fixture_revision: run.manifest.fixture_revision.clone(),
            provider: run.manifest.provider.clone(),
            model: run.manifest.model.clone(),
            request_options,
            harness_sha256: run.harness_hash.clone(),
            harness_surface_hashes: run.manifest.harness_surface_hashes.clone(),
            effective_config_sha256: run.effective_config_hash.clone(),
            effective_config_schema_version: run.manifest.effective_config_schema_version.clone(),
            effective_config,
            session_seed_hashes: run.manifest.session_seeds.clone(),
            case_ids: run.manifest.case_ids.clone(),
            case_policies: run.manifest.case_policies.clone(),
            splits: run.manifest.splits.clone(),
            repetition: run.manifest.repetition,
            manifest_schema_version: run.manifest.schema_version.clone(),
            trajectory_schema_version: run.manifest.trajectory_schema_version.clone(),
            attempt_schema_version: run.manifest.attempt_schema_version.clone(),
            results: run.results.clone(),
            source_identity_sha256: String::new(),
        };
        evidence.source_identity_sha256 = evidence.recompute_source_identity()?;
        Ok(evidence)
    }

    fn to_loaded(&self) -> Result<LoadedRun, EvidenceError> {
        self.verify_source_hashes()?;
        let effective_value = serde_json::to_value(&self.effective_config)
            .map_err(|error| EvidenceError::new(format!("serialize effective config: {error}")))?;
        let effective_config_bytes = canonical_json_bytes(&effective_value)?;
        let recomputed = hex_sha256(&effective_config_bytes);
        if recomputed != self.effective_config_sha256 {
            return Err(EvidenceError::new(format!(
                "{} effective-config hash mismatch: claimed {}, recomputed {}",
                self.label, self.effective_config_sha256, recomputed
            )));
        }
        let manifest = RunManifest {
            schema_version: self.manifest_schema_version.clone(),
            label: self.label.clone(),
            created_unix_ms: self.created_unix_ms,
            git: GitInfo {
                commit: self.git_commit.clone(),
                dirty: self.git_dirty,
                dirty_paths: self.git_dirty_paths.clone(),
            },
            fixture_revision: self.fixture_revision.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            request_options: serde_json::to_value(&self.request_options).map_err(|error| {
                EvidenceError::new(format!("serialize request options: {error}"))
            })?,
            harness: SnapshotRef {
                path: "harness/snapshot.json".into(),
                sha256: self.harness_sha256.clone(),
            },
            harness_surface_hashes: self.harness_surface_hashes.clone(),
            effective_config: SnapshotRef {
                path: "effective-config/snapshot.json".into(),
                sha256: self.effective_config_sha256.clone(),
            },
            effective_config_schema_version: self.effective_config_schema_version.clone(),
            session_seeds: self.session_seed_hashes.clone(),
            case_ids: self.case_ids.clone(),
            case_policies: self.case_policies.clone(),
            splits: self.splits.clone(),
            repetition: self.repetition,
            record_sensitive: true,
            trajectory_schema_version: self.trajectory_schema_version.clone(),
            attempt_schema_version: self.attempt_schema_version.clone(),
        };
        super::compare::validate_results_contract(&manifest, &self.results).map_err(|errors| {
            EvidenceError::new(format!(
                "{} results contract invalid: {}",
                self.label,
                errors.join("; ")
            ))
        })?;
        Ok(LoadedRun {
            label: self.label.clone(),
            dir: PathBuf::new(),
            manifest,
            results: self.results.clone(),
            effective_config_bytes,
            effective_config_hash: recomputed,
            harness_hash: self.harness_sha256.clone(),
        })
    }

    fn verify_source_hashes(&self) -> Result<(), EvidenceError> {
        for (name, value) in std::iter::once(("fixture_revision", &self.fixture_revision))
            .chain(std::iter::once(("harness_sha256", &self.harness_sha256)))
            .chain(std::iter::once((
                "effective_config_sha256",
                &self.effective_config_sha256,
            )))
            .chain(
                self.harness_surface_hashes
                    .values()
                    .map(|value| ("harness_surface_sha256", value)),
            )
            .chain(
                self.session_seed_hashes
                    .values()
                    .map(|value| ("session_seed_sha256", value)),
            )
        {
            validate_sha256(name, value)?;
        }
        let expected = self.recompute_source_identity()?;
        if self.source_identity_sha256 != expected {
            return Err(EvidenceError::new(format!(
                "{} source identity hash mismatch",
                self.label
            )));
        }
        Ok(())
    }

    fn recompute_source_identity(&self) -> Result<String, EvidenceError> {
        let value = serde_json::json!({
            "fixture_revision": self.fixture_revision,
            "harness_sha256": self.harness_sha256,
            "harness_surface_hashes": self.harness_surface_hashes,
            "effective_config_sha256": self.effective_config_sha256,
            "session_seed_hashes": self.session_seed_hashes,
        });
        Ok(hex_sha256(&canonical_json_bytes(&value)?))
    }
}

pub fn verify_evidence_bundle(bundle: &EvidenceBundle) -> Result<VerifiedEvidence, EvidenceError> {
    if bundle.schema_version != EVIDENCE_SCHEMA_VERSION {
        return Err(EvidenceError::new(format!(
            "unsupported evidence schema {}",
            bundle.schema_version
        )));
    }
    if bundle.hypothesis != FORMAL_HYPOTHESIS {
        return Err(EvidenceError::new("formal hypothesis mismatch"));
    }
    validate_decision(&bundle.human_decision)?;
    validate_scorecard_state(&bundle.scorecard_state)?;
    let value = serde_json::to_value(bundle)
        .map_err(|error| EvidenceError::new(format!("serialize evidence: {error}")))?;
    if value_has_credentials(&value) {
        return Err(EvidenceError::new("evidence contains credential material"));
    }

    let baseline = bundle.baseline.to_loaded()?;
    let candidate = bundle.candidate.to_loaded()?;
    let report = compare_runs(&baseline, &candidate);
    let gates: Vec<EvidenceGate> = report.gates.iter().map(EvidenceGate::from).collect();
    if report.schema_version != bundle.comparison.schema_version
        || report.status != bundle.comparison.status
        || gates != bundle.comparison.gates
    {
        return Err(EvidenceError::new(
            "comparison status/gates do not match recomputed production comparison",
        ));
    }
    if bundle.human_decision == "accepted" && report.status != CompareStatus::EligibleForReview {
        return Err(EvidenceError::new(
            "human decision cannot be accepted unless comparison is eligible_for_review",
        ));
    }
    if bundle.scorecard_state != "not_run"
        && (report.status != CompareStatus::EligibleForReview
            || bundle.human_decision != "accepted")
    {
        return Err(EvidenceError::new(
            "scorecard may run only after an eligible, human-accepted candidate",
        ));
    }

    Ok(VerifiedEvidence {
        status: report.status,
        baseline_overall: baseline.results.overall,
        candidate_overall: candidate.results.overall,
        baseline_validation_tokens: baseline.results.validation_mean_gate_tokens,
        candidate_validation_tokens: candidate.results.validation_mean_gate_tokens,
    })
}

pub fn write_evidence_bundle(
    out_dir: &Path,
    bundle: &EvidenceBundle,
) -> Result<(PathBuf, PathBuf, PathBuf), EvidenceError> {
    verify_evidence_bundle(bundle)?;
    fs::create_dir_all(out_dir)
        .map_err(|error| EvidenceError::new(format!("create {}: {error}", out_dir.display())))?;
    let json_path = out_dir.join("bundle.json");
    let hash_path = out_dir.join("bundle.sha256");
    let index_path = out_dir.join("README.md");
    let mut bytes = serde_json::to_vec_pretty(bundle)
        .map_err(|error| EvidenceError::new(format!("serialize evidence: {error}")))?;
    bytes.push(b'\n');
    let hash = hex_sha256(&bytes);
    fs::write(&json_path, &bytes)
        .map_err(|error| EvidenceError::new(format!("write {}: {error}", json_path.display())))?;
    fs::write(&hash_path, format!("{hash}\n"))
        .map_err(|error| EvidenceError::new(format!("write {}: {error}", hash_path.display())))?;
    let verified = verify_evidence_bundle(bundle)?;
    let index = format!(
        "# v0.16 Eval Repair Evidence\n\n- schema: `{}`\n- baseline: `{}`\n- candidate: `{}`\n- comparison: `{}`\n- human decision: `{}`\n- scorecard: `{}`\n- bundle sha256: `{}`\n- baseline overall: `{:?}`\n- candidate overall: `{:?}`\n\nRun `briefing-desk eval verify-evidence {}` to verify hashes and recompute aggregates.\n",
        bundle.schema_version,
        bundle.baseline.label,
        bundle.candidate.label,
        status_str(verified.status),
        bundle.human_decision,
        bundle.scorecard_state,
        hash,
        verified.baseline_overall,
        verified.candidate_overall,
        json_path.display(),
    );
    fs::write(&index_path, index)
        .map_err(|error| EvidenceError::new(format!("write {}: {error}", index_path.display())))?;
    Ok((json_path, hash_path, index_path))
}

pub fn verify_evidence_file(path: &Path) -> Result<VerifiedEvidence, EvidenceError> {
    let bytes = fs::read(path)
        .map_err(|error| EvidenceError::new(format!("read {}: {error}", path.display())))?;
    let expected = fs::read_to_string(path.with_extension("sha256"))
        .map_err(|error| EvidenceError::new(format!("read evidence hash: {error}")))?;
    let actual = hex_sha256(&bytes);
    if expected.trim() != actual {
        return Err(EvidenceError::new(format!(
            "bundle hash mismatch: expected {}, recomputed {}",
            expected.trim(),
            actual
        )));
    }
    let raw_value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| EvidenceError::new(format!("parse evidence: {error}")))?;
    let bundle: EvidenceBundle = serde_json::from_value(raw_value.clone())
        .map_err(|error| EvidenceError::new(format!("parse evidence allowlist: {error}")))?;
    let typed_value = serde_json::to_value(&bundle)
        .map_err(|error| EvidenceError::new(format!("serialize evidence allowlist: {error}")))?;
    if raw_value != typed_value {
        return Err(EvidenceError::new(
            "bundle contains fields outside the typed evidence allowlist",
        ));
    }
    verify_evidence_bundle(&bundle)
}

fn validate_decision(value: &str) -> Result<&str, EvidenceError> {
    match value {
        "accepted" | "rejected" | "not_eligible" | "pending" => Ok(value),
        other => Err(EvidenceError::new(format!(
            "invalid human decision '{other}'"
        ))),
    }
}

fn validate_scorecard_state(value: &str) -> Result<&str, EvidenceError> {
    match value {
        "not_run" | "passed" | "failed" => Ok(value),
        other => Err(EvidenceError::new(format!(
            "invalid scorecard state '{other}'"
        ))),
    }
}

fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, EvidenceError> {
    let mut bytes = serde_json::to_vec(&sort_value(value.clone()))
        .map_err(|error| EvidenceError::new(format!("canonicalize JSON: {error}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn validate_sha256(name: &str, value: &str) -> Result<(), EvidenceError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(EvidenceError::new(format!(
            "{name} is not a lowercase/uppercase SHA-256 hex digest"
        )))
    }
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut sorted = serde_json::Map::new();
            for key in keys {
                if let Some(value) = map.get(&key) {
                    sorted.insert(key, sort_value(value.clone()));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sort_value).collect()),
        other => other,
    }
}

fn status_str(status: CompareStatus) -> &'static str {
    match status {
        CompareStatus::EligibleForReview => "eligible_for_review",
        CompareStatus::NotEligible => "not_eligible",
        CompareStatus::InvalidBaseline => "invalid_baseline",
        CompareStatus::Incomparable => "incomparable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn verifier_rejects_numeric_and_hash_tampering() {
        let mut bundle = synthetic_bundle();
        verify_evidence_bundle(&bundle).expect("fixture verifies");
        bundle.candidate.results.cases[0].attempts[0].score = Some(1.0);
        assert!(verify_evidence_bundle(&bundle)
            .unwrap_err()
            .to_string()
            .contains("results contract"));

        let mut bundle = synthetic_bundle();
        bundle.baseline.effective_config_sha256 = "0".repeat(64);
        assert!(verify_evidence_bundle(&bundle)
            .unwrap_err()
            .to_string()
            .contains("hash mismatch"));

        let mut bundle = synthetic_bundle();
        bundle.baseline.harness_sha256 = "2".repeat(64);
        assert!(verify_evidence_bundle(&bundle)
            .unwrap_err()
            .to_string()
            .contains("source identity hash mismatch"));
    }

    #[test]
    fn verifier_enforces_scorecard_process_contract() {
        let mut bundle = synthetic_bundle();
        bundle.human_decision = "rejected".into();
        bundle.scorecard_state = "passed".into();
        assert!(verify_evidence_bundle(&bundle)
            .unwrap_err()
            .to_string()
            .contains("scorecard may run only"));
    }

    #[test]
    fn allowlist_excludes_raw_payload_and_free_text_fields() {
        let bundle = synthetic_bundle();
        let value = serde_json::to_value(bundle).unwrap();
        let text = serde_json::to_string(&value).unwrap();
        for forbidden in [
            "trajectory.jsonl",
            "output.md",
            "tool_payload",
            "grader_evidence",
            "failure_detail",
            "prompt_reasoning",
            "authorization",
            "cookie",
            "api_key",
        ] {
            assert!(!text.to_ascii_lowercase().contains(forbidden));
        }
    }

    #[test]
    fn exporter_ignores_raw_attempt_files() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = synthetic_bundle();
        write_synthetic_run(temp.path(), &fixture.baseline);
        write_synthetic_run(temp.path(), &fixture.candidate);
        let raw_dir = temp.path().join("baseline/cases/case-1/1");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(
            raw_dir.join("trajectory.jsonl"),
            "{\"tool_payload\":\"RAW-CANARY\",\"api_key\":\"secret\"}\n",
        )
        .unwrap();
        fs::write(raw_dir.join("output.md"), "RAW-CANARY hidden report").unwrap();
        fs::write(
            raw_dir.join("scores.json"),
            "{\"grader_evidence\":\"RAW-CANARY\"}",
        )
        .unwrap();

        let exported =
            export_evidence_bundle(temp.path(), "baseline", "candidate", "accepted", "not_run")
                .unwrap();
        let serialized = serde_json::to_string(&exported).unwrap();
        assert!(!serialized.contains("RAW-CANARY"));
        assert!(!serialized.contains("grader_evidence"));
        assert!(!serialized.contains("tool_payload"));
    }

    fn synthetic_bundle() -> EvidenceBundle {
        let effective_config = super::super::effective_config::EffectiveConfigSnapshot::from_input(
            super::super::effective_config::sample_input(),
        )
        .unwrap();
        let effective_value = serde_json::to_value(&effective_config).unwrap();
        let effective_bytes = canonical_json_bytes(&effective_value).unwrap();
        let hash = hex_sha256(&effective_bytes);
        let baseline = synthetic_run("baseline", 80.0, &hash, effective_config.clone());
        let candidate = synthetic_run("candidate", 90.0, &hash, effective_config);
        let baseline_loaded = baseline.to_loaded().unwrap();
        let candidate_loaded = candidate.to_loaded().unwrap();
        let report = compare_runs(&baseline_loaded, &candidate_loaded);
        assert_eq!(report.status, CompareStatus::EligibleForReview);
        EvidenceBundle {
            schema_version: EVIDENCE_SCHEMA_VERSION.into(),
            hypothesis: FORMAL_HYPOTHESIS.into(),
            baseline,
            candidate,
            comparison: EvidenceComparison {
                schema_version: report.schema_version,
                status: report.status,
                gates: report.gates.iter().map(EvidenceGate::from).collect(),
            },
            human_decision: "accepted".into(),
            scorecard_state: "not_run".into(),
        }
    }

    fn synthetic_run(
        label: &str,
        score: f64,
        hash: &str,
        effective: EffectiveConfigSnapshot,
    ) -> EvidenceRun {
        use super::super::case::GATING_TAGS;
        use super::super::compare::{AttemptResultRow, CaseResultRow, RESULTS_SCHEMA_VERSION};
        use super::super::runner::aggregate_split_scores;

        let tags: Vec<String> = GATING_TAGS
            .iter()
            .map(|tag| tag.as_str().to_string())
            .collect();
        let attempts = (1..=3)
            .map(|attempt| AttemptResultRow {
                attempt,
                status: "completed".into(),
                passed: Some(true),
                score: Some(score),
                wall_latency_ms: 100,
                gate_total_tokens: 100,
                resource_coverage: "complete".into(),
                cost_usd: None,
                cost_complete: false,
                grader_status: "completed".into(),
            })
            .collect();
        let case = super::super::compare::CaseResultRow {
            case_id: "case-1".into(),
            split: "validation".into(),
            must_pass: true,
            weight: 1.0,
            tags: tags.clone(),
            passed: Some(true),
            score: Some(score),
            attempts,
        };
        let cases = vec![case];
        let (overall, per_tag) =
            aggregate_split_scores(&cases, &[super::super::case::EvalSplit::Validation]);
        let results = RunResults {
            schema_version: RESULTS_SCHEMA_VERSION.into(),
            label: label.into(),
            splits: vec!["validation".into()],
            cases,
            overall,
            per_tag,
            validation_mean_gate_tokens: Some(100.0),
            validation_median_latency_ms: Some(100.0),
            validation_attempt_gate_tokens: vec![100, 100, 100],
            validation_completed_latencies_ms: vec![100, 100, 100],
            validation_total_cost_usd: None,
            validation_mean_cost_usd: None,
            validation_cost_complete: false,
            any_inconclusive: false,
            any_resource_incomplete: false,
            all_completed: true,
        };
        let mut run = EvidenceRun {
            label: label.into(),
            created_unix_ms: 1,
            git_commit: Some("abc".into()),
            git_dirty: false,
            git_dirty_paths: vec![],
            fixture_revision: hex_sha256(b"fixture"),
            provider: Some("test".into()),
            model: Some("test-model".into()),
            request_options: RequestOptions {
                max_tokens: Some(4096),
                ..RequestOptions::default()
            },
            harness_sha256: hex_sha256(b"{}\n"),
            harness_surface_hashes: BTreeMap::new(),
            effective_config_sha256: hash.into(),
            effective_config_schema_version: "test".into(),
            effective_config: effective,
            session_seed_hashes: BTreeMap::new(),
            case_ids: vec!["case-1".into()],
            case_policies: BTreeMap::from([(
                "case-1".into(),
                ManifestCasePolicy {
                    split: "validation".into(),
                    must_pass: true,
                    weight: 1.0,
                    tags,
                },
            )]),
            splits: vec!["validation".into()],
            repetition: 3,
            manifest_schema_version: super::super::artifact::MANIFEST_SCHEMA_VERSION.into(),
            trajectory_schema_version: super::super::trajectory::TRAJECTORY_SCHEMA_VERSION.into(),
            attempt_schema_version: super::super::artifact::ATTEMPT_SCHEMA_VERSION.into(),
            results,
            source_identity_sha256: String::new(),
        };
        run.source_identity_sha256 = run.recompute_source_identity().unwrap();
        run
    }

    fn write_synthetic_run(root: &Path, run: &EvidenceRun) {
        let loaded = run.to_loaded().unwrap();
        let dir = root.join(&run.label);
        fs::create_dir_all(dir.join("effective-config")).unwrap();
        fs::create_dir_all(dir.join("harness")).unwrap();
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&loaded.manifest).unwrap(),
        )
        .unwrap();
        fs::write(
            dir.join("results.json"),
            serde_json::to_vec_pretty(&loaded.results).unwrap(),
        )
        .unwrap();
        fs::write(
            dir.join("effective-config/snapshot.json"),
            &loaded.effective_config_bytes,
        )
        .unwrap();
        fs::write(
            dir.join("effective-config/snapshot.sha256"),
            format!("{}\n", loaded.effective_config_hash),
        )
        .unwrap();
        fs::write(dir.join("harness/snapshot.json"), b"{}\n").unwrap();
    }
}
