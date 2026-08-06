//! Shared eval-lab helpers are re-exported for tests and future CLI seams;
//! keep them free of per-item dead_code noise under a single allow.
#![allow(dead_code, unused_imports)]
//! Briefing Desk Eval Lab: corpus, recording, grading, and comparison.
//!
//! Issue 001: harness-facing case/seed contract.
//! Issue 002: sensitive trajectory recorder, run manifest, effective config,
//! and per-attempt session isolation foundations.
//! Issue 003: deterministic graders and aggregation formulas.
//! Issue 004: eval runner, CLI, and candidate comparison.

pub mod artifact;
pub mod case;
pub mod cli;
pub mod compare;
pub mod credential;
pub mod effective_config;
pub mod grader;
pub mod resource;
pub mod runner;
pub mod scripted_model;
pub mod session;
pub mod trajectory;

pub use artifact::{
    attempt_record_template, build_manifest_skeleton, collect_git_info, create_run_dir,
    default_runs_dir, fixture_revision, preflight_dirty_paths, require_record_sensitive,
    write_attempt_artifacts, write_effective_config_snapshot, write_harness_snapshot,
    write_manifest, ArtifactError, AttemptRecord, AttemptStatus, GitInfo, HarnessSnapshot,
    HarnessSurface, ManifestCasePolicy, RunManifest, ScoresPlaceholder, SnapshotRef,
    StoreCleanupRecord, ATTEMPT_SCHEMA_VERSION, MANIFEST_SCHEMA_VERSION, RUNS_DIR_REL,
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
pub use grader::{
    aggregate_attempt, aggregate_case, aggregate_split, aggregate_tag, grade_attempt,
    list_fixture_inventory, run_grader, validate_validation_tag_coverage, AttemptAggregate,
    AttemptGrade, AttemptGraderStatus, AttemptScoreInput, CaseAggregate, GraderError, GraderInput,
    GraderResult, SplitAggregate, TagAggregate, KNOWN_GRADER_IDS,
};
pub use session::AttemptSession;
pub use trajectory::{
    sanitize_runtime_event, sanitize_value, RunRelation, SanitizeOutcome, TrajectoryError,
    TrajectoryEvent, TrajectoryRecorder, SECRET_REDACTION, TRAJECTORY_SCHEMA_VERSION,
};
