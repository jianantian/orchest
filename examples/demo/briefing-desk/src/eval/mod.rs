#![allow(dead_code, unused_imports)]
//! Briefing Desk Eval Lab: corpus, recording, grading, and comparison.
//!
//! Issue 001: harness-facing case/seed contract.
//! Issue 002: sensitive trajectory recorder, run manifest, effective config,
//! and per-attempt session isolation foundations.

pub mod artifact;
pub mod case;
pub mod effective_config;
pub mod session;
pub mod trajectory;

pub use artifact::{
    attempt_record_template, build_manifest_skeleton, collect_git_info, create_run_dir,
    default_runs_dir, fixture_revision, preflight_dirty_paths, require_record_sensitive,
    write_attempt_artifacts, write_effective_config_snapshot, write_harness_snapshot,
    write_manifest, ArtifactError, AttemptRecord, AttemptStatus, GitInfo, HarnessSnapshot,
    HarnessSurface, RunManifest, ScoresPlaceholder, SnapshotRef, StoreCleanupRecord,
    ATTEMPT_SCHEMA_VERSION, MANIFEST_SCHEMA_VERSION, RUNS_DIR_REL,
};
pub use case::{
    load_corpus, load_session_seed, BehaviorTag, CaseCorpus, CaseLoadError, EvalCase, EvalSplit,
    RunMode, SessionSeed, CASE_SCHEMA_VERSION, GATING_TAGS, SEED_SCHEMA_VERSION,
};
pub use effective_config::{
    fingerprint_registry, surface_id_for_tool, AgentRoleSnapshot, CapabilityRoute,
    EffectiveConfigInput, EffectiveConfigSnapshot, SessionPersistenceMode, ToolFingerprint,
    EFFECTIVE_CONFIG_SCHEMA_VERSION,
};
pub use session::AttemptSession;
pub use trajectory::{
    sanitize_runtime_event, sanitize_value, TrajectoryError, TrajectoryEvent, TrajectoryRecorder,
    RunRelation, SanitizeOutcome, SECRET_REDACTION, TRAJECTORY_SCHEMA_VERSION,
};
