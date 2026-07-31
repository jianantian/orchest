//! Deterministic graders for Briefing Desk Eval Lab.
//!
//! Graders consume only case contracts, sanitized trajectories, attempt
//! outputs, and the fixture inventory. They never call a model.

mod aggregate;
mod content;
mod tool_flow;

pub use aggregate::{
    aggregate_attempt, aggregate_case, aggregate_split, aggregate_tag,
    validate_validation_tag_coverage, AttemptAggregate, AttemptScoreInput, CaseAggregate,
    SplitAggregate, TagAggregate,
};

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::case::{EvalCase, GraderSpec};
use super::trajectory::TrajectoryEvent;
use super::AttemptStatus;

/// Known deterministic grader identifiers.
pub const KNOWN_GRADER_IDS: [&str; 7] = [
    "tool_selection",
    "tool_chaining",
    "modality_coverage",
    "conflict_reconciliation",
    "report_structure",
    "citation_quality",
    "followup_grounding",
];

/// Unified grader output contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraderResult {
    pub grader_id: String,
    pub case_id: String,
    pub passed: bool,
    /// Score on a 0–100 scale.
    pub score: f64,
    /// Positive weight contributed to attempt aggregation.
    pub weight: f64,
    pub evidence: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

/// Errors from grading or aggregation contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraderError {
    pub message: String,
    pub grader_id: Option<String>,
    pub case_id: Option<String>,
}

impl GraderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            grader_id: None,
            case_id: None,
        }
    }

    pub fn with_grader(mut self, grader_id: impl Into<String>) -> Self {
        self.grader_id = Some(grader_id.into());
        self
    }

    pub fn with_case(mut self, case_id: impl Into<String>) -> Self {
        self.case_id = Some(case_id.into());
        self
    }
}

impl std::fmt::Display for GraderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(case) = &self.case_id {
            write!(f, " (case={case})")?;
        }
        if let Some(grader) = &self.grader_id {
            write!(f, " (grader={grader})")?;
        }
        Ok(())
    }
}

impl std::error::Error for GraderError {}

/// Input visible to a single grader invocation.
#[derive(Debug, Clone)]
pub struct GraderInput<'a> {
    pub case: &'a EvalCase,
    pub trajectory: &'a [TrajectoryEvent],
    pub output_md: &'a str,
    pub fixtures_dir: &'a Path,
    pub fixture_inventory: &'a BTreeSet<String>,
    /// Whether the attempt completed with a terminal run result.
    pub attempt_status: AttemptStatus,
    /// Follow-up seed id actually used for the attempt (from attempt.json).
    pub session_seed_id: Option<&'a str>,
    /// Follow-up seed content hash actually used for the attempt.
    pub session_seed_hash: Option<&'a str>,
}

/// Result of grading one attempt (all configured graders).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptGrade {
    pub case_id: String,
    pub grader_status: AttemptGraderStatus,
    pub graders: Vec<GraderResult>,
    pub aggregate: Option<AttemptAggregate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Grading lifecycle status written into scores.json.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptGraderStatus {
    Completed,
    Inconclusive,
    NotRun,
    Error,
}

impl AttemptGraderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Inconclusive => "inconclusive",
            Self::NotRun => "not_run",
            Self::Error => "error",
        }
    }
}

/// Run every grader listed on the case and aggregate.
pub fn grade_attempt(input: &GraderInput<'_>) -> AttemptGrade {
    let case_id = input.case.case_id.clone();
    if input.case.graders.is_empty() {
        return AttemptGrade {
            case_id,
            grader_status: AttemptGraderStatus::Error,
            graders: vec![],
            aggregate: None,
            error: Some("case has no graders configured".into()),
        };
    }

    let mut results = Vec::with_capacity(input.case.graders.len());
    for spec in &input.case.graders {
        match run_grader(spec, input) {
            Ok(result) => results.push(result),
            Err(err) => {
                return AttemptGrade {
                    case_id,
                    grader_status: AttemptGraderStatus::Error,
                    graders: results,
                    aggregate: None,
                    error: Some(err.to_string()),
                };
            }
        }
    }

    match aggregate_attempt(&case_id, &results, &input.case.graders) {
        Ok(agg) => AttemptGrade {
            case_id,
            grader_status: AttemptGraderStatus::Completed,
            graders: results,
            aggregate: Some(agg),
            error: None,
        },
        Err(err) => AttemptGrade {
            case_id,
            grader_status: AttemptGraderStatus::Inconclusive,
            graders: results,
            aggregate: None,
            error: Some(err.to_string()),
        },
    }
}

/// Dispatch one grader by id.
pub fn run_grader(spec: &GraderSpec, input: &GraderInput<'_>) -> Result<GraderResult, GraderError> {
    validate_spec(spec, &input.case.case_id)?;

    let mut result = match spec.grader_id.as_str() {
        "tool_selection" => tool_flow::grade_tool_selection(input, spec.weight)?,
        "tool_chaining" => tool_flow::grade_tool_chaining(input, spec.weight)?,
        "followup_grounding" => tool_flow::grade_followup_grounding(input, spec.weight)?,
        "modality_coverage" => content::grade_modality_coverage(input, spec.weight)?,
        "conflict_reconciliation" => content::grade_conflict_reconciliation(input, spec.weight)?,
        "report_structure" => content::grade_report_structure(input, spec.weight)?,
        "citation_quality" => content::grade_citation_quality(input, spec.weight)?,
        other => {
            return Err(GraderError::new(format!("unknown grader_id '{other}'"))
                .with_grader(other)
                .with_case(input.case.case_id.clone()));
        }
    };

    result.weight = spec.weight;
    validate_result(&result)?;
    Ok(result)
}

fn validate_spec(spec: &GraderSpec, case_id: &str) -> Result<(), GraderError> {
    if spec.grader_id.trim().is_empty() {
        return Err(GraderError::new("grader_id must be non-empty").with_case(case_id));
    }
    if !(spec.weight.is_finite() && spec.weight > 0.0) {
        return Err(GraderError::new("grader weight must be positive")
            .with_grader(spec.grader_id.clone())
            .with_case(case_id));
    }
    Ok(())
}

fn validate_result(result: &GraderResult) -> Result<(), GraderError> {
    if !(result.score.is_finite() && (0.0..=100.0).contains(&result.score)) {
        return Err(GraderError::new(format!(
            "grader score out of range: {}",
            result.score
        ))
        .with_grader(result.grader_id.clone())
        .with_case(result.case_id.clone()));
    }
    if !(result.weight.is_finite() && result.weight > 0.0) {
        return Err(GraderError::new(format!(
            "grader weight must be positive: {}",
            result.weight
        ))
        .with_grader(result.grader_id.clone())
        .with_case(result.case_id.clone()));
    }
    Ok(())
}

/// Helper used by graders to emit a result.
pub(crate) fn make_result(
    grader_id: &str,
    case_id: &str,
    passed: bool,
    score: f64,
    weight: f64,
    evidence: Value,
    failure_reason: Option<String>,
) -> GraderResult {
    let score = score.clamp(0.0, 100.0);
    GraderResult {
        grader_id: grader_id.into(),
        case_id: case_id.into(),
        passed,
        score,
        weight,
        evidence,
        failure_reason,
    }
}

/// Collect ordered tool names from trajectory tool lifecycle events.
#[derive(Debug, Clone, Default)]
pub(crate) struct ToolTrace {
    /// Ordered tool names from `tool_call_started` (includes retries as new starts).
    pub started_sequence: Vec<String>,
    /// First successful completion sequence index per tool.
    pub first_completed_seq: std::collections::BTreeMap<String, u64>,
    /// First start sequence index per tool.
    pub first_started_seq: std::collections::BTreeMap<String, u64>,
    /// Tools that completed at least once.
    pub completed: BTreeSet<String>,
    /// Tools that failed at least once.
    pub failed: BTreeSet<String>,
    /// Tools that retried at least once.
    pub retried: BTreeSet<String>,
    /// Distinct tools observed in any lifecycle event.
    pub observed: BTreeSet<String>,
}

pub(crate) fn extract_tool_trace(events: &[TrajectoryEvent]) -> ToolTrace {
    let mut trace = ToolTrace::default();
    for ev in events {
        let tool = ev
            .data
            .get("tool")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                ev.data
                    .get("tool_call")
                    .and_then(|v| v.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });

        match ev.kind.as_str() {
            "tool_call_started" => {
                if let Some(tool) = tool {
                    trace.started_sequence.push(tool.clone());
                    trace
                        .first_started_seq
                        .entry(tool.clone())
                        .or_insert(ev.sequence);
                    trace.observed.insert(tool);
                }
            }
            "tool_call_completed" => {
                if let Some(tool) = tool {
                    trace
                        .first_completed_seq
                        .entry(tool.clone())
                        .or_insert(ev.sequence);
                    trace.completed.insert(tool.clone());
                    trace.observed.insert(tool);
                }
            }
            "tool_call_failed" => {
                if let Some(tool) = tool {
                    trace.failed.insert(tool.clone());
                    trace.observed.insert(tool);
                }
            }
            "tool_call_retry" => {
                if let Some(tool) = tool {
                    trace.retried.insert(tool.clone());
                    trace.observed.insert(tool);
                }
            }
            "tool_call_batch_item_started" | "tool_call_batch_item_completed" => {
                if let Some(tool) = tool {
                    if ev.kind == "tool_call_batch_item_started" {
                        trace.started_sequence.push(tool.clone());
                        trace
                            .first_started_seq
                            .entry(tool.clone())
                            .or_insert(ev.sequence);
                    } else {
                        trace
                            .first_completed_seq
                            .entry(tool.clone())
                            .or_insert(ev.sequence);
                        trace.completed.insert(tool.clone());
                    }
                    trace.observed.insert(tool);
                }
            }
            _ => {}
        }
    }
    trace
}

/// A tool counts as "selected" if it was started, completed, failed, or retried.
pub(crate) fn tool_was_selected(trace: &ToolTrace, tool: &str) -> bool {
    trace.observed.contains(tool)
}

/// Case-insensitive section heading match for markdown reports.
pub(crate) fn output_has_section(output: &str, section: &str) -> bool {
    let needle = section.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    for line in output.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') {
            let plain = trimmed
                .trim_matches(|c: char| c == '*' || c == '_' || c == ':')
                .trim()
                .to_lowercase();
            if plain == needle || plain.starts_with(&format!("{needle} ")) {
                return true;
            }
            continue;
        }
        let heading = trimmed
            .trim_start_matches('#')
            .trim()
            .trim_matches(|c: char| c == '*' || c == '_' || c == ':')
            .to_lowercase();
        if heading == needle || heading.starts_with(&format!("{needle} ")) {
            return true;
        }
    }
    false
}

/// Lowercase haystack contains all required needles (case-insensitive).
pub(crate) fn contains_all(haystack: &str, needles: &[String]) -> bool {
    let lower = haystack.to_lowercase();
    needles.iter().all(|n| lower.contains(&n.to_lowercase()))
}

/// Extract formal citation candidates (backticks / brackets) from report text.
pub(crate) fn extract_citation_candidates(output: &str) -> Vec<String> {
    let mut found = BTreeSet::new();
    for part in output.split('`') {
        let candidate = part.trim();
        if looks_like_fixture_ref(candidate) {
            if let Some(base) = basename_of(candidate) {
                found.insert(base);
            }
        }
    }
    let mut rest = output;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find(']') {
            let inner = &after[..end];
            for token in inner.split([',', ';', '|']) {
                let token = token.trim();
                if looks_like_fixture_ref(token) {
                    if let Some(base) = basename_of(token) {
                        found.insert(base);
                    }
                }
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    found.into_iter().collect()
}

/// Bare fixture-looking tokens in free prose (not formal citations).
pub(crate) fn extract_bare_fixture_mentions(output: &str) -> Vec<String> {
    let mut found = BTreeSet::new();
    for word in output.split_whitespace() {
        let cleaned = word.trim_matches(|c: char| {
            matches!(c, '(' | ')' | '"' | '\'' | ',' | '.' | ';' | ':' | '!' | '?')
        });
        if looks_like_fixture_ref(cleaned) {
            if let Some(base) = basename_of(cleaned) {
                found.insert(base);
            }
        }
    }
    found.into_iter().collect()
}

fn looks_like_fixture_ref(s: &str) -> bool {
    if s.is_empty() || s.len() > 256 {
        return false;
    }
    let lower = s.to_lowercase();
    lower.ends_with(".md")
        || lower.ends_with(".wav")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".json")
        || lower.contains('/')
        || lower.contains('\\')
}

pub(crate) fn basename_of(path_like: &str) -> Option<String> {
    let p = Path::new(path_like);
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

/// Canonicalize `path` and require it to stay under `fixtures_dir`.
pub(crate) fn resolve_under_fixtures(
    fixtures_dir: &Path,
    path_like: &str,
) -> Result<PathBuf, String> {
    let candidate = if Path::new(path_like).is_absolute() {
        PathBuf::from(path_like)
    } else {
        fixtures_dir.join(path_like)
    };
    let fixtures_canon = fixtures_dir.canonicalize().map_err(|e| {
        format!(
            "cannot canonicalize fixtures dir {}: {e}",
            fixtures_dir.display()
        )
    })?;

    if candidate.exists() {
        let canon = candidate
            .canonicalize()
            .map_err(|e| format!("cannot canonicalize citation '{path_like}': {e}"))?;
        if !path_is_under(&canon, &fixtures_canon) {
            return Err(format!(
                "citation '{path_like}' resolves outside fixture root"
            ));
        }
        return Ok(canon);
    }

    let lexical = if Path::new(path_like).is_absolute() {
        normalize_path(Path::new(path_like))
    } else {
        normalize_path(&fixtures_dir.join(path_like))
    };
    if path_like.contains("..") && !path_is_under(&lexical, fixtures_dir) {
        return Err(format!(
            "citation '{path_like}' escapes fixture root via '..'"
        ));
    }
    Err(format!("citation target does not exist: {path_like}"))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn path_is_under(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

/// Default fixtures inventory from a directory listing (basenames only).
pub fn list_fixture_inventory(fixtures_dir: &Path) -> Result<BTreeSet<String>, GraderError> {
    let mut out = BTreeSet::new();
    let rd = std::fs::read_dir(fixtures_dir).map_err(|e| {
        GraderError::new(format!(
            "reading fixtures dir {}: {e}",
            fixtures_dir.display()
        ))
    })?;
    for ent in rd {
        let ent = ent.map_err(|e| GraderError::new(format!("reading fixture entry: {e}")))?;
        let path = ent.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                out.insert(name.to_string());
            }
        }
    }
    Ok(out)
}

/// Build a minimal trajectory event for fixtures/tests.
pub fn test_event(sequence: u64, kind: &str, data: Value) -> TrajectoryEvent {
    TrajectoryEvent {
        schema_version: super::trajectory::TRAJECTORY_SCHEMA_VERSION.into(),
        sequence,
        elapsed_ms: sequence.saturating_mul(10),
        run_relation: super::trajectory::RunRelation::default(),
        kind: kind.into(),
        data,
    }
}

pub fn tool_started(seq: u64, tool: &str) -> TrajectoryEvent {
    test_event(
        seq,
        "tool_call_started",
        json!({ "tool": tool, "input": {} }),
    )
}

pub fn tool_completed(seq: u64, tool: &str) -> TrajectoryEvent {
    test_event(
        seq,
        "tool_call_completed",
        json!({ "tool": tool, "output": {"ok": true}, "duration_ms": 1 }),
    )
}

pub fn tool_failed(seq: u64, tool: &str) -> TrajectoryEvent {
    test_event(
        seq,
        "tool_call_failed",
        json!({ "tool": tool, "error": {"message": "boom"} }),
    )
}

pub fn tool_retry(seq: u64, tool: &str, attempt: u32) -> TrajectoryEvent {
    test_event(
        seq,
        "tool_call_retry",
        json!({ "tool": tool, "attempt": attempt }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::case::{
        default_cases_path, default_fixtures_dir, default_seeds_dir, load_corpus, BehaviorTag,
        EvalSplit, ExpectedConflict, ExpectedFact, OrderConstraint, ReportExpectations, RunMode,
        ToolConstraints,
    };
    use crate::eval::AttemptStatus;
    use std::collections::BTreeSet;

    fn fixtures_dir() -> PathBuf {
        default_fixtures_dir()
    }

    fn inventory() -> BTreeSet<String> {
        list_fixture_inventory(&fixtures_dir()).expect("inventory")
    }

    fn base_case(id: &str) -> EvalCase {
        EvalCase {
            case_id: id.into(),
            scenario_family: "test".into(),
            tags: vec![BehaviorTag::ToolSelection],
            split: EvalSplit::Optimization,
            run: RunMode::Fresh {
                question: "q".into(),
            },
            must_pass: false,
            weight: 1.0,
            tools: ToolConstraints::default(),
            order: vec![],
            expected_facts: vec![],
            expected_conflict: None,
            report: ReportExpectations::default(),
            fixture_refs: vec![],
            graders: vec![],
            notes: None,
        }
    }

    fn input_for<'a>(
        case: &'a EvalCase,
        trajectory: &'a [TrajectoryEvent],
        output: &'a str,
        inventory: &'a BTreeSet<String>,
        fixtures: &'a Path,
    ) -> GraderInput<'a> {
        GraderInput {
            case,
            trajectory,
            output_md: output,
            fixtures_dir: fixtures,
            fixture_inventory: inventory,
            attempt_status: AttemptStatus::Completed,
            session_seed_id: None,
            session_seed_hash: None,
        }
    }

    #[test]
    fn tool_selection_pass_and_forbidden_fail() {
        let inv = inventory();
        let fixtures = fixtures_dir();
        let mut case = base_case("sel");
        case.tools = ToolConstraints {
            required: vec!["search_fixtures".into(), "read_fixture".into()],
            forbidden: vec!["synthesize_brief".into()],
            optional: vec!["review_report".into()],
        };
        case.graders = vec![GraderSpec {
            grader_id: "tool_selection".into(),
            weight: 1.0,
            required: true,
        }];

        let pass_traj = vec![
            tool_started(0, "search_fixtures"),
            tool_completed(1, "search_fixtures"),
            tool_started(2, "read_fixture"),
            tool_completed(3, "read_fixture"),
        ];
        let pass = run_grader(&case.graders[0], &input_for(&case, &pass_traj, "", &inv, &fixtures)).unwrap();
        assert!(pass.passed, "{pass:?}");
        assert_eq!(pass.score, 100.0);

        let fail_traj = vec![
            tool_started(0, "search_fixtures"),
            tool_completed(1, "search_fixtures"),
            tool_started(2, "synthesize_brief"),
            tool_completed(3, "synthesize_brief"),
        ];
        let fail = run_grader(&case.graders[0], &input_for(&case, &fail_traj, "", &inv, &fixtures)).unwrap();
        assert!(!fail.passed);
        assert!(fail
            .failure_reason
            .as_ref()
            .unwrap()
            .contains("synthesize_brief"));
        assert!(fail.evidence.get("actual_call_sequence").is_some());
    }

    #[test]
    fn tool_chaining_partial_order_and_retry_success() {
        let inv = inventory();
        let fixtures = fixtures_dir();
        let mut case = base_case("chain");
        case.order = vec![
            OrderConstraint {
                before: "search_fixtures".into(),
                after: "read_fixture".into(),
            },
            OrderConstraint {
                before: "read_fixture".into(),
                after: "write_report".into(),
            },
        ];
        case.graders = vec![GraderSpec {
            grader_id: "tool_chaining".into(),
            weight: 1.0,
            required: true,
        }];

        let pass_traj = vec![
            tool_started(0, "search_fixtures"),
            tool_completed(1, "search_fixtures"),
            tool_started(2, "describe_image"),
            tool_completed(3, "describe_image"),
            tool_started(4, "read_fixture"),
            tool_completed(5, "read_fixture"),
            tool_started(6, "write_report"),
            tool_completed(7, "write_report"),
        ];
        let pass = run_grader(&case.graders[0], &input_for(&case, &pass_traj, "", &inv, &fixtures)).unwrap();
        assert!(pass.passed);

        let early_write = vec![
            tool_started(0, "search_fixtures"),
            tool_completed(1, "search_fixtures"),
            tool_started(2, "write_report"),
            tool_completed(3, "write_report"),
            tool_started(4, "read_fixture"),
            tool_completed(5, "read_fixture"),
        ];
        let fail = run_grader(
            &case.graders[0],
            &input_for(&case, &early_write, "", &inv, &fixtures),
        )
        .unwrap();
        assert!(!fail.passed);

        let retry_traj = vec![
            tool_started(0, "search_fixtures"),
            tool_failed(1, "search_fixtures"),
            tool_retry(2, "search_fixtures", 1),
            tool_started(3, "search_fixtures"),
            tool_completed(4, "search_fixtures"),
            tool_started(5, "read_fixture"),
            tool_completed(6, "read_fixture"),
            tool_started(7, "write_report"),
            tool_completed(8, "write_report"),
        ];
        let retry_ok =
            run_grader(&case.graders[0], &input_for(&case, &retry_traj, "", &inv, &fixtures)).unwrap();
        assert!(retry_ok.passed, "{retry_ok:?}");
    }

    #[test]
    fn modality_and_conflict_graders() {
        let inv = inventory();
        let fixtures = fixtures_dir();
        let mut case = base_case("mod");
        case.tools.required = vec![
            "transcribe_audio".into(),
            "describe_image".into(),
            "read_fixture".into(),
        ];
        case.expected_conflict = Some(ExpectedConflict {
            left_value: "42%".into(),
            left_source: "001-retention-dashboard-notes.md".into(),
            right_value: "35%".into(),
            right_source: "002-support-ticket-summary.md".into(),
        });
        case.graders = vec![
            GraderSpec {
                grader_id: "modality_coverage".into(),
                weight: 1.0,
                required: true,
            },
            GraderSpec {
                grader_id: "conflict_reconciliation".into(),
                weight: 1.0,
                required: true,
            },
        ];

        let traj = vec![
            tool_started(0, "read_fixture"),
            tool_completed(1, "read_fixture"),
            tool_started(2, "transcribe_audio"),
            tool_completed(3, "transcribe_audio"),
            tool_started(4, "describe_image"),
            tool_completed(5, "describe_image"),
        ];
        let out = r#"
# Conflicts
Finance cut shows 42% from `001-retention-dashboard-notes.md`.
Support pull shows 35% from `002-support-ticket-summary.md`.
# Sources
- 001-retention-dashboard-notes.md
- 002-support-ticket-summary.md
"#;
        let mod_r = run_grader(&case.graders[0], &input_for(&case, &traj, out, &inv, &fixtures)).unwrap();
        assert!(mod_r.passed, "{mod_r:?}");
        let conf_r = run_grader(&case.graders[1], &input_for(&case, &traj, out, &inv, &fixtures)).unwrap();
        assert!(conf_r.passed, "{conf_r:?}");

        let bare = "Numbers are 42% and 35% but no sources.";
        let conf_fail =
            run_grader(&case.graders[1], &input_for(&case, &traj, bare, &inv, &fixtures)).unwrap();
        assert!(!conf_fail.passed);
    }

    #[test]
    fn report_and_citation_graders() {
        let inv = inventory();
        let fixtures = fixtures_dir();
        let mut case = base_case("rep");
        case.report.required_sections = vec![
            "executive summary".into(),
            "recommendation".into(),
            "sources".into(),
        ];
        case.expected_facts = vec![ExpectedFact {
            id: "referral".into(),
            must_contain: vec!["42%".into()],
            source_fixture: Some("001-retention-dashboard-notes.md".into()),
        }];
        case.fixture_refs = vec!["001-retention-dashboard-notes.md".into()];
        case.graders = vec![
            GraderSpec {
                grader_id: "report_structure".into(),
                weight: 1.0,
                required: true,
            },
            GraderSpec {
                grader_id: "citation_quality".into(),
                weight: 1.0,
                required: true,
            },
        ];

        let good = r#"
# Executive Summary
Referral is up.
# Recommendation
Invest carefully.
# Sources
- `001-retention-dashboard-notes.md` supports the 42% claim.
"#;
        let rep = run_grader(&case.graders[0], &input_for(&case, &[], good, &inv, &fixtures)).unwrap();
        assert!(rep.passed, "{rep:?}");
        let cit = run_grader(&case.graders[1], &input_for(&case, &[], good, &inv, &fixtures)).unwrap();
        assert!(cit.passed, "{cit:?}");

        let missing_section = "# Sources\n- 001-retention-dashboard-notes.md\n42%\n";
        let rep_fail = run_grader(
            &case.graders[0],
            &input_for(&case, &[], missing_section, &inv, &fixtures),
        )
        .unwrap();
        assert!(!rep_fail.passed);

        let bad_cite = r#"
# Sources
- `does-not-exist.md`
- `../secrets.env`
"#;
        let cit_fail =
            run_grader(&case.graders[1], &input_for(&case, &[], bad_cite, &inv, &fixtures)).unwrap();
        assert!(!cit_fail.passed);
        let reason = cit_fail.failure_reason.unwrap_or_default();
        assert!(
            reason.contains("does-not-exist")
                || reason.contains("..")
                || reason.contains("outside")
                || reason.contains("inventory"),
            "{reason}"
        );

        let body_only = r#"
# Executive Summary
See 001-retention-dashboard-notes.md for 42%.
# Recommendation
Go.
# Key findings
details
"#;
        let mut case2 = case.clone();
        case2.report.required_sections = vec![];
        let body_fail =
            run_grader(&case2.graders[1], &input_for(&case2, &[], body_only, &inv, &fixtures)).unwrap();
        assert!(!body_fail.passed, "{body_fail:?}");
    }

    #[test]
    fn followup_grounding_seed_and_forbidden_tools() {
        let inv = inventory();
        let fixtures = fixtures_dir();
        let mut case = base_case("fu");
        case.run = RunMode::FollowUp {
            question: "which number?".into(),
            session_seed_id: "seed-followup-retention".into(),
            session_seed_hash: "abc".into(),
        };
        case.tools.forbidden = vec![
            "search_fixtures".into(),
            "read_fixture".into(),
            "transcribe_audio".into(),
            "describe_image".into(),
        ];
        case.expected_facts = vec![ExpectedFact {
            id: "both".into(),
            must_contain: vec!["42%".into(), "35%".into()],
            source_fixture: None,
        }];
        case.graders = vec![GraderSpec {
            grader_id: "followup_grounding".into(),
            weight: 1.0,
            required: true,
        }];
        case.tags = vec![BehaviorTag::FollowupGrounding];

        let out = "Trust finance 42% over support 35%.";
        let mut input = input_for(&case, &[], out, &inv, &fixtures);
        input.session_seed_id = Some("seed-followup-retention");
        input.session_seed_hash = Some("abc");
        let pass = run_grader(&case.graders[0], &input).unwrap();
        assert!(pass.passed, "{pass:?}");

        let traj = vec![
            tool_started(0, "search_fixtures"),
            tool_completed(1, "search_fixtures"),
        ];
        let mut input2 = input_for(&case, &traj, out, &inv, &fixtures);
        input2.session_seed_id = Some("seed-followup-retention");
        input2.session_seed_hash = Some("abc");
        let fail_tools = run_grader(&case.graders[0], &input2).unwrap();
        assert!(!fail_tools.passed);

        let input3 = input_for(&case, &[], out, &inv, &fixtures);
        let fail_seed = run_grader(&case.graders[0], &input3).unwrap();
        assert!(!fail_seed.passed);

        case.run = RunMode::Fresh {
            question: "q".into(),
        };
        let fail_fresh = run_grader(&case.graders[0], &input_for(&case, &[], out, &inv, &fixtures)).unwrap();
        assert!(!fail_fresh.passed);
    }

    #[test]
    fn unknown_grader_and_bad_weight_are_errors() {
        let inv = inventory();
        let fixtures = fixtures_dir();
        let case = base_case("bad");
        let err = run_grader(
            &GraderSpec {
                grader_id: "llm_judge".into(),
                weight: 1.0,
                required: true,
            },
            &input_for(&case, &[], "", &inv, &fixtures),
        )
        .unwrap_err();
        assert!(err.message.contains("unknown"));

        let err2 = run_grader(
            &GraderSpec {
                grader_id: "tool_selection".into(),
                weight: 0.0,
                required: true,
            },
            &input_for(&case, &[], "", &inv, &fixtures),
        )
        .unwrap_err();
        assert!(err2.message.contains("positive"));
    }

    #[test]
    fn corpus_grader_contracts_are_known_and_cover_all_tags() {
        let corpus = load_corpus(
            &default_cases_path(),
            &default_fixtures_dir(),
            &default_seeds_dir(),
        )
        .expect("corpus loads");

        let mut tags_with_grader: BTreeSet<BehaviorTag> = BTreeSet::new();
        for case in &corpus.cases {
            assert!(
                !case.graders.is_empty(),
                "case {} has no graders",
                case.case_id
            );
            for g in &case.graders {
                assert!(
                    KNOWN_GRADER_IDS.contains(&g.grader_id.as_str()),
                    "case {} unknown grader {}",
                    case.case_id,
                    g.grader_id
                );
                assert!(g.weight > 0.0);
            }
            for tag in &case.tags {
                let tag_id = tag.as_str();
                assert!(
                    case.graders.iter().any(|g| g.grader_id == tag_id),
                    "case {} tag {} lacks matching grader",
                    case.case_id,
                    tag_id
                );
                tags_with_grader.insert(*tag);
            }
        }
        for tag in crate::eval::case::GATING_TAGS {
            assert!(
                tags_with_grader.contains(&tag),
                "no case grades tag {}",
                tag.as_str()
            );
        }
    }

    #[test]
    fn fixture_files_cover_pass_fail_boundaries() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evals/test-fixtures");
        assert!(root.join("trajectories/tool_selection_pass.jsonl").exists());
        assert!(root.join("trajectories/tool_selection_fail.jsonl").exists());
        assert!(root.join("outputs/report_pass.md").exists());
        assert!(root.join("outputs/citation_escape_fail.md").exists());
        assert!(root.join("outputs/conflict_pass.md").exists());
        assert!(root.join("outputs/conflict_bare_numbers_fail.md").exists());
        assert!(root.join("outputs/followup_pass.md").exists());
    }
}
