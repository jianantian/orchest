//! Attempt / case / tag / split aggregation formulas (PRD-fixed).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{GraderError, GraderResult};
use crate::eval::case::{BehaviorTag, EvalCase, EvalSplit, GraderSpec, GATING_TAGS};

/// Aggregate over graders for one completed attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptAggregate {
    pub case_id: String,
    pub passed: bool,
    /// Weighted mean of grader scores: Σ(score × weight) / Σ(weight).
    pub score: f64,
    pub total_weight: f64,
    pub required_passed: bool,
    pub grader_count: usize,
}

/// One attempt's contribution to case aggregation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptScoreInput {
    /// Whether grading finished without error and produced an aggregate.
    pub grading_completed: bool,
    /// Whether the attempt itself completed (run terminal ok).
    pub attempt_completed: bool,
    pub aggregate: Option<AttemptAggregate>,
}

/// Aggregate over repetitions of one case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseAggregate {
    pub case_id: String,
    pub must_pass: bool,
    pub case_weight: f64,
    pub tags: Vec<BehaviorTag>,
    pub split: EvalSplit,
    pub passed: bool,
    /// Arithmetic mean of attempt scores (when complete).
    pub score: f64,
    pub attempts_total: usize,
    pub attempts_passed: usize,
    pub attempts_present: usize,
}

/// Weighted aggregate for one behavior tag (typically validation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagAggregate {
    pub tag: BehaviorTag,
    pub passed_cases: usize,
    pub case_count: usize,
    pub score: f64,
    pub total_weight: f64,
}

/// Weighted aggregate for a split (overall).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitAggregate {
    pub split: EvalSplit,
    pub passed_cases: usize,
    pub case_count: usize,
    pub score: f64,
    pub total_weight: f64,
    pub per_tag: BTreeMap<String, TagAggregate>,
}

/// Aggregate grader results for one attempt.
///
/// - score = Σ(score × weight) / Σ(weight)
/// - passed only if every *required* grader has `passed=true`
/// - returns error (→ null aggregate upstream) on empty results, weight issues,
///   or missing required grader outputs
pub fn aggregate_attempt(
    case_id: &str,
    results: &[GraderResult],
    specs: &[GraderSpec],
) -> Result<AttemptAggregate, GraderError> {
    if results.is_empty() {
        return Err(GraderError::new("no grader results to aggregate").with_case(case_id));
    }

    for spec in specs {
        if !results.iter().any(|r| r.grader_id == spec.grader_id) {
            return Err(GraderError::new(format!(
                "missing result for required grader '{}'",
                spec.grader_id
            ))
            .with_case(case_id)
            .with_grader(spec.grader_id.clone()));
        }
    }

    let mut total_weight = 0.0;
    let mut weighted_sum = 0.0;
    for r in results {
        if !(r.weight.is_finite() && r.weight > 0.0) {
            return Err(GraderError::new(format!(
                "invalid weight {} on grader {}",
                r.weight, r.grader_id
            ))
            .with_case(case_id)
            .with_grader(r.grader_id.clone()));
        }
        if !(r.score.is_finite() && (0.0..=100.0).contains(&r.score)) {
            return Err(GraderError::new(format!(
                "invalid score {} on grader {}",
                r.score, r.grader_id
            ))
            .with_case(case_id)
            .with_grader(r.grader_id.clone()));
        }
        total_weight += r.weight;
        weighted_sum += r.score * r.weight;
    }

    if !(total_weight.is_finite() && total_weight > 0.0) {
        return Err(GraderError::new("total grader weight must be positive").with_case(case_id));
    }

    let score = weighted_sum / total_weight;

    let required_ids: BTreeSet<&str> = specs
        .iter()
        .filter(|s| s.required)
        .map(|s| s.grader_id.as_str())
        .collect();

    let required_passed = if required_ids.is_empty() {
        results.iter().all(|r| r.passed)
    } else {
        results
            .iter()
            .filter(|r| required_ids.contains(r.grader_id.as_str()))
            .all(|r| r.passed)
    };

    Ok(AttemptAggregate {
        case_id: case_id.into(),
        passed: required_passed,
        score,
        total_weight,
        required_passed,
        grader_count: results.len(),
    })
}

/// Aggregate repeated attempts for one case.
///
/// - Normal: pass if ≥ 2/3 attempts pass; score = arithmetic mean of attempt scores.
/// - Must-pass: every attempt must pass (3/3 when repetitions=3).
/// - If any required attempt is missing / not completed / grading incomplete,
///   returns `None` (null aggregate — do not shrink the denominator).
pub fn aggregate_case(
    case: &EvalCase,
    attempts: &[AttemptScoreInput],
    expected_repetitions: usize,
) -> Result<Option<CaseAggregate>, GraderError> {
    if expected_repetitions == 0 {
        return Err(
            GraderError::new("expected_repetitions must be > 0").with_case(case.case_id.clone())
        );
    }
    if attempts.len() != expected_repetitions {
        return Ok(None);
    }

    for a in attempts {
        if !a.attempt_completed || !a.grading_completed || a.aggregate.is_none() {
            return Ok(None);
        }
    }

    let scores: Vec<f64> = attempts
        .iter()
        .map(|a| a.aggregate.as_ref().unwrap().score)
        .collect();
    let passed_flags: Vec<bool> = attempts
        .iter()
        .map(|a| a.aggregate.as_ref().unwrap().passed)
        .collect();

    let attempts_passed = passed_flags.iter().filter(|p| **p).count();
    let mean_score = scores.iter().sum::<f64>() / scores.len() as f64;

    let passed = if case.must_pass {
        attempts_passed == expected_repetitions
    } else {
        let need = majority_threshold(expected_repetitions);
        attempts_passed >= need
    };

    Ok(Some(CaseAggregate {
        case_id: case.case_id.clone(),
        must_pass: case.must_pass,
        case_weight: case.weight,
        tags: case.tags.clone(),
        split: case.split,
        passed,
        score: mean_score,
        attempts_total: expected_repetitions,
        attempts_passed,
        attempts_present: attempts.len(),
    }))
}

fn majority_threshold(n: usize) -> usize {
    (2 * n).div_ceil(3)
}

/// Case-weighted overall score for a split.
///
/// Returns `None` if any case aggregate is missing (null — incomplete).
pub fn aggregate_split(
    split: EvalSplit,
    case_aggregates: &[CaseAggregate],
) -> Result<Option<SplitAggregate>, GraderError> {
    if case_aggregates.is_empty() {
        return Ok(None);
    }

    let mut total_weight = 0.0;
    let mut weighted_sum = 0.0;
    let mut passed_cases = 0usize;

    for c in case_aggregates {
        if c.split != split {
            continue;
        }
        if !(c.case_weight.is_finite() && c.case_weight > 0.0) {
            return Err(GraderError::new(format!(
                "invalid case weight {} on {}",
                c.case_weight, c.case_id
            ))
            .with_case(c.case_id.clone()));
        }
        total_weight += c.case_weight;
        weighted_sum += c.score * c.case_weight;
        if c.passed {
            passed_cases += 1;
        }
    }

    let case_count = case_aggregates.iter().filter(|c| c.split == split).count();
    if case_count == 0 {
        return Ok(None);
    }
    if !(total_weight.is_finite() && total_weight > 0.0) {
        return Err(GraderError::new("split total case weight must be positive"));
    }

    let per_tag = aggregate_all_tags(case_aggregates, Some(split))?;

    Ok(Some(SplitAggregate {
        split,
        passed_cases,
        case_count,
        score: weighted_sum / total_weight,
        total_weight,
        per_tag,
    }))
}

/// Case-weighted score for one tag among the provided case aggregates.
pub fn aggregate_tag(
    tag: BehaviorTag,
    case_aggregates: &[CaseAggregate],
) -> Result<Option<TagAggregate>, GraderError> {
    let mut total_weight = 0.0;
    let mut weighted_sum = 0.0;
    let mut case_count = 0usize;
    let mut passed_cases = 0usize;

    for c in case_aggregates {
        if !c.tags.contains(&tag) {
            continue;
        }
        if !(c.case_weight.is_finite() && c.case_weight > 0.0) {
            return Err(GraderError::new(format!(
                "invalid case weight {} on {}",
                c.case_weight, c.case_id
            ))
            .with_case(c.case_id.clone()));
        }
        total_weight += c.case_weight;
        weighted_sum += c.score * c.case_weight;
        case_count += 1;
        if c.passed {
            passed_cases += 1;
        }
    }

    if case_count == 0 {
        return Ok(None);
    }
    if !(total_weight.is_finite() && total_weight > 0.0) {
        return Err(GraderError::new(format!(
            "tag '{}' total weight must be positive",
            tag.as_str()
        )));
    }

    Ok(Some(TagAggregate {
        tag,
        passed_cases,
        case_count,
        score: weighted_sum / total_weight,
        total_weight,
    }))
}

fn aggregate_all_tags(
    case_aggregates: &[CaseAggregate],
    split_filter: Option<EvalSplit>,
) -> Result<BTreeMap<String, TagAggregate>, GraderError> {
    let filtered: Vec<CaseAggregate> = match split_filter {
        Some(split) => case_aggregates
            .iter()
            .filter(|c| c.split == split)
            .cloned()
            .collect(),
        None => case_aggregates.to_vec(),
    };

    let mut out = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for c in &filtered {
        for t in &c.tags {
            seen.insert(*t);
        }
    }
    for tag in seen {
        if let Some(agg) = aggregate_tag(tag, &filtered)? {
            out.insert(tag.as_str().to_string(), agg);
        }
    }
    Ok(out)
}

/// Validation split must cover every gating tag; missing tag is a corpus error.
pub fn validate_validation_tag_coverage(
    case_aggregates: &[CaseAggregate],
) -> Result<(), GraderError> {
    let mut covered = BTreeSet::new();
    for c in case_aggregates {
        if c.split != EvalSplit::Validation {
            continue;
        }
        for t in &c.tags {
            covered.insert(*t);
        }
    }
    for tag in GATING_TAGS {
        if !covered.contains(&tag) {
            return Err(GraderError::new(format!(
                "validation tag '{}' has no cases; corpus is invalid for comparison",
                tag.as_str()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::case::{BehaviorTag, EvalSplit, GraderSpec};

    fn result(id: &str, case: &str, passed: bool, score: f64, weight: f64) -> GraderResult {
        GraderResult {
            grader_id: id.into(),
            case_id: case.into(),
            passed,
            score,
            weight,
            evidence: serde_json::json!({}),
            failure_reason: None,
        }
    }

    fn case(id: &str, must_pass: bool, weight: f64, tags: Vec<BehaviorTag>) -> EvalCase {
        EvalCase {
            case_id: id.into(),
            scenario_family: "t".into(),
            tags,
            split: EvalSplit::Validation,
            run: crate::eval::case::RunMode::Fresh {
                question: "q".into(),
            },
            must_pass,
            weight,
            tools: Default::default(),
            order: vec![],
            expected_facts: vec![],
            expected_conflict: None,
            report: Default::default(),
            fixture_refs: vec![],
            graders: vec![],
            notes: None,
        }
    }

    #[test]
    fn attempt_weighted_mean_and_required_gate() {
        let specs = vec![
            GraderSpec {
                grader_id: "a".into(),
                weight: 1.0,
                required: true,
            },
            GraderSpec {
                grader_id: "b".into(),
                weight: 3.0,
                required: true,
            },
        ];
        let results = vec![
            result("a", "c1", true, 100.0, 1.0),
            result("b", "c1", true, 80.0, 3.0),
        ];
        let agg = aggregate_attempt("c1", &results, &specs).unwrap();
        assert!(agg.passed);
        assert!((agg.score - 85.0).abs() < 1e-9);

        let results_fail = vec![
            result("a", "c1", true, 100.0, 1.0),
            result("b", "c1", false, 90.0, 3.0),
        ];
        let agg_fail = aggregate_attempt("c1", &results_fail, &specs).unwrap();
        assert!(!agg_fail.passed);
        assert!((agg_fail.score - 92.5).abs() < 1e-9);
    }

    #[test]
    fn attempt_null_on_missing_or_bad_weight() {
        let specs = vec![GraderSpec {
            grader_id: "a".into(),
            weight: 1.0,
            required: true,
        }];
        assert!(aggregate_attempt("c1", &[], &specs).is_err());
        let bad = vec![result("a", "c1", true, 50.0, 0.0)];
        assert!(aggregate_attempt("c1", &bad, &specs).is_err());
        let only_b = vec![result("b", "c1", true, 50.0, 1.0)];
        assert!(aggregate_attempt("c1", &only_b, &specs).is_err());
    }

    #[test]
    fn case_majority_vs_must_pass() {
        let normal = case("n", false, 1.0, vec![BehaviorTag::ToolSelection]);
        let must = case("m", true, 1.0, vec![BehaviorTag::ToolSelection]);

        let mk = |passed: bool, score: f64| AttemptScoreInput {
            grading_completed: true,
            attempt_completed: true,
            aggregate: Some(AttemptAggregate {
                case_id: "x".into(),
                passed,
                score,
                total_weight: 1.0,
                required_passed: passed,
                grader_count: 1,
            }),
        };

        let attempts = vec![mk(true, 90.0), mk(true, 80.0), mk(false, 70.0)];
        let n = aggregate_case(&normal, &attempts, 3).unwrap().unwrap();
        assert!(n.passed);
        assert!((n.score - 80.0).abs() < 1e-9);

        let m = aggregate_case(&must, &attempts, 3).unwrap().unwrap();
        assert!(!m.passed);
        assert!((m.score - 80.0).abs() < 1e-9);

        let attempts2 = vec![mk(true, 100.0), mk(false, 0.0), mk(false, 0.0)];
        let n2 = aggregate_case(&normal, &attempts2, 3).unwrap().unwrap();
        assert!(!n2.passed);

        let incomplete = vec![
            mk(true, 100.0),
            AttemptScoreInput {
                grading_completed: false,
                attempt_completed: true,
                aggregate: None,
            },
            mk(true, 100.0),
        ];
        assert!(aggregate_case(&normal, &incomplete, 3).unwrap().is_none());

        assert!(aggregate_case(&normal, &attempts[..2], 3)
            .unwrap()
            .is_none());
    }

    #[test]
    fn multi_tag_case_weights_and_missing_tag_error() {
        let c1 = CaseAggregate {
            case_id: "a".into(),
            must_pass: true,
            case_weight: 2.0,
            tags: vec![BehaviorTag::ToolSelection, BehaviorTag::ReportStructure],
            split: EvalSplit::Validation,
            passed: true,
            score: 100.0,
            attempts_total: 3,
            attempts_passed: 3,
            attempts_present: 3,
        };
        let c2 = CaseAggregate {
            case_id: "b".into(),
            must_pass: false,
            case_weight: 1.0,
            tags: vec![BehaviorTag::ToolSelection],
            split: EvalSplit::Validation,
            passed: true,
            score: 70.0,
            attempts_total: 3,
            attempts_passed: 2,
            attempts_present: 3,
        };

        let tag = aggregate_tag(BehaviorTag::ToolSelection, &[c1.clone(), c2.clone()])
            .unwrap()
            .unwrap();
        assert!((tag.score - 90.0).abs() < 1e-9);
        assert_eq!(tag.case_count, 2);

        let report = aggregate_tag(BehaviorTag::ReportStructure, &[c1.clone(), c2.clone()])
            .unwrap()
            .unwrap();
        assert!((report.score - 100.0).abs() < 1e-9);
        assert_eq!(report.case_count, 1);
        assert_eq!(report.total_weight, 2.0);

        let split = aggregate_split(EvalSplit::Validation, &[c1.clone(), c2.clone()])
            .unwrap()
            .unwrap();
        assert!((split.score - 90.0).abs() < 1e-9);

        let err = validate_validation_tag_coverage(&[c1, c2]).unwrap_err();
        assert!(err.message.contains("validation tag"));
    }

    #[test]
    fn must_pass_not_offset_by_other_high_scores_in_split() {
        let must_fail = CaseAggregate {
            case_id: "must".into(),
            must_pass: true,
            case_weight: 1.0,
            tags: vec![BehaviorTag::FollowupGrounding],
            split: EvalSplit::Validation,
            passed: false,
            score: 95.0,
            attempts_total: 3,
            attempts_passed: 2,
            attempts_present: 3,
        };
        let other = CaseAggregate {
            case_id: "other".into(),
            must_pass: false,
            case_weight: 10.0,
            tags: vec![BehaviorTag::ToolSelection],
            split: EvalSplit::Validation,
            passed: true,
            score: 100.0,
            attempts_total: 3,
            attempts_passed: 3,
            attempts_present: 3,
        };
        let split = aggregate_split(EvalSplit::Validation, &[must_fail.clone(), other])
            .unwrap()
            .unwrap();
        assert!(split.score > 99.0);
        assert!(!must_fail.passed);
        assert_eq!(split.passed_cases, 1);
    }
}
