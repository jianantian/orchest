//! Tool selection, chaining (partial order), and follow-up grounding graders.

use serde_json::json;

use super::{
    contains_all, extract_tool_trace, make_result, tool_was_selected, GraderError, GraderInput,
    GraderResult, ToolTrace,
};
use crate::eval::case::RunMode;

pub fn grade_tool_selection(
    input: &GraderInput<'_>,
    weight: f64,
) -> Result<GraderResult, GraderError> {
    let trace = extract_tool_trace(input.trajectory);
    let required = &input.case.tools.required;
    let forbidden = &input.case.tools.forbidden;
    let optional = &input.case.tools.optional;

    let mut missing_required = Vec::new();
    for tool in required {
        if !tool_was_selected(&trace, tool) {
            missing_required.push(tool.clone());
        }
    }
    let mut violated_forbidden = Vec::new();
    for tool in forbidden {
        if tool_was_selected(&trace, tool) {
            violated_forbidden.push(tool.clone());
        }
    }

    let total_checks = required.len() + forbidden.len();
    let failed_checks = missing_required.len() + violated_forbidden.len();
    let passed = failed_checks == 0;
    let score = if total_checks == 0 {
        100.0
    } else {
        let ok = total_checks - failed_checks;
        (ok as f64 / total_checks as f64) * 100.0
    };

    let evidence = json!({
        "required": required,
        "forbidden": forbidden,
        "optional": optional,
        "missing_required": missing_required,
        "violated_forbidden": violated_forbidden,
        "actual_call_sequence": trace.started_sequence,
        "completed": trace.completed.iter().cloned().collect::<Vec<_>>(),
        "failed": trace.failed.iter().cloned().collect::<Vec<_>>(),
        "retried": trace.retried.iter().cloned().collect::<Vec<_>>(),
    });

    let failure_reason = if passed {
        None
    } else {
        let mut parts = Vec::new();
        if !missing_required.is_empty() {
            parts.push(format!(
                "missing required tools: {}",
                missing_required.join(", ")
            ));
        }
        if !violated_forbidden.is_empty() {
            parts.push(format!(
                "forbidden tools used: {}",
                violated_forbidden.join(", ")
            ));
        }
        parts.push(format!(
            "actual call sequence: [{}]",
            trace.started_sequence.join(" -> ")
        ));
        Some(parts.join("; "))
    };

    Ok(make_result(
        "tool_selection",
        &input.case.case_id,
        passed,
        score,
        weight,
        evidence,
        failure_reason,
    ))
}

pub fn grade_tool_chaining(
    input: &GraderInput<'_>,
    weight: f64,
) -> Result<GraderResult, GraderError> {
    let trace = extract_tool_trace(input.trajectory);
    let edges = &input.case.order;

    let mut satisfied = Vec::new();
    let mut violated = Vec::new();
    let mut vacuous = Vec::new();

    for edge in edges {
        let before = edge.before.as_str();
        let after = edge.after.as_str();
        let before_done = trace.first_completed_seq.get(before).copied();
        let after_started = trace
            .first_started_seq
            .get(after)
            .copied()
            .or_else(|| trace.first_completed_seq.get(after).copied());

        match (before_done, after_started) {
            (None, None) => {
                vacuous
                    .push(json!({"before": before, "after": after, "status": "vacuous_neither"}));
            }
            (Some(_), None) => {
                vacuous.push(
                    json!({"before": before, "after": after, "status": "vacuous_after_absent"}),
                );
            }
            (None, Some(_)) => {
                violated.push(json!({
                    "before": before,
                    "after": after,
                    "status": "missing_before_completion",
                }));
            }
            (Some(b_seq), Some(a_seq)) => {
                if b_seq < a_seq {
                    satisfied.push(json!({
                        "before": before,
                        "after": after,
                        "before_completed_seq": b_seq,
                        "after_seq": a_seq,
                        "status": "ok",
                    }));
                } else {
                    violated.push(json!({
                        "before": before,
                        "after": after,
                        "before_completed_seq": b_seq,
                        "after_seq": a_seq,
                        "status": "order_violation",
                    }));
                }
            }
        }
    }

    let active = satisfied.len() + violated.len();
    let passed = violated.is_empty();
    let score = if edges.is_empty() || active == 0 {
        100.0
    } else {
        (satisfied.len() as f64 / active as f64) * 100.0
    };

    let evidence = json!({
        "edges": edges,
        "satisfied": satisfied,
        "violated": violated,
        "vacuous": vacuous,
        "actual_call_sequence": trace.started_sequence,
        "first_completed_seq": trace.first_completed_seq,
        "retried": trace.retried.iter().cloned().collect::<Vec<_>>(),
    });

    let failure_reason = if passed {
        None
    } else {
        Some(format!(
            "order constraints violated: {}",
            serde_json::to_string(&violated).unwrap_or_default()
        ))
    };

    Ok(make_result(
        "tool_chaining",
        &input.case.case_id,
        passed,
        score,
        weight,
        evidence,
        failure_reason,
    ))
}

pub fn grade_followup_grounding(
    input: &GraderInput<'_>,
    weight: f64,
) -> Result<GraderResult, GraderError> {
    let trace = extract_tool_trace(input.trajectory);

    let (expected_seed_id, expected_seed_hash) = match &input.case.run {
        RunMode::FollowUp {
            session_seed_id,
            session_seed_hash,
            ..
        } => (
            Some(session_seed_id.as_str()),
            Some(session_seed_hash.as_str()),
        ),
        RunMode::Fresh { .. } => (None, None),
    };

    let mut failures = Vec::new();

    if expected_seed_id.is_none() {
        failures.push("case run mode is fresh; follow-up grounding requires follow_up".into());
    }

    let seed_id_ok = match (expected_seed_id, input.session_seed_id) {
        (Some(exp), Some(got)) if exp == got => true,
        (Some(exp), Some(got)) => {
            failures.push(format!(
                "session_seed_id mismatch: expected '{exp}', got '{got}'"
            ));
            false
        }
        (Some(exp), None) => {
            failures.push(format!(
                "missing session_seed_id on attempt (expected '{exp}')"
            ));
            false
        }
        (None, _) => false,
    };

    let seed_hash_ok = match (expected_seed_hash, input.session_seed_hash) {
        (Some(exp), Some(got)) if exp == got => true,
        (Some(exp), Some(got)) => {
            failures.push(format!(
                "session_seed_hash mismatch: expected '{exp}', got '{got}'"
            ));
            false
        }
        (Some(exp), None) => {
            failures.push(format!(
                "missing session_seed_hash on attempt (expected '{exp}')"
            ));
            false
        }
        (None, _) => false,
    };

    let resume_evidence_ok = expected_seed_id.is_some()
        && input.trajectory.iter().any(|event| {
            event.kind == "followup_session_resumed"
                && event
                    .data
                    .get("session_seed_id")
                    .and_then(serde_json::Value::as_str)
                    == expected_seed_id
                && event
                    .data
                    .get("session_seed_hash")
                    .and_then(serde_json::Value::as_str)
                    == expected_seed_hash
        });
    if !resume_evidence_ok {
        failures.push("missing retained follow-up session resume evidence".into());
    }

    let mut forbidden_used = Vec::new();
    for tool in &input.case.tools.forbidden {
        if tool_was_selected(&trace, tool) {
            forbidden_used.push(tool.clone());
        }
    }
    if !forbidden_used.is_empty() {
        failures.push(format!(
            "re-executed forbidden research tools: {}",
            forbidden_used.join(", ")
        ));
    }

    let mut missing_facts = Vec::new();
    for fact in &input.case.expected_facts {
        if !contains_all(input.output_md, &fact.must_contain) {
            missing_facts.push(fact.id.clone());
        }
    }
    if !missing_facts.is_empty() {
        failures.push(format!(
            "missing historical grounding facts: {}",
            missing_facts.join(", ")
        ));
    }

    let checks = [
        seed_id_ok,
        seed_hash_ok,
        forbidden_used.is_empty(),
        missing_facts.is_empty(),
        expected_seed_id.is_some(),
    ];
    let ok_count = checks.iter().filter(|c| **c).count();
    let passed = failures.is_empty();
    let score = (ok_count as f64 / checks.len() as f64) * 100.0;

    let evidence = json!({
        "expected_seed_id": expected_seed_id,
        "expected_seed_hash": expected_seed_hash,
        "actual_seed_id": input.session_seed_id,
        "actual_seed_hash": input.session_seed_hash,
        "resume_evidence_observed": resume_evidence_ok,
        "forbidden_used": forbidden_used,
        "missing_facts": missing_facts,
        "actual_call_sequence": trace.started_sequence,
        "seed_resumed": seed_id_ok && seed_hash_ok && resume_evidence_ok,
    });

    Ok(make_result(
        "followup_grounding",
        &input.case.case_id,
        passed,
        score,
        weight,
        evidence,
        if passed {
            None
        } else {
            Some(failures.join("; "))
        },
    ))
}

#[allow(dead_code)]
pub(crate) fn trace_summary(trace: &ToolTrace) -> serde_json::Value {
    json!({
        "started_sequence": trace.started_sequence,
        "completed": trace.completed.iter().cloned().collect::<Vec<_>>(),
        "failed": trace.failed.iter().cloned().collect::<Vec<_>>(),
        "retried": trace.retried.iter().cloned().collect::<Vec<_>>(),
    })
}
