//! Versioned eval case schema, session seeds, and corpus validation.
//!
//! Validation runs before any model call. Illegal corpora fail with case id and
//! field names so authors can fix the data without guessing.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use orchest::budget::BudgetUsage;
use orchest::model::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Case corpus schema version accepted by this build.
pub const CASE_SCHEMA_VERSION: &str = "1";
/// Session seed schema version accepted by this build.
pub const SEED_SCHEMA_VERSION: &str = "1";

/// All behavior tags that validation must cover.
pub const GATING_TAGS: [BehaviorTag; 7] = [
    BehaviorTag::ToolSelection,
    BehaviorTag::ToolChaining,
    BehaviorTag::ModalityCoverage,
    BehaviorTag::ConflictReconciliation,
    BehaviorTag::ReportStructure,
    BehaviorTag::CitationQuality,
    BehaviorTag::FollowupGrounding,
];

/// Split assignment for an eval case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalSplit {
    Optimization,
    Validation,
    Scorecard,
}

impl EvalSplit {
    pub fn as_str(self) -> &'static str {
        match self {
            EvalSplit::Optimization => "optimization",
            EvalSplit::Validation => "validation",
            EvalSplit::Scorecard => "scorecard",
        }
    }
}

/// How the runner should start the attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RunMode {
    /// Fresh agent run with a user question.
    Fresh { question: String },
    /// Resume from a versioned session seed with a follow-up question.
    FollowUp {
        question: String,
        session_seed_id: String,
        /// Expected content hash of the seed after normalization.
        session_seed_hash: String,
    },
}

/// Registered behavior tags used by validation gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorTag {
    ToolSelection,
    ToolChaining,
    ModalityCoverage,
    ConflictReconciliation,
    ReportStructure,
    CitationQuality,
    FollowupGrounding,
}

impl BehaviorTag {
    pub fn as_str(self) -> &'static str {
        match self {
            BehaviorTag::ToolSelection => "tool_selection",
            BehaviorTag::ToolChaining => "tool_chaining",
            BehaviorTag::ModalityCoverage => "modality_coverage",
            BehaviorTag::ConflictReconciliation => "conflict_reconciliation",
            BehaviorTag::ReportStructure => "report_structure",
            BehaviorTag::CitationQuality => "citation_quality",
            BehaviorTag::FollowupGrounding => "followup_grounding",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        GATING_TAGS.into_iter().find(|t| t.as_str() == s)
    }
}

/// Tool selection contract for a case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolConstraints {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub forbidden: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

/// Partial order edge: `before` must complete before `after` when both run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderConstraint {
    pub before: String,
    pub after: String,
}

/// Expected factual claim the output should contain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedFact {
    pub id: String,
    /// Substrings that must all appear for the fact to count as present.
    pub must_contain: Vec<String>,
    #[serde(default)]
    pub source_fixture: Option<String>,
}

/// Expected retention conflict: both numbers + attributed sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedConflict {
    pub left_value: String,
    pub left_source: String,
    pub right_value: String,
    pub right_source: String,
}

/// Report structure expectations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportExpectations {
    #[serde(default)]
    pub required_sections: Vec<String>,
}

/// Grader configuration for one case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraderSpec {
    pub grader_id: String,
    pub weight: f64,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

/// One versioned eval case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalCase {
    pub case_id: String,
    pub scenario_family: String,
    pub tags: Vec<BehaviorTag>,
    pub split: EvalSplit,
    pub run: RunMode,
    #[serde(default)]
    pub must_pass: bool,
    /// Positive case weight used by overall / per-tag aggregation.
    pub weight: f64,
    #[serde(default)]
    pub tools: ToolConstraints,
    #[serde(default)]
    pub order: Vec<OrderConstraint>,
    #[serde(default)]
    pub expected_facts: Vec<ExpectedFact>,
    #[serde(default)]
    pub expected_conflict: Option<ExpectedConflict>,
    #[serde(default)]
    pub report: ReportExpectations,
    #[serde(default)]
    pub fixture_refs: Vec<String>,
    #[serde(default)]
    pub graders: Vec<GraderSpec>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Versioned synthetic session seed for follow-up cases.
///
/// Mutable runtime fields (session/run ids, store path, harness config) are
/// intentionally absent. Presence of unknown keys is rejected at load time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSeed {
    pub schema_version: String,
    pub seed_id: String,
    pub messages: Vec<Message>,
    pub step: u32,
    pub budget_used: BudgetUsage,
}

/// Loaded and validated corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseCorpus {
    pub schema_version: String,
    pub cases: Vec<EvalCase>,
}

/// Errors produced while loading or validating cases/seeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseLoadError {
    pub message: String,
    pub case_id: Option<String>,
    pub field: Option<String>,
}

impl CaseLoadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            case_id: None,
            field: None,
        }
    }

    pub fn with_case(mut self, case_id: impl Into<String>) -> Self {
        self.case_id = Some(case_id.into());
        self
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }
}

impl std::fmt::Display for CaseLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.case_id, &self.field) {
            (Some(case_id), Some(field)) => {
                write!(f, "case '{case_id}' field '{field}': {}", self.message)
            }
            (Some(case_id), None) => write!(f, "case '{case_id}': {}", self.message),
            (None, Some(field)) => write!(f, "field '{field}': {}", self.message),
            (None, None) => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for CaseLoadError {}

/// Default package-relative paths.
pub fn default_cases_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evals/cases.json")
}

pub fn default_seeds_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evals/session-seeds")
}

pub fn default_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/research")
}

/// Load and validate the case corpus against fixture inventory and seeds.
pub fn load_corpus(
    cases_path: &Path,
    fixtures_dir: &Path,
    seeds_dir: &Path,
) -> Result<CaseCorpus, CaseLoadError> {
    let raw = fs::read_to_string(cases_path).map_err(|e| {
        CaseLoadError::new(format!("reading {}: {e}", cases_path.display())).with_field("path")
    })?;
    let corpus: CaseCorpus = serde_json::from_str(&raw).map_err(|e| {
        CaseLoadError::new(format!("parse error: {e}")).with_field("schema_version")
    })?;
    corpus.validate(fixtures_dir, seeds_dir)?;
    Ok(corpus)
}

/// Load one session seed file and reject mutable/extra fields.
pub fn load_session_seed(path: &Path) -> Result<SessionSeed, CaseLoadError> {
    let raw = fs::read_to_string(path).map_err(|e| {
        CaseLoadError::new(format!("reading {}: {e}", path.display())).with_field("session_seed")
    })?;
    parse_session_seed_json(&raw)
}

/// Parse seed JSON, rejecting mutable keys.
pub fn parse_session_seed_json(raw: &str) -> Result<SessionSeed, CaseLoadError> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| CaseLoadError::new(format!("seed parse error: {e}")).with_field("seed"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| CaseLoadError::new("seed must be a JSON object").with_field("seed"))?;

    const ALLOWED: &[&str] = &[
        "schema_version",
        "seed_id",
        "messages",
        "step",
        "budget_used",
    ];
    const FORBIDDEN: &[&str] = &[
        "session_id",
        "run_id",
        "store_path",
        "active_config",
        "harness",
        "harness_config",
        "config",
    ];

    for key in obj.keys() {
        if FORBIDDEN.contains(&key.as_str()) {
            return Err(CaseLoadError::new(format!(
                "mutable field '{key}' is not allowed in session seeds"
            ))
            .with_field(key.clone()));
        }
        if !ALLOWED.contains(&key.as_str()) {
            return Err(CaseLoadError::new(format!(
                "unknown field '{key}' is not allowed in session seeds"
            ))
            .with_field(key.clone()));
        }
    }

    let seed: SessionSeed = serde_json::from_value(value)
        .map_err(|e| CaseLoadError::new(format!("seed decode error: {e}")).with_field("seed"))?;

    if seed.schema_version != SEED_SCHEMA_VERSION {
        return Err(CaseLoadError::new(format!(
            "unsupported seed schema_version '{}'; expected '{SEED_SCHEMA_VERSION}'",
            seed.schema_version
        ))
        .with_field("schema_version")
        .with_case(seed.seed_id.clone()));
    }
    if seed.seed_id.trim().is_empty() {
        return Err(CaseLoadError::new("seed_id must be non-empty").with_field("seed_id"));
    }
    if seed.messages.is_empty() {
        return Err(CaseLoadError::new("messages must be non-empty")
            .with_field("messages")
            .with_case(seed.seed_id.clone()));
    }
    Ok(seed)
}

impl SessionSeed {
    /// Canonical content hash over normalized seed JSON (stable field order).
    pub fn content_hash(&self) -> Result<String, CaseLoadError> {
        let bytes = normalize_seed_bytes(self)?;
        Ok(hex_sha256(&bytes))
    }
}

/// Normalize seed to stable JSON bytes for hashing.
pub fn normalize_seed_bytes(seed: &SessionSeed) -> Result<Vec<u8>, CaseLoadError> {
    // Re-encode through Value so map keys sort stably under serde_json.
    let value = serde_json::to_value(seed)
        .map_err(|e| CaseLoadError::new(format!("seed serialize: {e}")).with_field("seed"))?;
    let normalized = sort_value(value);
    serde_json::to_vec(&normalized)
        .map_err(|e| CaseLoadError::new(format!("seed normalize: {e}")).with_field("seed"))
}

fn sort_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(v) = map.get(&k) {
                    out.insert(k, sort_value(v.clone()));
                }
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_value).collect())
        }
        other => other,
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

impl CaseCorpus {
    /// Validate schema, split counts, isolation, tags, fixtures, and seeds.
    pub fn validate(&self, fixtures_dir: &Path, seeds_dir: &Path) -> Result<(), CaseLoadError> {
        if self.schema_version != CASE_SCHEMA_VERSION {
            return Err(CaseLoadError::new(format!(
                "unsupported corpus schema_version '{}'; expected '{CASE_SCHEMA_VERSION}'",
                self.schema_version
            ))
            .with_field("schema_version"));
        }

        if self.cases.is_empty() {
            return Err(CaseLoadError::new("corpus has no cases").with_field("cases"));
        }

        let fixture_inventory = list_fixture_basenames(fixtures_dir)?;
        let mut seed_cache: HashMap<String, SessionSeed> = HashMap::new();

        let mut seen_ids = HashSet::new();
        let mut family_to_split: HashMap<String, EvalSplit> = HashMap::new();
        let mut split_counts = BTreeMap::from([
            (EvalSplit::Optimization, 0usize),
            (EvalSplit::Validation, 0usize),
            (EvalSplit::Scorecard, 0usize),
        ]);
        let mut validation_tags: BTreeSet<BehaviorTag> = BTreeSet::new();

        for case in &self.cases {
            self.validate_case(case, &fixture_inventory, seeds_dir, &mut seed_cache)?;

            if !seen_ids.insert(case.case_id.clone()) {
                return Err(CaseLoadError::new("duplicate case_id")
                    .with_case(case.case_id.clone())
                    .with_field("case_id"));
            }

            if let Some(prev) = family_to_split.get(&case.scenario_family) {
                if *prev != case.split {
                    return Err(CaseLoadError::new(format!(
                        "scenario_family crosses splits (already in {}, now in {})",
                        prev.as_str(),
                        case.split.as_str()
                    ))
                    .with_case(case.case_id.clone())
                    .with_field("scenario_family"));
                }
            } else {
                family_to_split.insert(case.scenario_family.clone(), case.split);
            }

            *split_counts.entry(case.split).or_default() += 1;

            if case.split == EvalSplit::Validation {
                for tag in &case.tags {
                    validation_tags.insert(*tag);
                }
            }
        }

        for (split, expected) in [
            (EvalSplit::Optimization, 10usize),
            (EvalSplit::Validation, 4usize),
            (EvalSplit::Scorecard, 4usize),
        ] {
            let actual = split_counts.get(&split).copied().unwrap_or(0);
            if actual != expected {
                return Err(CaseLoadError::new(format!(
                    "split '{}' must have exactly {expected} cases, found {actual}",
                    split.as_str()
                ))
                .with_field("split"));
            }
        }

        for tag in GATING_TAGS {
            if !validation_tags.contains(&tag) {
                return Err(CaseLoadError::new(format!(
                    "behavior tag '{}' is not covered by any validation case",
                    tag.as_str()
                ))
                .with_field("tags"));
            }
        }

        Ok(())
    }

    fn validate_case(
        &self,
        case: &EvalCase,
        fixture_inventory: &BTreeSet<String>,
        seeds_dir: &Path,
        seed_cache: &mut HashMap<String, SessionSeed>,
    ) -> Result<(), CaseLoadError> {
        if case.case_id.trim().is_empty() {
            return Err(CaseLoadError::new("case_id must be non-empty").with_field("case_id"));
        }
        if case.scenario_family.trim().is_empty() {
            return Err(CaseLoadError::new("scenario_family must be non-empty")
                .with_case(case.case_id.clone())
                .with_field("scenario_family"));
        }
        if case.tags.is_empty() {
            return Err(
                CaseLoadError::new("tags must contain at least one behavior tag")
                    .with_case(case.case_id.clone())
                    .with_field("tags"),
            );
        }
        if !(case.weight.is_finite() && case.weight > 0.0) {
            return Err(
                CaseLoadError::new("weight must be a positive finite number")
                    .with_case(case.case_id.clone())
                    .with_field("weight"),
            );
        }

        match &case.run {
            RunMode::Fresh { question } => {
                if question.trim().is_empty() {
                    return Err(CaseLoadError::new("question must be non-empty")
                        .with_case(case.case_id.clone())
                        .with_field("run.question"));
                }
            }
            RunMode::FollowUp {
                question,
                session_seed_id,
                session_seed_hash,
            } => {
                if question.trim().is_empty() {
                    return Err(CaseLoadError::new("question must be non-empty")
                        .with_case(case.case_id.clone())
                        .with_field("run.question"));
                }
                if session_seed_id.trim().is_empty() {
                    return Err(CaseLoadError::new("session_seed_id must be non-empty")
                        .with_case(case.case_id.clone())
                        .with_field("run.session_seed_id"));
                }
                if session_seed_hash.trim().is_empty() {
                    return Err(CaseLoadError::new("session_seed_hash must be non-empty")
                        .with_case(case.case_id.clone())
                        .with_field("run.session_seed_hash"));
                }

                let seed = if let Some(seed) = seed_cache.get(session_seed_id) {
                    seed.clone()
                } else {
                    let path = seeds_dir.join(format!("{session_seed_id}.json"));
                    if !path.exists() {
                        return Err(CaseLoadError::new(format!(
                            "session seed file missing: {}",
                            path.display()
                        ))
                        .with_case(case.case_id.clone())
                        .with_field("run.session_seed_id"));
                    }
                    let seed =
                        load_session_seed(&path).map_err(|e| e.with_case(case.case_id.clone()))?;
                    if seed.seed_id != *session_seed_id {
                        return Err(CaseLoadError::new(format!(
                            "seed_id inside file '{}' does not match referenced id '{}'",
                            seed.seed_id, session_seed_id
                        ))
                        .with_case(case.case_id.clone())
                        .with_field("run.session_seed_id"));
                    }
                    seed_cache.insert(session_seed_id.clone(), seed.clone());
                    seed
                };

                let actual_hash = seed
                    .content_hash()
                    .map_err(|e| e.with_case(case.case_id.clone()))?;
                if actual_hash != *session_seed_hash {
                    return Err(CaseLoadError::new(format!(
                        "session_seed_hash mismatch: expected {session_seed_hash}, got {actual_hash}"
                    ))
                    .with_case(case.case_id.clone())
                    .with_field("run.session_seed_hash"));
                }
            }
        }

        if case.expected_facts.is_empty()
            && case.expected_conflict.is_none()
            && case.report.required_sections.is_empty()
            && case.tools.required.is_empty()
            && case.tools.forbidden.is_empty()
            && case.order.is_empty()
        {
            return Err(CaseLoadError::new(
                "case has empty expectations (facts/conflict/report/tools/order)",
            )
            .with_case(case.case_id.clone())
            .with_field("expected"));
        }

        for fixture in &case.fixture_refs {
            if !fixture_inventory.contains(fixture.as_str()) {
                return Err(CaseLoadError::new(format!(
                    "fixture_refs entry '{fixture}' is not in the fixtures inventory"
                ))
                .with_case(case.case_id.clone())
                .with_field("fixture_refs"));
            }
        }

        for fact in &case.expected_facts {
            if fact.id.trim().is_empty() {
                return Err(CaseLoadError::new("expected fact id must be non-empty")
                    .with_case(case.case_id.clone())
                    .with_field("expected_facts.id"));
            }
            if fact.must_contain.is_empty() {
                return Err(
                    CaseLoadError::new("expected fact must_contain must be non-empty")
                        .with_case(case.case_id.clone())
                        .with_field("expected_facts.must_contain"),
                );
            }
            if let Some(src) = &fact.source_fixture {
                if !fixture_inventory.contains(src.as_str()) {
                    return Err(CaseLoadError::new(format!(
                        "expected fact source_fixture '{src}' is not in fixtures inventory"
                    ))
                    .with_case(case.case_id.clone())
                    .with_field("expected_facts.source_fixture"));
                }
            }
        }

        if let Some(conflict) = &case.expected_conflict {
            for (field, value) in [
                ("expected_conflict.left_value", &conflict.left_value),
                ("expected_conflict.left_source", &conflict.left_source),
                ("expected_conflict.right_value", &conflict.right_value),
                ("expected_conflict.right_source", &conflict.right_source),
            ] {
                if value.trim().is_empty() {
                    return Err(CaseLoadError::new("conflict field must be non-empty")
                        .with_case(case.case_id.clone())
                        .with_field(field));
                }
            }
            for src in [&conflict.left_source, &conflict.right_source] {
                if !fixture_inventory.contains(src.as_str()) {
                    return Err(CaseLoadError::new(format!(
                        "conflict source '{src}' is not in fixtures inventory"
                    ))
                    .with_case(case.case_id.clone())
                    .with_field("expected_conflict"));
                }
            }
        }

        for g in &case.graders {
            if g.grader_id.trim().is_empty() {
                return Err(CaseLoadError::new("grader_id must be non-empty")
                    .with_case(case.case_id.clone())
                    .with_field("graders.grader_id"));
            }
            if !(g.weight.is_finite() && g.weight > 0.0) {
                return Err(CaseLoadError::new("grader weight must be positive")
                    .with_case(case.case_id.clone())
                    .with_field("graders.weight"));
            }
        }

        for edge in &case.order {
            if edge.before.trim().is_empty() || edge.after.trim().is_empty() {
                return Err(
                    CaseLoadError::new("order constraint tools must be non-empty")
                        .with_case(case.case_id.clone())
                        .with_field("order"),
                );
            }
        }

        Ok(())
    }

    pub fn cases_for_split(&self, split: EvalSplit) -> Vec<&EvalCase> {
        self.cases.iter().filter(|c| c.split == split).collect()
    }
}

fn list_fixture_basenames(fixtures_dir: &Path) -> Result<BTreeSet<String>, CaseLoadError> {
    let mut out = BTreeSet::new();
    let entries = fs::read_dir(fixtures_dir).map_err(|e| {
        CaseLoadError::new(format!(
            "reading fixtures dir {}: {e}",
            fixtures_dir.display()
        ))
        .with_field("fixtures")
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            CaseLoadError::new(format!("reading fixtures entry: {e}")).with_field("fixtures")
        })?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                out.insert(name.to_string());
            }
        }
    }
    if out.is_empty() {
        return Err(CaseLoadError::new("fixtures inventory is empty").with_field("fixtures"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest::model::{ContentBlock, Role};
    use serde_json::json;

    fn fixtures_dir() -> PathBuf {
        default_fixtures_dir()
    }

    fn seeds_dir() -> PathBuf {
        default_seeds_dir()
    }

    fn sample_seed() -> SessionSeed {
        SessionSeed {
            schema_version: SEED_SCHEMA_VERSION.to_string(),
            seed_id: "seed-followup-retention".to_string(),
            messages: vec![
                Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text(
                        "Is Loom worth continued investment in Q4?".into(),
                    )],
                },
                Message {
                    role: Role::Assistant,
                    content: vec![ContentBlock::Text(
                        "Referral retention is 42% in the finance cut; support cites 35%.".into(),
                    )],
                },
            ],
            step: 4,
            budget_used: BudgetUsage {
                tokens_used: 1200,
                tool_calls_used: 6,
                cost_usd: 0.0,
            },
        }
    }

    fn base_case(id: &str, family: &str, split: EvalSplit, tags: Vec<BehaviorTag>) -> EvalCase {
        EvalCase {
            case_id: id.into(),
            scenario_family: family.into(),
            tags,
            split,
            run: RunMode::Fresh {
                question: "Is Loom worth continued investment in Q4?".into(),
            },
            must_pass: false,
            weight: 1.0,
            tools: ToolConstraints {
                required: vec!["search_fixtures".into()],
                forbidden: vec![],
                optional: vec![],
            },
            order: vec![],
            expected_facts: vec![ExpectedFact {
                id: "referral_trend".into(),
                must_contain: vec!["42%".into()],
                source_fixture: Some("001-retention-dashboard-notes.md".into()),
            }],
            expected_conflict: None,
            report: ReportExpectations {
                required_sections: vec!["recommendation".into()],
            },
            fixture_refs: vec!["001-retention-dashboard-notes.md".into()],
            graders: vec![GraderSpec {
                grader_id: "tool_selection".into(),
                weight: 1.0,
                required: true,
            }],
            notes: None,
        }
    }

    #[test]
    fn rejects_duplicate_case_ids() {
        let mut corpus = CaseCorpus {
            schema_version: CASE_SCHEMA_VERSION.into(),
            cases: vec![
                base_case(
                    "dup",
                    "fam-a",
                    EvalSplit::Optimization,
                    vec![BehaviorTag::ToolSelection],
                ),
                base_case(
                    "dup",
                    "fam-b",
                    EvalSplit::Optimization,
                    vec![BehaviorTag::ToolSelection],
                ),
            ],
        };
        // pad to pass split counts later — we only care about duplicate id path
        pad_to_valid_counts(&mut corpus, /*skip_validation_tags*/ true);
        let err = corpus
            .validate(&fixtures_dir(), &seeds_dir())
            .expect_err("duplicate ids");
        assert!(err.to_string().contains("case 'dup'"));
        assert_eq!(err.field.as_deref(), Some("case_id"));
    }

    #[test]
    fn rejects_scenario_family_crossing_splits() {
        let mut corpus = CaseCorpus {
            schema_version: CASE_SCHEMA_VERSION.into(),
            cases: vec![
                base_case(
                    "o1",
                    "shared-family",
                    EvalSplit::Optimization,
                    vec![BehaviorTag::ToolSelection],
                ),
                base_case(
                    "v1",
                    "shared-family",
                    EvalSplit::Validation,
                    vec![BehaviorTag::ToolSelection],
                ),
            ],
        };
        pad_to_valid_counts(&mut corpus, true);
        let err = corpus
            .validate(&fixtures_dir(), &seeds_dir())
            .expect_err("cross split family");
        assert!(err.to_string().contains("scenario_family"));
        assert_eq!(err.field.as_deref(), Some("scenario_family"));
    }

    #[test]
    fn rejects_unknown_fixture_ref() {
        let mut case = base_case(
            "bad-fix",
            "fam-fix",
            EvalSplit::Optimization,
            vec![BehaviorTag::CitationQuality],
        );
        case.fixture_refs = vec!["does-not-exist.md".into()];
        let mut corpus = CaseCorpus {
            schema_version: CASE_SCHEMA_VERSION.into(),
            cases: vec![case],
        };
        pad_to_valid_counts(&mut corpus, true);
        let err = corpus
            .validate(&fixtures_dir(), &seeds_dir())
            .expect_err("missing fixture");
        assert!(err.to_string().contains("does-not-exist.md"));
        assert_eq!(err.field.as_deref(), Some("fixture_refs"));
    }

    #[test]
    fn rejects_non_positive_weight() {
        let mut case = base_case(
            "bad-weight",
            "fam-weight",
            EvalSplit::Optimization,
            vec![BehaviorTag::ToolSelection],
        );
        case.weight = 0.0;
        let mut corpus = CaseCorpus {
            schema_version: CASE_SCHEMA_VERSION.into(),
            cases: vec![case],
        };
        pad_to_valid_counts(&mut corpus, true);
        let err = corpus
            .validate(&fixtures_dir(), &seeds_dir())
            .expect_err("bad weight");
        assert_eq!(err.field.as_deref(), Some("weight"));
    }

    #[test]
    fn rejects_empty_expectations() {
        let mut case = base_case(
            "empty-exp",
            "fam-empty",
            EvalSplit::Optimization,
            vec![BehaviorTag::ToolSelection],
        );
        case.tools = ToolConstraints::default();
        case.expected_facts.clear();
        case.report = ReportExpectations::default();
        case.order.clear();
        case.expected_conflict = None;
        let mut corpus = CaseCorpus {
            schema_version: CASE_SCHEMA_VERSION.into(),
            cases: vec![case],
        };
        pad_to_valid_counts(&mut corpus, true);
        let err = corpus
            .validate(&fixtures_dir(), &seeds_dir())
            .expect_err("empty expected");
        assert_eq!(err.field.as_deref(), Some("expected"));
    }

    #[test]
    fn rejects_seed_with_mutable_fields() {
        let raw = json!({
            "schema_version": SEED_SCHEMA_VERSION,
            "seed_id": "seed-x",
            "messages": [{
                "role": "user",
                "content": [{"Text": "hi"}]
            }],
            "step": 1,
            "budget_used": {"tokens_used": 1, "tool_calls_used": 0, "cost_usd": 0.0},
            "session_id": "must-not-appear"
        })
        .to_string();
        let err = parse_session_seed_json(&raw).expect_err("mutable field");
        assert_eq!(err.field.as_deref(), Some("session_id"));
    }

    #[test]
    fn rejects_missing_validation_tag_coverage() {
        // Build a corpus with correct counts but no followup_grounding on validation.
        let mut cases = Vec::new();
        for i in 0..10 {
            cases.push(base_case(
                &format!("opt-{i}"),
                &format!("opt-fam-{i}"),
                EvalSplit::Optimization,
                vec![BehaviorTag::ToolSelection],
            ));
        }
        // Four validation cases covering six tags only (missing followup_grounding).
        let validation_tags = [
            BehaviorTag::ToolSelection,
            BehaviorTag::ToolChaining,
            BehaviorTag::ModalityCoverage,
            BehaviorTag::ConflictReconciliation,
        ];
        for (i, tag) in validation_tags.into_iter().enumerate() {
            cases.push(base_case(
                &format!("val-{i}"),
                &format!("val-fam-{i}"),
                EvalSplit::Validation,
                vec![tag],
            ));
        }
        for i in 0..4 {
            cases.push(base_case(
                &format!("sc-{i}"),
                &format!("sc-fam-{i}"),
                EvalSplit::Scorecard,
                vec![BehaviorTag::ReportStructure],
            ));
        }
        let corpus = CaseCorpus {
            schema_version: CASE_SCHEMA_VERSION.into(),
            cases,
        };
        let err = corpus
            .validate(&fixtures_dir(), &seeds_dir())
            .expect_err("missing tag");
        assert!(
            err.to_string().contains("followup_grounding")
                || err.to_string().contains("report_structure")
                || err.to_string().contains("citation_quality"),
            "unexpected error: {err}"
        );
        assert_eq!(err.field.as_deref(), Some("tags"));
    }

    #[test]
    fn committed_corpus_loads_and_validates() {
        let corpus = load_corpus(&default_cases_path(), &fixtures_dir(), &seeds_dir())
            .expect("committed corpus must validate");
        assert_eq!(corpus.cases.len(), 18);
        assert_eq!(corpus.cases_for_split(EvalSplit::Optimization).len(), 10);
        assert_eq!(corpus.cases_for_split(EvalSplit::Validation).len(), 4);
        assert_eq!(corpus.cases_for_split(EvalSplit::Scorecard).len(), 4);

        let mut tags = BTreeSet::new();
        for case in corpus.cases_for_split(EvalSplit::Validation) {
            for t in &case.tags {
                tags.insert(*t);
            }
        }
        for t in GATING_TAGS {
            assert!(tags.contains(&t), "missing validation tag {}", t.as_str());
        }

        // Families never cross splits.
        let mut family_split = HashMap::new();
        for case in &corpus.cases {
            if let Some(prev) = family_split.insert(case.scenario_family.clone(), case.split) {
                assert_eq!(prev, case.split);
            }
        }
    }

    #[test]

    fn seed_hash_is_stable_and_changes_with_content() {
        let seed = sample_seed();
        let h1 = seed.content_hash().unwrap();
        let h2 = seed.content_hash().unwrap();
        assert_eq!(h1, h2);
        let mut other = seed.clone();
        other.step = 99;
        assert_ne!(h1, other.content_hash().unwrap());
    }

    /// Pad a partial corpus so split counts pass when the test is checking a
    /// different failure mode. When `skip_validation_tags` is true, validation
    /// cases still cover all seven tags so tag-coverage does not mask the
    /// intended error — except when the partial cases already force a family
    /// or id failure first.
    fn pad_to_valid_counts(corpus: &mut CaseCorpus, _skip_validation_tags: bool) {
        let mut opt = corpus
            .cases
            .iter()
            .filter(|c| c.split == EvalSplit::Optimization)
            .count();
        let mut val = corpus
            .cases
            .iter()
            .filter(|c| c.split == EvalSplit::Validation)
            .count();
        let mut sc = corpus
            .cases
            .iter()
            .filter(|c| c.split == EvalSplit::Scorecard)
            .count();

        let mut i = 0;
        while opt < 10 {
            corpus.cases.push(base_case(
                &format!("pad-opt-{i}"),
                &format!("pad-opt-fam-{i}"),
                EvalSplit::Optimization,
                vec![BehaviorTag::ToolSelection],
            ));
            opt += 1;
            i += 1;
        }

        // Ensure validation covers all tags with up to 4 cases.
        let needed_tags = GATING_TAGS.to_vec();
        // Keep existing validation cases; fill remaining slots with multi-tag cases.
        while val < 4 {
            let start = (val * 2) % needed_tags.len();
            let tags = vec![
                needed_tags[start],
                needed_tags[(start + 1) % needed_tags.len()],
            ];
            // For the last validation case, dump remaining tags to guarantee coverage.
            let tags = if val == 3 { needed_tags.clone() } else { tags };
            corpus.cases.push(base_case(
                &format!("pad-val-{val}"),
                &format!("pad-val-fam-{val}"),
                EvalSplit::Validation,
                tags,
            ));
            val += 1;
        }

        while sc < 4 {
            corpus.cases.push(base_case(
                &format!("pad-sc-{sc}"),
                &format!("pad-sc-fam-{sc}"),
                EvalSplit::Scorecard,
                vec![BehaviorTag::ReportStructure],
            ));
            sc += 1;
        }
    }
}
