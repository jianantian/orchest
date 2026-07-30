use std::collections::{HashMap, HashSet};

use super::{super::*, safety::*};

pub(super) fn validate_evidence(
    document: &FindingsDocument,
    run_ids: &HashSet<&str>,
    runs_by_id: &HashMap<&str, &RunEvidence>,
) -> Result<(), FindingsError> {
    for evidence in &document.evidence {
        required("evidence id", &evidence.id)?;
        required("evidence summary", &evidence.summary)?;
        validate_private_text("evidence summary", &evidence.summary)?;
        if let Some(path) = &evidence.path {
            required("evidence path", path)?;
            validate_repository_path(path)?;
        }
        if let Some(symbol) = &evidence.symbol {
            required("evidence symbol", symbol)?;
        }
        if evidence.kind == EvidenceKind::Source {
            validate_source_evidence(evidence)?;
        }
        if evidence.path.is_none() && evidence.symbol.is_none() && evidence.run_ref.is_none() {
            return invalid(format!("evidence {} needs a stable locator", evidence.id));
        }
        if let Some(run_ref) = &evidence.run_ref {
            ensure_ref("evidence", run_ref, run_ids, "run")?;
            let run = map_get(runs_by_id, run_ref, "run")?;
            if !run
                .evidence_refs
                .iter()
                .any(|reference| reference == &evidence.id)
            {
                return invalid(format!(
                    "run {run_ref} must list evidence {} in evidenceRefs",
                    evidence.id
                ));
            }
        }
        validate_evidence_execution(evidence, runs_by_id)?;
    }
    Ok(())
}

fn validate_source_evidence(evidence: &Evidence) -> Result<(), FindingsError> {
    let Some(symbol) = evidence.symbol.as_deref() else {
        return invalid(format!(
            "source evidence {} requires path and symbol",
            evidence.id
        ));
    };
    if evidence.path.is_none() || symbol.trim().is_empty() {
        return invalid(format!(
            "source evidence {} requires path and symbol",
            evidence.id
        ));
    }
    if !is_stable_symbol(symbol) {
        return invalid(format!(
            "source evidence {} requires a stable symbol, not a line locator",
            evidence.id
        ));
    }
    Ok(())
}

fn validate_evidence_execution(
    evidence: &Evidence,
    runs_by_id: &HashMap<&str, &RunEvidence>,
) -> Result<(), FindingsError> {
    if let Some(command) = &evidence.command {
        required("evidence command", command)?;
        validate_private_text("evidence command", command)?;
        let Some(run_ref) = evidence.run_ref.as_deref() else {
            return invalid(format!(
                "evidence {} command requires an executed run reference",
                evidence.id
            ));
        };
        let run = map_get(runs_by_id, run_ref, "run")?;
        if run.status == RunStatus::NotRun {
            return invalid(format!(
                "evidence {} command requires an executed run",
                evidence.id
            ));
        }
        if run.command.as_deref() != Some(command.as_str()) {
            return invalid(format!(
                "evidence {} command must match run {run_ref}",
                evidence.id
            ));
        }
    }
    if let Some(result) = &evidence.result {
        required("evidence result", result)?;
        bounded("evidence result", result, MAX_DIAGNOSTIC_LENGTH)?;
        validate_private_text("evidence result", result)?;
    }
    if matches!(
        evidence.kind,
        EvidenceKind::Test | EvidenceKind::SmokeRun | EvidenceKind::LiveRun
    ) && (evidence.run_ref.is_none() || evidence.command.is_none() || evidence.result.is_none())
    {
        return invalid(format!(
            "behavior evidence {} requires runRef, command, and result",
            evidence.id
        ));
    }
    if let Some(run_ref) = evidence.run_ref.as_deref() {
        let run = map_get(runs_by_id, run_ref, "run")?;
        let kind_matches = matches!(
            (evidence.kind, run.kind),
            (EvidenceKind::Test, RunKind::Test)
                | (EvidenceKind::SmokeRun, RunKind::Smoke)
                | (EvidenceKind::LiveRun, RunKind::LiveProvider)
                | (EvidenceKind::RuntimeOutput, _)
        );
        if !kind_matches {
            return invalid(format!(
                "evidence {} kind does not match run {run_ref}",
                evidence.id
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_runs(
    runs: &[RunEvidence],
    evidence_ids: &HashSet<&str>,
    evidence_by_id: &HashMap<&str, &Evidence>,
) -> Result<(), FindingsError> {
    for run in runs {
        required("run id", &run.id)?;
        required("run summary", &run.summary)?;
        validate_private_text("run summary", &run.summary)?;
        validate_distinct_refs(&format!("run {} evidenceRefs", run.id), &run.evidence_refs)?;
        validate_run_provider(run)?;
        validate_execution_fields(run, evidence_by_id)?;
        if let Some(diagnostic) = &run.diagnostic_excerpt {
            bounded("diagnosticExcerpt", diagnostic, MAX_DIAGNOSTIC_LENGTH)?;
            validate_private_text("diagnosticExcerpt", diagnostic)?;
        }
        validate_evidence_refs(&format!("run {}", run.id), &run.evidence_refs, evidence_ids)?;
        for evidence_ref in &run.evidence_refs {
            let evidence = map_get(evidence_by_id, evidence_ref, "evidence")?;
            if evidence.run_ref.as_deref() != Some(run.id.as_str()) {
                return invalid(format!(
                    "run {} evidence {evidence_ref} must point back through runRef",
                    run.id
                ));
            }
        }
    }
    Ok(())
}

fn validate_run_provider(run: &RunEvidence) -> Result<(), FindingsError> {
    if run.kind == RunKind::LiveProvider {
        let Some(provider) = run.provider.as_deref() else {
            return invalid(format!(
                "live-provider run {} requires provider and model",
                run.id
            ));
        };
        let Some(model) = run.model.as_deref() else {
            return invalid(format!(
                "live-provider run {} requires provider and model",
                run.id
            ));
        };
        required("live-provider provider", provider)?;
        required("live-provider model", model)?;
    } else if run.provider.is_some() || run.model.is_some() {
        return invalid(format!(
            "non-live run {} must omit provider and model",
            run.id
        ));
    }
    Ok(())
}

fn validate_execution_fields(
    run: &RunEvidence,
    evidence_by_id: &HashMap<&str, &Evidence>,
) -> Result<(), FindingsError> {
    if run.status != RunStatus::NotRun {
        let Some(command) = run.command.as_deref() else {
            return invalid(format!("executed run {} requires command", run.id));
        };
        required("executed run command", command)?;
        validate_private_text("executed run command", command)?;
        let Some(date) = run.date.as_deref() else {
            return invalid(format!("executed run {} requires date", run.id));
        };
        validate_date(date)?;
        let Some(revision) = run.revision.as_deref() else {
            return invalid(format!("executed run {} requires revision", run.id));
        };
        validate_revision("executed run revision", revision, true)?;
        if revision == "git:self" {
            let has_verifier_evidence = run.evidence_refs.iter().any(|reference| {
                evidence_by_id
                    .get(reference.as_str())
                    .is_some_and(|evidence| {
                        evidence.run_ref.as_deref() == Some(run.id.as_str())
                            && evidence.command.as_deref() == run.command.as_deref()
                            && evidence
                                .result
                                .as_deref()
                                .is_some_and(|result| !result.is_empty())
                    })
            });
            if run.status != RunStatus::Passed || !has_verifier_evidence {
                return invalid(format!(
                    "run {} may use git:self only for passed post-commit verification evidence",
                    run.id
                ));
            }
        }
    }
    if run.status == RunStatus::NotRun
        && (run.command.is_some()
            || run.date.is_some()
            || run.revision.is_some()
            || run.diagnostic_excerpt.is_some()
            || !run.evidence_refs.is_empty())
    {
        return invalid(format!(
            "not-run run {} must omit execution-only fields",
            run.id
        ));
    }
    Ok(())
}

pub(super) fn validate_checklist(
    document: &FindingsDocument,
    evidence_ids: &HashSet<&str>,
    finding_ids: &HashSet<&str>,
) -> Result<(), FindingsError> {
    for item in &document.api_checklist {
        required("checklist id", &item.id)?;
        required("apiSurface", &item.api_surface)?;
        required("publicPath", &item.public_path)?;
        required("requirement", &item.requirement)?;
        validate_evidence_refs(
            &format!("checklist {}", item.id),
            &item.evidence_refs,
            evidence_ids,
        )?;
        validate_finding_refs(
            &format!("checklist {}", item.id),
            &item.finding_refs,
            finding_ids,
        )?;
        validate_distinct_refs(
            &format!("checklist {} evidenceRefs", item.id),
            &item.evidence_refs,
        )?;
        validate_distinct_refs(
            &format!("checklist {} findingRefs", item.id),
            &item.finding_refs,
        )?;
        if item.status == ChecklistStatus::Exercised
            && !item.evidence_refs.iter().any(|reference| {
                document.evidence.iter().any(|evidence| {
                    evidence.id == *reference
                        && evidence_has_executed_behavior_run(evidence, &document.runs)
                })
            })
        {
            return invalid(format!(
                "exercised checklist {} needs an executed test, smoke, or live run",
                item.id
            ));
        }
        if item.status == ChecklistStatus::NotApplicable && item.evidence_refs.is_empty() {
            return invalid(format!(
                "not-applicable checklist {} needs evidence",
                item.id
            ));
        }
        if matches!(
            item.status,
            ChecklistStatus::Failed | ChecklistStatus::Blocked
        ) && item.finding_refs.is_empty()
        {
            return invalid(format!(
                "{} checklist {} requires a finding reference",
                checklist_status_name(item.status),
                item.id
            ));
        }
    }
    Ok(())
}

pub(super) fn evidence_has_executed_behavior_run(
    evidence: &Evidence,
    runs: &[RunEvidence],
) -> bool {
    evidence_has_behavior_run(evidence, runs, |status| status != RunStatus::NotRun)
}

pub(super) fn evidence_has_passed_behavior_run(evidence: &Evidence, runs: &[RunEvidence]) -> bool {
    evidence_has_behavior_run(evidence, runs, |status| status == RunStatus::Passed)
}

fn evidence_has_behavior_run(
    evidence: &Evidence,
    runs: &[RunEvidence],
    accepts_status: impl Fn(RunStatus) -> bool,
) -> bool {
    let Some(run_ref) = evidence.run_ref.as_deref() else {
        return false;
    };
    runs.iter().any(|run| {
        run.id == run_ref
            && accepts_status(run.status)
            && matches!(
                (evidence.kind, run.kind),
                (EvidenceKind::Test, RunKind::Test)
                    | (EvidenceKind::SmokeRun, RunKind::Smoke)
                    | (EvidenceKind::LiveRun, RunKind::LiveProvider)
            )
    })
}
