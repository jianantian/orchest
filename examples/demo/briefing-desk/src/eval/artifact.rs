//! Run artifact layout: harness snapshots, attempt four-file writers, and
//! non-overwrite run directories under `evals/runs/<label>/`.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::harness::{self, SurfaceId};

use super::trajectory::{
    sanitize_free_text, sanitize_value, TrajectoryError, TrajectoryEvent, TrajectoryRecorder,
};

/// Schema version for run manifests.
pub const MANIFEST_SCHEMA_VERSION: &str = "1";
/// Schema version for attempt.json.
pub const ATTEMPT_SCHEMA_VERSION: &str = "1";
/// Relative directory for all sensitive run artifacts.
pub const RUNS_DIR_REL: &str = "evals/runs";

/// Attempt terminal status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Completed,
    ExecutionFailure,
    Inconclusive,
}

/// One editable surface in a harness snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessSurface {
    pub surface_id: String,
    pub text: String,
}

/// Normalized harness snapshot: surfaces sorted by surface_id, LF newlines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessSnapshot {
    pub schema_version: String,
    pub surfaces: Vec<HarnessSurface>,
}

impl HarnessSnapshot {
    pub const SCHEMA_VERSION: &'static str = "1";

    /// Capture current in-process harness surfaces.
    pub fn capture_current() -> Self {
        let mut surfaces: Vec<HarnessSurface> = SurfaceId::ALL
            .into_iter()
            .map(|id| HarnessSurface {
                surface_id: id.as_str().to_string(),
                // CRLF -> LF, no trim — preserve exact candidate text otherwise.
                text: harness::text_for(id)
                    .replace("\r\n", "\n")
                    .replace('\r', "\n"),
            })
            .collect();
        surfaces.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
        Self {
            schema_version: Self::SCHEMA_VERSION.to_string(),
            surfaces,
        }
    }

    /// Build from an explicit map (for tests / alternate sources).
    pub fn from_map(map: BTreeMap<String, String>) -> Self {
        let surfaces = map
            .into_iter()
            .map(|(surface_id, text)| HarnessSurface {
                surface_id,
                text: text.replace("\r\n", "\n").replace('\r', "\n"),
            })
            .collect();
        Self {
            schema_version: Self::SCHEMA_VERSION.to_string(),
            surfaces,
        }
    }

    /// Canonical UTF-8 JSON bytes used for hashing and on-disk snapshot.
    pub fn normalize_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        // Stable field order via serde struct order + sorted surfaces.
        let value = serde_json::to_value(self)
            .map_err(|e| ArtifactError::serialize(format!("harness snapshot: {e}")))?;
        let normalized = sort_value(value);
        let mut bytes = serde_json::to_vec(&normalized)
            .map_err(|e| ArtifactError::serialize(format!("harness normalize: {e}")))?;
        // Pretty is not used; compact JSON with sorted keys.
        // Ensure trailing newline for stable file bytes.
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        Ok(bytes)
    }

    pub fn content_hash(&self) -> Result<String, ArtifactError> {
        Ok(hex_sha256(&self.normalize_bytes()?))
    }

    /// Recover surface text by id.
    pub fn text_for(&self, surface_id: &str) -> Option<&str> {
        self.surfaces
            .iter()
            .find(|s| s.surface_id == surface_id)
            .map(|s| s.text.as_str())
    }

    /// Per-surface content hashes.
    pub fn surface_hashes(&self) -> BTreeMap<String, String> {
        self.surfaces
            .iter()
            .map(|s| {
                let text = s.text.replace("\r\n", "\n").replace('\r', "\n");
                (s.surface_id.clone(), hex_sha256(text.as_bytes()))
            })
            .collect()
    }
}

/// Paths relative to the run root for harness snapshot files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotRef {
    pub path: String,
    pub sha256: String,
}

/// Skeleton run manifest (issue 002 foundations; runner fills remaining fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub schema_version: String,
    pub label: String,
    pub created_unix_ms: u64,
    pub git: GitInfo,
    pub fixture_revision: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub request_options: Value,
    pub harness: SnapshotRef,
    pub harness_surface_hashes: BTreeMap<String, String>,
    pub effective_config: SnapshotRef,
    pub effective_config_schema_version: String,
    pub session_seeds: BTreeMap<String, String>,
    pub case_ids: Vec<String>,
    pub splits: Vec<String>,
    pub repetition: u32,
    pub record_sensitive: bool,
    pub trajectory_schema_version: String,
    pub attempt_schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub commit: Option<String>,
    pub dirty: bool,
    pub dirty_paths: Vec<String>,
}

/// attempt.json payload (grader fields filled later; always written).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub schema_version: String,
    pub case_id: String,
    pub attempt: u32,
    pub status: AttemptStatus,
    pub started_unix_ms: u64,
    pub ended_unix_ms: u64,
    pub wall_latency_ms: u64,
    pub terminal_kind: Option<String>,
    pub stop_reason: Option<String>,
    pub tokens: Value,
    pub resource_coverage: Value,
    pub session_seed_id: Option<String>,
    pub session_seed_hash: Option<String>,
    pub store_cleanup: StoreCleanupRecord,
    pub error: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreCleanupRecord {
    pub attempted: bool,
    pub succeeded: bool,
    pub detail: Option<String>,
}

/// scores.json placeholder when graders have not run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoresPlaceholder {
    pub grader_status: String,
    pub aggregate: Option<Value>,
    pub graders: Vec<Value>,
}

impl ScoresPlaceholder {
    pub fn not_run() -> Self {
        Self {
            grader_status: "not_run".into(),
            aggregate: None,
            graders: vec![],
        }
    }
}

/// Errors from artifact I/O / preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactError {
    pub message: String,
}

impl ArtifactError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
    pub fn io(message: impl Into<String>) -> Self {
        Self::new(message)
    }
    pub fn serialize(message: impl Into<String>) -> Self {
        Self::new(message)
    }
    pub fn preflight(message: impl Into<String>) -> Self {
        Self::new(message)
    }
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ArtifactError {}

impl From<TrajectoryError> for ArtifactError {
    fn from(value: TrajectoryError) -> Self {
        Self::new(value.message)
    }
}

/// Default package-relative runs root.
pub fn default_runs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RUNS_DIR_REL)
}

/// Require explicit sensitive recording confirmation before any model call.
pub fn require_record_sensitive(record_sensitive: bool) -> Result<(), ArtifactError> {
    if record_sensitive {
        Ok(())
    } else {
        Err(ArtifactError::preflight(
            "eval run requires explicit --record-sensitive confirmation before recording trajectories",
        ))
    }
}

/// Create `evals/runs/<label>/` and refuse if it already exists.
pub fn create_run_dir(runs_root: &Path, label: &str) -> Result<PathBuf, ArtifactError> {
    validate_label(label)?;
    fs::create_dir_all(runs_root).map_err(|e| {
        ArtifactError::io(format!("creating runs root {}: {e}", runs_root.display()))
    })?;
    let run_dir = runs_root.join(label);
    if run_dir.exists() {
        return Err(ArtifactError::preflight(format!(
            "run label '{label}' already exists at {}; refusing to overwrite",
            run_dir.display()
        )));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|e| ArtifactError::io(format!("creating {}: {e}", run_dir.display())))?;
    Ok(run_dir)
}

fn validate_label(label: &str) -> Result<(), ArtifactError> {
    if label.is_empty()
        || label.contains('/')
        || label.contains('\\')
        || label.contains("..")
        || label.starts_with('.')
    {
        return Err(ArtifactError::preflight(format!(
            "invalid run label '{label}': must be a non-empty single path segment"
        )));
    }
    Ok(())
}

/// Write harness snapshot.json + snapshot.sha256 under the run directory.
pub fn write_harness_snapshot(
    run_dir: &Path,
    snapshot: &HarnessSnapshot,
) -> Result<SnapshotRef, ArtifactError> {
    let dir = run_dir.join("harness");
    fs::create_dir_all(&dir)
        .map_err(|e| ArtifactError::io(format!("creating {}: {e}", dir.display())))?;
    let bytes = snapshot.normalize_bytes()?;
    let hash = hex_sha256(&bytes);
    let snap_path = dir.join("snapshot.json");
    let hash_path = dir.join("snapshot.sha256");
    fs::write(&snap_path, &bytes)
        .map_err(|e| ArtifactError::io(format!("writing {}: {e}", snap_path.display())))?;
    // Hash file is bare hex + newline.
    fs::write(&hash_path, format!("{hash}\n"))
        .map_err(|e| ArtifactError::io(format!("writing {}: {e}", hash_path.display())))?;
    // Verify on-disk bytes match hash.
    let on_disk = fs::read(&snap_path)
        .map_err(|e| ArtifactError::io(format!("reading {}: {e}", snap_path.display())))?;
    let on_disk_hash = hex_sha256(&on_disk);
    if on_disk_hash != hash {
        return Err(ArtifactError::io(
            "harness snapshot hash mismatch after write",
        ));
    }
    Ok(SnapshotRef {
        path: "harness/snapshot.json".into(),
        sha256: hash,
    })
}

/// Write effective-config snapshot.json + snapshot.sha256 from already-normalized bytes.
pub fn write_effective_config_snapshot(
    run_dir: &Path,
    normalized_bytes: &[u8],
) -> Result<SnapshotRef, ArtifactError> {
    let dir = run_dir.join("effective-config");
    fs::create_dir_all(&dir)
        .map_err(|e| ArtifactError::io(format!("creating {}: {e}", dir.display())))?;
    let hash = hex_sha256(normalized_bytes);
    let snap_path = dir.join("snapshot.json");
    let hash_path = dir.join("snapshot.sha256");
    fs::write(&snap_path, normalized_bytes)
        .map_err(|e| ArtifactError::io(format!("writing {}: {e}", snap_path.display())))?;
    fs::write(&hash_path, format!("{hash}\n"))
        .map_err(|e| ArtifactError::io(format!("writing {}: {e}", hash_path.display())))?;
    Ok(SnapshotRef {
        path: "effective-config/snapshot.json".into(),
        sha256: hash,
    })
}

/// Write the four attempt files atomically enough for offline use:
/// trajectory.jsonl, output.md, attempt.json, scores.json.
#[allow(clippy::too_many_arguments)]
pub fn write_attempt_artifacts(
    run_dir: &Path,
    case_id: &str,
    attempt: u32,
    trajectory: &TrajectoryRecorder,
    output_md: &str,
    record: &AttemptRecord,
    scores: &ScoresPlaceholder,
) -> Result<PathBuf, ArtifactError> {
    let attempt_dir = run_dir
        .join("cases")
        .join(case_id)
        .join(attempt.to_string());
    fs::create_dir_all(&attempt_dir)
        .map_err(|e| ArtifactError::io(format!("creating {}: {e}", attempt_dir.display())))?;

    trajectory.write_jsonl(&attempt_dir.join("trajectory.jsonl"))?;

    fs::write(attempt_dir.join("output.md"), output_md)
        .map_err(|e| ArtifactError::io(format!("writing output.md: {e}")))?;

    let record = sanitized_attempt_record(record);
    write_json_pretty(&attempt_dir.join("attempt.json"), &record)?;
    write_json_pretty(&attempt_dir.join("scores.json"), scores)?;

    // All four files must exist.
    for name in [
        "trajectory.jsonl",
        "output.md",
        "attempt.json",
        "scores.json",
    ] {
        let p = attempt_dir.join(name);
        if !p.is_file() {
            return Err(ArtifactError::io(format!(
                "missing required attempt file {}",
                p.display()
            )));
        }
    }
    Ok(attempt_dir)
}

/// Apply the shared free-text redactor at the final attempt-artifact boundary.
///
/// The runner is expected to sanitize execution errors before constructing the
/// record, but this final gate also covers future pre-run and cleanup paths.
fn sanitized_attempt_record(record: &AttemptRecord) -> AttemptRecord {
    let mut sanitized = record.clone();
    sanitized.error = sanitized.error.as_ref().map(sanitize_value);
    sanitized.store_cleanup.detail = sanitized
        .store_cleanup
        .detail
        .as_deref()
        .map(sanitize_free_text);
    sanitized
}

/// Write manifest.json at the run root.
pub fn write_manifest(run_dir: &Path, manifest: &RunManifest) -> Result<(), ArtifactError> {
    write_json_pretty(&run_dir.join("manifest.json"), manifest)
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<(), ArtifactError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| ArtifactError::serialize(format!("{}: {e}", path.display())))?;
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    fs::write(path, bytes)
        .map_err(|e| ArtifactError::io(format!("writing {}: {e}", path.display())))
}

/// Collect git commit + dirty paths (best-effort; offline-friendly).
pub fn collect_git_info(repo_root: &Path) -> GitInfo {
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let dirty_paths = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            s.lines()
                .filter_map(|line| {
                    let path = line.get(3..)?.trim();
                    if path.is_empty() {
                        None
                    } else {
                        Some(path.to_string())
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let dirty = !dirty_paths.is_empty();
    GitInfo {
        commit,
        dirty,
        dirty_paths,
    }
}

/// Reject dirty paths outside the harness surface file before model calls.
pub fn preflight_dirty_paths(
    dirty_paths: &[String],
    harness_path_suffix: &str,
) -> Result<(), ArtifactError> {
    let unexpected: Vec<_> = dirty_paths
        .iter()
        .filter(|p| !p.replace('\\', "/").ends_with(harness_path_suffix))
        .cloned()
        .collect();
    if unexpected.is_empty() {
        Ok(())
    } else {
        Err(ArtifactError::preflight(format!(
            "workspace has dirty paths outside harness surface file ({harness_path_suffix}): {}",
            unexpected.join(", ")
        )))
    }
}

/// Fixture revision: sha256 of sorted fixture basenames + file contents.
pub fn fixture_revision(fixtures_dir: &Path) -> Result<String, ArtifactError> {
    let mut entries = Vec::new();
    let rd = fs::read_dir(fixtures_dir).map_err(|e| {
        ArtifactError::io(format!("reading fixtures {}: {e}", fixtures_dir.display()))
    })?;
    for ent in rd {
        let ent = ent.map_err(|e| ArtifactError::io(format!("fixtures entry: {e}")))?;
        let meta = ent
            .metadata()
            .map_err(|e| ArtifactError::io(format!("fixtures metadata: {e}")))?;
        if meta.is_file() {
            let name = ent.file_name().to_string_lossy().into_owned();
            let bytes = fs::read(ent.path()).map_err(|e| {
                ArtifactError::io(format!("reading fixture {}: {e}", ent.path().display()))
            })?;
            let content_sha = hex_sha256(&bytes);
            entries.push((name, content_sha));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let payload = serde_json::to_vec(&entries)
        .map_err(|e| ArtifactError::serialize(format!("fixture revision: {e}")))?;
    Ok(hex_sha256(&payload))
}

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Build a skeleton manifest from already-written snapshot refs.
#[allow(clippy::too_many_arguments)]
pub fn build_manifest_skeleton(
    label: &str,
    git: GitInfo,
    fixture_revision: String,
    provider: Option<String>,
    model: Option<String>,
    request_options: Value,
    harness: SnapshotRef,
    harness_surface_hashes: BTreeMap<String, String>,
    effective_config: SnapshotRef,
    effective_config_schema_version: String,
    session_seeds: BTreeMap<String, String>,
    case_ids: Vec<String>,
    splits: Vec<String>,
    repetition: u32,
) -> RunManifest {
    RunManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        label: label.to_string(),
        created_unix_ms: now_unix_ms(),
        git,
        fixture_revision,
        provider,
        model,
        request_options,
        harness,
        harness_surface_hashes,
        effective_config,
        effective_config_schema_version,
        session_seeds,
        case_ids,
        splits,
        repetition,
        record_sensitive: true,
        trajectory_schema_version: super::trajectory::TRAJECTORY_SCHEMA_VERSION.to_string(),
        attempt_schema_version: ATTEMPT_SCHEMA_VERSION.to_string(),
    }
}

/// Helper: empty attempt record template.
pub fn attempt_record_template(
    case_id: &str,
    attempt: u32,
    status: AttemptStatus,
) -> AttemptRecord {
    let now = now_unix_ms();
    AttemptRecord {
        schema_version: ATTEMPT_SCHEMA_VERSION.to_string(),
        case_id: case_id.to_string(),
        attempt,
        status,
        started_unix_ms: now,
        ended_unix_ms: now,
        wall_latency_ms: 0,
        terminal_kind: None,
        stop_reason: None,
        tokens: json!({}),
        resource_coverage: json!({}),
        session_seed_id: None,
        session_seed_hash: None,
        store_cleanup: StoreCleanupRecord {
            attempted: false,
            succeeded: false,
            detail: None,
        },
        error: None,
    }
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(v) = map.get(&k) {
                    out.insert(k, sort_value(v.clone()));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

/// Re-export for tests that need to parse trajectory lines.
#[allow(dead_code)]
pub type TrajectoryLine = TrajectoryEvent;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::credential::text_has_credentials;
    use crate::eval::trajectory::TrajectoryRecorder;
    use orchest::events::RuntimeEvent;
    use orchest::run::RunId;
    use serde_json::json;

    #[test]
    fn require_record_sensitive_blocks_without_flag() {
        assert!(require_record_sensitive(false).is_err());
        assert!(require_record_sensitive(true).is_ok());
    }

    #[test]
    fn label_conflict_refuses_overwrite() {
        let root = tempfile::tempdir().unwrap();
        let first = create_run_dir(root.path(), "baseline").unwrap();
        assert!(first.is_dir());
        let err = create_run_dir(root.path(), "baseline").unwrap_err();
        assert!(err.message.contains("already exists"));
        assert!(err.message.contains("refusing to overwrite"));
    }

    #[test]
    fn harness_hash_changes_with_text_change() {
        let mut map = BTreeMap::new();
        map.insert("main.system_prompt".into(), "hello".into());
        map.insert("reviewer.system_prompt".into(), "review".into());
        let a = HarnessSnapshot::from_map(map.clone());
        map.insert("main.system_prompt".into(), "hello!".into());
        let b = HarnessSnapshot::from_map(map);
        assert_ne!(a.content_hash().unwrap(), b.content_hash().unwrap());
        // Recover surfaces
        assert_eq!(a.text_for("main.system_prompt"), Some("hello"));
    }

    #[test]
    fn harness_normalizes_crlf_without_trim() {
        let mut map = BTreeMap::new();
        map.insert("main.system_prompt".into(), " line\r\n dual \r".into());
        let snap = HarnessSnapshot::from_map(map);
        assert_eq!(snap.surfaces[0].text, " line\n dual \n");
        // Leading/trailing spaces preserved (no trim).
        assert!(snap.surfaces[0].text.starts_with(' '));
        assert!(snap.surfaces[0].text.contains(" dual \n"));
    }

    #[test]
    fn harness_snapshot_path_and_hash_match_disk() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "h1").unwrap();
        let snap = HarnessSnapshot::capture_current();
        let href = write_harness_snapshot(&run_dir, &snap).unwrap();
        assert_eq!(href.path, "harness/snapshot.json");
        let disk = fs::read(run_dir.join(&href.path)).unwrap();
        assert_eq!(hex_sha256(&disk), href.sha256);
        // surface text recoverable
        let parsed: HarnessSnapshot = serde_json::from_slice(&disk).unwrap();
        let expected = harness::MAIN_SYSTEM_PROMPT.replace("\r\n", "\n");
        assert_eq!(
            parsed.text_for("main.system_prompt"),
            Some(expected.as_str())
        );
        assert!(!parsed.surfaces.is_empty());
        // sorted by surface_id
        let ids: Vec<_> = parsed
            .surfaces
            .iter()
            .map(|s| s.surface_id.as_str())
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn attempt_four_files_always_written_including_failure() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "a1").unwrap();
        let mut traj = TrajectoryRecorder::new();
        traj.observe(&RuntimeEvent::RunFailed {
            error: "provider down".into(),
            kind: orchest::events::RunFailureKind::Other,
        });
        let mut record = attempt_record_template("case-x", 1, AttemptStatus::ExecutionFailure);
        record.terminal_kind = Some("run_failed".into());
        record.error = Some(json!({"message": "provider down"}));
        let scores = ScoresPlaceholder::not_run();
        let dir =
            write_attempt_artifacts(&run_dir, "case-x", 1, &traj, "", &record, &scores).unwrap();
        for name in [
            "trajectory.jsonl",
            "output.md",
            "attempt.json",
            "scores.json",
        ] {
            assert!(dir.join(name).is_file(), "missing {name}");
        }
        let scores_raw = fs::read_to_string(dir.join("scores.json")).unwrap();
        assert!(scores_raw.contains("not_run"));
        assert!(
            scores_raw.contains("\"aggregate\": null") || scores_raw.contains("\"aggregate\":null")
        );
    }

    #[test]
    fn attempt_artifact_redacts_provider_model_and_pre_run_error_canaries() {
        let root = tempfile::tempdir().unwrap();
        let run_dir = create_run_dir(root.path(), "redaction").unwrap();
        let canary = "CREDENTIAL-CANARY-9e97d2";
        let errors = [
            format!("provider failed: --api-key {canary}"),
            format!("model failed: --client-secret {canary}"),
            format!(
                "pre-run failed: https://api.example.test/run?carrier=--client-secret%20{canary}"
            ),
            format!("provider failed: api_key={canary}"),
            format!("model failed: Authorization: Bearer {canary}"),
            format!("pre-run failed: bearer {canary}"),
        ];

        for (index, message) in errors.iter().enumerate() {
            let record = AttemptRecord {
                error: Some(json!({"message": message})),
                store_cleanup: StoreCleanupRecord {
                    attempted: true,
                    succeeded: false,
                    detail: Some(format!("cleanup failed: bearer {canary}")),
                },
                ..attempt_record_template(
                    "case-redaction",
                    (index + 1) as u32,
                    AttemptStatus::ExecutionFailure,
                )
            };
            let dir = write_attempt_artifacts(
                &run_dir,
                "case-redaction",
                (index + 1) as u32,
                &TrajectoryRecorder::new(),
                "",
                &record,
                &ScoresPlaceholder::not_run(),
            )
            .unwrap();
            let persisted = fs::read_to_string(dir.join("attempt.json")).unwrap();
            assert!(!persisted.contains(canary), "canary leaked in {persisted}");
            let parsed: Value = serde_json::from_str(&persisted).unwrap();
            let message = parsed["error"]["message"].as_str().unwrap();
            assert!(
                !text_has_credentials(message),
                "detector still sees credentials in {message}"
            );
        }
    }

    #[test]
    fn preflight_rejects_non_harness_dirty_paths() {
        let ok = preflight_dirty_paths(
            &["examples/demo/briefing-desk/src/harness.rs".into()],
            "src/harness.rs",
        );
        assert!(ok.is_ok());
        let bad = preflight_dirty_paths(
            &[
                "examples/demo/briefing-desk/src/harness.rs".into(),
                "crates/orchest/src/lib.rs".into(),
            ],
            "src/harness.rs",
        );
        assert!(bad.is_err());
    }

    #[test]
    fn invalid_labels_rejected() {
        let root = tempfile::tempdir().unwrap();
        assert!(create_run_dir(root.path(), "").is_err());
        assert!(create_run_dir(root.path(), "../x").is_err());
        assert!(create_run_dir(root.path(), "a/b").is_err());
    }

    #[test]
    fn run_started_event_compiles_in_test() {
        // Smoke: ensure RunId available for other modules' tests.
        let _ = RuntimeEvent::RunStarted {
            run_id: RunId::new(),
        };
    }
}
