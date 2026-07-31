//! Modality, conflict, report structure, and citation graders.

use std::collections::BTreeSet;

use serde_json::json;

use super::{
    basename_of, contains_all, extract_bare_fixture_mentions, extract_citation_candidates,
    extract_tool_trace, make_result, output_has_section, resolve_under_fixtures, tool_was_selected,
    GraderError, GraderInput, GraderResult,
};

const AUDIO_TOOLS: &[&str] = &["transcribe_audio"];
const IMAGE_TOOLS: &[&str] = &["describe_image"];
const TEXT_TOOLS: &[&str] = &["read_fixture", "search_fixtures"];

pub fn grade_modality_coverage(
    input: &GraderInput<'_>,
    weight: f64,
) -> Result<GraderResult, GraderError> {
    let trace = extract_tool_trace(input.trajectory);

    let needs_audio = input
        .case
        .tools
        .required
        .iter()
        .any(|t| AUDIO_TOOLS.contains(&t.as_str()))
        || input
            .case
            .fixture_refs
            .iter()
            .any(|f| f.ends_with(".wav") || f.ends_with(".mp3"));
    let needs_image = input
        .case
        .tools
        .required
        .iter()
        .any(|t| IMAGE_TOOLS.contains(&t.as_str()))
        || input
            .case
            .fixture_refs
            .iter()
            .any(|f| f.ends_with(".png") || f.ends_with(".jpg") || f.ends_with(".jpeg"));
    let needs_text = input
        .case
        .tools
        .required
        .iter()
        .any(|t| TEXT_TOOLS.contains(&t.as_str()))
        || input
            .case
            .fixture_refs
            .iter()
            .any(|f| f.ends_with(".md") || f.ends_with(".json"))
        || input.case.expected_facts.iter().any(|f| {
            f.source_fixture
                .as_deref()
                .is_some_and(|s| s.ends_with(".md"))
        });

    let checks_needed = needs_audio || needs_image || needs_text;

    let audio_ok = !needs_audio || AUDIO_TOOLS.iter().any(|t| tool_was_selected(&trace, t));
    let image_ok = !needs_image || IMAGE_TOOLS.iter().any(|t| tool_was_selected(&trace, t));
    let text_ok = !needs_text || TEXT_TOOLS.iter().any(|t| tool_was_selected(&trace, t));

    let mut missing_fact_ids = Vec::new();
    for fact in &input.case.expected_facts {
        let src = fact.source_fixture.as_deref().unwrap_or("");
        let is_modality_fact = src.ends_with(".wav")
            || src.ends_with(".png")
            || src.ends_with(".jpg")
            || src.ends_with(".md")
            || src.is_empty();
        if is_modality_fact && !contains_all(input.output_md, &fact.must_contain) {
            missing_fact_ids.push(fact.id.clone());
        }
    }
    let facts_ok = missing_fact_ids.is_empty();

    let mut parts_ok = Vec::new();
    let mut parts_total = 0usize;
    if needs_audio {
        parts_total += 1;
        if audio_ok {
            parts_ok.push("audio");
        }
    }
    if needs_image {
        parts_total += 1;
        if image_ok {
            parts_ok.push("image");
        }
    }
    if needs_text {
        parts_total += 1;
        if text_ok {
            parts_ok.push("text");
        }
    }
    parts_total += 1;
    if facts_ok {
        parts_ok.push("facts");
    }

    let passed = audio_ok && image_ok && text_ok && facts_ok;
    let score = if !checks_needed && input.case.expected_facts.is_empty() {
        100.0
    } else if parts_total == 0 {
        100.0
    } else {
        (parts_ok.len() as f64 / parts_total as f64) * 100.0
    };

    let mut failures = Vec::new();
    if !audio_ok {
        failures.push("audio modality not proven (transcribe_audio not called)".into());
    }
    if !image_ok {
        failures.push("image modality not proven (describe_image not called)".into());
    }
    if !text_ok {
        failures.push("text modality not proven (search/read fixture not called)".into());
    }
    if !facts_ok {
        failures.push(format!(
            "missing modality fact content: {}",
            missing_fact_ids.join(", ")
        ));
    }

    let evidence = json!({
        "needs_audio": needs_audio,
        "needs_image": needs_image,
        "needs_text": needs_text,
        "audio_ok": audio_ok,
        "image_ok": image_ok,
        "text_ok": text_ok,
        "missing_fact_ids": missing_fact_ids,
        "actual_call_sequence": trace.started_sequence,
        "observed_tools": trace.observed.iter().cloned().collect::<Vec<_>>(),
    });

    Ok(make_result(
        "modality_coverage",
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

pub fn grade_conflict_reconciliation(
    input: &GraderInput<'_>,
    weight: f64,
) -> Result<GraderResult, GraderError> {
    let Some(conflict) = &input.case.expected_conflict else {
        return Ok(make_result(
            "conflict_reconciliation",
            &input.case.case_id,
            true,
            100.0,
            weight,
            json!({"status": "no_expected_conflict"}),
            None,
        ));
    };

    let output = input.output_md;
    let lower = output.to_lowercase();

    let left_val_ok = lower.contains(&conflict.left_value.to_lowercase());
    let right_val_ok = lower.contains(&conflict.right_value.to_lowercase());

    let left_src_base =
        basename_of(&conflict.left_source).unwrap_or_else(|| conflict.left_source.clone());
    let right_src_base =
        basename_of(&conflict.right_source).unwrap_or_else(|| conflict.right_source.clone());

    let left_src_ok = lower.contains(&left_src_base.to_lowercase());
    let right_src_ok = lower.contains(&right_src_base.to_lowercase());

    let left_attributed = value_attributed_to_source(output, &conflict.left_value, &left_src_base);
    let right_attributed =
        value_attributed_to_source(output, &conflict.right_value, &right_src_base);

    let both_present_with_sources = left_val_ok && right_val_ok && left_src_ok && right_src_ok;
    let attributed = (left_attributed && right_attributed) || both_present_with_sources;

    let checks = [
        left_val_ok,
        right_val_ok,
        left_src_ok,
        right_src_ok,
        attributed,
    ];
    let ok = checks.iter().filter(|c| **c).count();
    let passed = left_val_ok && right_val_ok && left_src_ok && right_src_ok && attributed;
    let score = (ok as f64 / checks.len() as f64) * 100.0;

    let mut failures = Vec::new();
    if !left_val_ok {
        failures.push(format!("missing left value '{}'", conflict.left_value));
    }
    if !right_val_ok {
        failures.push(format!("missing right value '{}'", conflict.right_value));
    }
    if !left_src_ok {
        failures.push(format!("missing left source '{left_src_base}'"));
    }
    if !right_src_ok {
        failures.push(format!("missing right source '{right_src_base}'"));
    }
    if left_val_ok && right_val_ok && (!left_src_ok || !right_src_ok || !attributed) {
        failures.push("conflict numbers present without proper source attribution".into());
    }

    let evidence = json!({
        "left_value": conflict.left_value,
        "right_value": conflict.right_value,
        "left_source": left_src_base,
        "right_source": right_src_base,
        "left_val_ok": left_val_ok,
        "right_val_ok": right_val_ok,
        "left_src_ok": left_src_ok,
        "right_src_ok": right_src_ok,
        "left_attributed": left_attributed,
        "right_attributed": right_attributed,
    });

    Ok(make_result(
        "conflict_reconciliation",
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

fn value_attributed_to_source(output: &str, value: &str, source: &str) -> bool {
    let lower = output.to_lowercase();
    let v = value.to_lowercase();
    let s = source.to_lowercase();
    if !(lower.contains(&v) && lower.contains(&s)) {
        return false;
    }
    for line in output.lines() {
        let l = line.to_lowercase();
        if l.contains(&v) && l.contains(&s) {
            return true;
        }
    }
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find(&v) {
        let idx = search_from + rel;
        let start = idx.saturating_sub(160);
        let end = (idx + v.len() + 160).min(lower.len());
        if lower[start..end].contains(&s) {
            return true;
        }
        search_from = idx + v.len();
    }
    false
}

pub fn grade_report_structure(
    input: &GraderInput<'_>,
    weight: f64,
) -> Result<GraderResult, GraderError> {
    let sections = &input.case.report.required_sections;
    let mut missing_sections = Vec::new();
    for section in sections {
        if !output_has_section(input.output_md, section) {
            missing_sections.push(section.clone());
        }
    }

    let mut missing_facts = Vec::new();
    for fact in &input.case.expected_facts {
        if !contains_all(input.output_md, &fact.must_contain) {
            missing_facts.push(fact.id.clone());
        }
    }

    let total = sections.len() + input.case.expected_facts.len();
    let failed = missing_sections.len() + missing_facts.len();
    let passed = failed == 0;
    let score = if total == 0 {
        100.0
    } else {
        ((total - failed) as f64 / total as f64) * 100.0
    };

    let mut failures = Vec::new();
    if !missing_sections.is_empty() {
        failures.push(format!(
            "missing required sections: {}",
            missing_sections.join(", ")
        ));
    }
    if !missing_facts.is_empty() {
        failures.push(format!(
            "missing expected facts: {}",
            missing_facts.join(", ")
        ));
    }

    let evidence = json!({
        "required_sections": sections,
        "missing_sections": missing_sections,
        "missing_facts": missing_facts,
    });

    Ok(make_result(
        "report_structure",
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

pub fn grade_citation_quality(
    input: &GraderInput<'_>,
    weight: f64,
) -> Result<GraderResult, GraderError> {
    let candidates = extract_citation_candidates(input.output_md);
    let inventory = input.fixture_inventory;

    let mut valid = Vec::new();
    let mut missing = Vec::new();
    let mut escaped = Vec::new();
    let mut other_errors = Vec::new();

    for cite in &candidates {
        let base = basename_of(cite).unwrap_or_else(|| cite.clone());
        if !inventory.contains(&base) {
            missing.push(cite.clone());
            continue;
        }
        match resolve_under_fixtures(input.fixtures_dir, cite) {
            Ok(_) => valid.push(cite.clone()),
            Err(err) => {
                if err.contains("outside") || err.contains("..") {
                    escaped.push(json!({"citation": cite, "error": err}));
                } else if err.contains("does not exist") {
                    match resolve_under_fixtures(input.fixtures_dir, &base) {
                        Ok(_) => valid.push(base),
                        Err(e2) => {
                            if e2.contains("outside") || e2.contains("..") {
                                escaped.push(json!({"citation": cite, "error": e2}));
                            } else {
                                missing.push(cite.clone());
                            }
                        }
                    }
                } else {
                    other_errors.push(json!({"citation": cite, "error": err}));
                }
            }
        }
    }

    for cite in &candidates {
        if cite.contains("..") {
            if !escaped.iter().any(|e| e["citation"] == *cite) {
                escaped.push(json!({
                    "citation": cite,
                    "error": "citation contains '..' path traversal",
                }));
            }
            valid.retain(|v| v != cite);
        }
    }

    let required_sources: BTreeSet<String> = input
        .case
        .fixture_refs
        .iter()
        .cloned()
        .chain(
            input
                .case
                .expected_facts
                .iter()
                .filter_map(|f| f.source_fixture.clone()),
        )
        .chain(
            input
                .case
                .expected_conflict
                .iter()
                .flat_map(|c| [c.left_source.clone(), c.right_source.clone()]),
        )
        .collect();

    let source_section = extract_sources_section(input.output_md);
    let mut body_only = Vec::new();
    let mut missing_required = Vec::new();

    let bare_mentions = extract_bare_fixture_mentions(input.output_md);

    for src in &required_sources {
        let base = basename_of(src).unwrap_or_else(|| src.clone());
        let in_candidates = candidates.iter().any(|c| {
            basename_of(c).as_deref() == Some(base.as_str()) || c == &base || c.ends_with(&base)
        });
        let in_sources_section = source_section
            .as_ref()
            .is_some_and(|sec| sec.to_lowercase().contains(&base.to_lowercase()));
        let in_output = input
            .output_md
            .to_lowercase()
            .contains(&base.to_lowercase());
        let bare_only = bare_mentions.iter().any(|b| b == &base) && !in_candidates && !in_sources_section;

        if !inventory.contains(&base) {
            missing_required.push(format!("{base} (not in fixture inventory)"));
            continue;
        }

        // Formal citation = backtick/bracket candidate or listed under Sources.
        if in_candidates || in_sources_section {
            if let Err(err) = resolve_under_fixtures(input.fixtures_dir, &base) {
                if err.contains("outside") || err.contains("..") {
                    escaped.push(json!({"citation": base, "error": err}));
                } else if !err.contains("does not exist") {
                    other_errors.push(json!({"citation": base, "error": err}));
                }
            }
        } else if bare_only || in_output {
            // Named in prose without a source record.
            body_only.push(base);
        } else {
            missing_required.push(base);
        }
    }

    let structural_ok = escaped.is_empty()
        && other_errors.is_empty()
        && body_only.is_empty()
        && missing_required.is_empty()
        && missing.is_empty();

    let mut checks_total = candidates.len() + required_sources.len();
    if checks_total == 0 {
        checks_total = 1;
    }
    let checks_failed = missing.len()
        + escaped.len()
        + other_errors.len()
        + body_only.len()
        + missing_required.len();
    let checks_ok = checks_total.saturating_sub(checks_failed);
    let passed = structural_ok;
    let score = if required_sources.is_empty() && candidates.is_empty() {
        100.0
    } else {
        (checks_ok as f64 / checks_total as f64) * 100.0
    };

    let mut failures = Vec::new();
    if !missing.is_empty() {
        failures.push(format!(
            "citations not in fixture inventory: {}",
            missing.join(", ")
        ));
    }
    if !escaped.is_empty() {
        failures.push(format!(
            "citations escape fixture root: {}",
            serde_json::to_string(&escaped).unwrap_or_default()
        ));
    }
    if !body_only.is_empty() {
        failures.push(format!(
            "fixtures named in body without source record: {}",
            body_only.join(", ")
        ));
    }
    if !missing_required.is_empty() {
        failures.push(format!(
            "required sources missing from citations: {}",
            missing_required.join(", ")
        ));
    }
    if !other_errors.is_empty() {
        failures.push(format!(
            "citation resolution errors: {}",
            serde_json::to_string(&other_errors).unwrap_or_default()
        ));
    }

    let evidence = json!({
        "candidates": candidates,
        "valid": valid,
        "missing_inventory": missing,
        "escaped": escaped,
        "body_only_mentions": body_only,
        "missing_required_sources": missing_required,
        "required_sources": required_sources.iter().cloned().collect::<Vec<_>>(),
        "has_sources_section": source_section.is_some(),
    });

    Ok(make_result(
        "citation_quality",
        &input.case.case_id,
        passed,
        score.clamp(0.0, 100.0),
        weight,
        evidence,
        if passed {
            None
        } else {
            Some(failures.join("; "))
        },
    ))
}

fn extract_sources_section(output: &str) -> Option<String> {
    let lines: Vec<&str> = output.lines().collect();
    let mut start = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let heading = if trimmed.starts_with('#') {
            trimmed
                .trim_start_matches('#')
                .trim()
                .trim_matches(|c: char| c == '*' || c == '_' || c == ':')
                .to_lowercase()
        } else {
            trimmed
                .trim_matches(|c: char| c == '*' || c == '_' || c == ':')
                .to_lowercase()
        };
        if heading == "sources" || heading.starts_with("sources ") {
            start = Some(i + 1);
            break;
        }
    }
    let start = start?;
    let mut buf = String::new();
    for line in &lines[start..] {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            break;
        }
        buf.push_str(line);
        buf.push('\n');
    }
    Some(buf)
}
