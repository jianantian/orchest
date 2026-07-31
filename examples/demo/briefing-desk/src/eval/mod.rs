#![allow(dead_code, unused_imports)]
//! Briefing Desk Eval Lab: corpus, recording, grading, and comparison.
//!
//! Issue 001 lands the harness-facing case/seed contract. Later issues add
//! trajectory recording, graders, runner, and comparison.

pub mod case;

pub use case::{
    load_corpus, load_session_seed, BehaviorTag, CaseCorpus, CaseLoadError, EvalCase, EvalSplit,
    RunMode, SessionSeed, CASE_SCHEMA_VERSION, GATING_TAGS, SEED_SCHEMA_VERSION,
};
