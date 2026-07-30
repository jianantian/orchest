use std::collections::{HashMap, HashSet};

use super::{super::*, refs::evidence_has_passed_behavior_run, safety::*};

pub(super) fn validate_findings(
    document: &FindingsDocument,
    evidence_ids: &HashSet<&str>,
    evidence_by_id: &HashMap<&str, &Evidence>,
    runs_by_id: &HashMap<&str, &RunEvidence>,
) -> Result<(), FindingsError> {
    for finding in &document.findings {
        required("finding id", &finding.id)?;
        required("finding title", &finding.title)?;
        required("finding apiSurface", &finding.api_surface)?;
        required("finding description", &finding.description)?;
        required("finding observedConsequence", &finding.observed_consequence)?;
        required("finding workaround", &finding.workaround)?;
        required("verification summary", &finding.verification.summary)?;
        validate_private_text("finding description", &finding.description)?;
        validate_private_text("finding observedConsequence", &finding.observed_consequence)?;
        validate_private_text("finding workaround", &finding.workaround)?;
        validate_private_text("verification summary", &finding.verification.summary)?;
        if finding.evidence_refs.is_empty() {
            return invalid(format!(
                "finding {} requires evidence references",
                finding.id
            ));
        }
        validate_evidence_refs(
            &format!("finding {}", finding.id),
            &finding.evidence_refs,
            evidence_ids,
        )?;
        validate_evidence_refs(
            &format!("finding {} verification", finding.id),
            &finding.verification.evidence_refs,
            evidence_ids,
        )?;
        validate_distinct_refs(
            &format!("finding {} evidenceRefs", finding.id),
            &finding.evidence_refs,
        )?;
        validate_distinct_refs(
            &format!("finding {} verification evidenceRefs", finding.id),
            &finding.verification.evidence_refs,
        )?;
        validate_finding_lifecycle(finding, document)?;
        validate_verification(finding, evidence_by_id, runs_by_id)?;
        validate_action(finding)?;
    }
    Ok(())
}

fn validate_finding_lifecycle(
    finding: &Finding,
    document: &FindingsDocument,
) -> Result<(), FindingsError> {
    if finding.status == FindingStatus::Verified
        && finding.verification.status != VerificationStatus::Passed
    {
        return invalid(format!(
            "verified finding {} requires passed verification evidence",
            finding.id
        ));
    }
    if finding.verification.status == VerificationStatus::Passed
        && !finding.verification.evidence_refs.iter().any(|reference| {
            document.evidence.iter().any(|evidence| {
                evidence.id == *reference
                    && evidence_has_passed_behavior_run(evidence, &document.runs)
            })
        })
    {
        return invalid(format!(
            "verification passed for finding {} requires passed verification evidence",
            finding.id
        ));
    }
    if finding.status == FindingStatus::Implemented
        && finding.verification.status == VerificationStatus::Passed
    {
        return invalid(format!(
            "implemented finding {} cannot claim passed verification",
            finding.id
        ));
    }
    Ok(())
}

fn validate_action(finding: &Finding) -> Result<(), FindingsError> {
    if let Some(action) = &finding.action {
        required("action owner", &action.owner)?;
        required("action summary", &action.summary)?;
        validate_private_text("action summary", &action.summary)?;
        if let Some(issue_ref) = &action.issue_ref {
            validate_issue_ref(issue_ref)?;
        }
        if let Some(revision) = &action.revision {
            validate_revision("action revision", revision, false)?;
        }
    }
    Ok(())
}

fn validate_verification(
    finding: &Finding,
    evidence_by_id: &HashMap<&str, &Evidence>,
    runs_by_id: &HashMap<&str, &RunEvidence>,
) -> Result<(), FindingsError> {
    let verification = &finding.verification;
    match verification.status {
        VerificationStatus::NotRun => {
            if !verification.commands.is_empty() {
                return invalid(format!(
                    "not-run verification for finding {} must omit commands",
                    finding.id
                ));
            }
        }
        VerificationStatus::NotApplicable => {
            if !verification.commands.is_empty() {
                return invalid(format!(
                    "not-applicable verification for finding {} must omit commands",
                    finding.id
                ));
            }
            if verification.evidence_refs.is_empty() {
                return invalid(format!(
                    "not-applicable verification for finding {} requires evidence",
                    finding.id
                ));
            }
        }
        VerificationStatus::Passed | VerificationStatus::Failed => {
            validate_executed_verification(finding, verification, evidence_by_id, runs_by_id)?;
        }
    }
    Ok(())
}

fn validate_executed_verification(
    finding: &Finding,
    verification: &Verification,
    evidence_by_id: &HashMap<&str, &Evidence>,
    runs_by_id: &HashMap<&str, &RunEvidence>,
) -> Result<(), FindingsError> {
    if verification.commands.is_empty() {
        return invalid(format!(
            "{} verification for finding {} requires commands",
            verification_status_name(verification.status),
            finding.id
        ));
    }
    for command in &verification.commands {
        required("verification command", command)?;
        validate_private_text("verification command", command)?;
        let has_matching_run = verification.evidence_refs.iter().any(|reference| {
            matching_verification_run(
                command,
                verification.status,
                reference,
                evidence_by_id,
                runs_by_id,
            )
        });
        if !has_matching_run {
            return invalid(format!(
                "verification command {command:?} for finding {} requires matching executed evidence",
                finding.id
            ));
        }
    }
    Ok(())
}

fn matching_verification_run(
    command: &str,
    status: VerificationStatus,
    reference: &str,
    evidence_by_id: &HashMap<&str, &Evidence>,
    runs_by_id: &HashMap<&str, &RunEvidence>,
) -> bool {
    let Some(evidence) = evidence_by_id.get(reference).copied() else {
        return false;
    };
    if evidence.command.as_deref() != Some(command) {
        return false;
    }
    let Some(run_ref) = evidence.run_ref.as_deref() else {
        return false;
    };
    let Some(run) = runs_by_id.get(run_ref).copied() else {
        return false;
    };
    match status {
        VerificationStatus::Passed => run.status == RunStatus::Passed,
        VerificationStatus::Failed => matches!(run.status, RunStatus::Failed | RunStatus::Partial),
        VerificationStatus::NotRun | VerificationStatus::NotApplicable => false,
    }
}

pub(super) fn validate_readiness(
    document: &FindingsDocument,
    finding_ids: &HashSet<&str>,
    run_ids: &HashSet<&str>,
) -> Result<(), FindingsError> {
    let verdict = &document.readiness_verdict;
    if verdict.status != ReadinessStatus::Ready {
        let Some(reason) = verdict.reason.as_deref() else {
            return invalid("readiness reason is required");
        };
        required("readiness reason", reason)?;
        validate_private_text("readiness reason", reason)?;
        if verdict.refs.is_empty() {
            return invalid("non-ready readiness verdict requires reason and refs");
        }
    }
    validate_distinct_refs("readiness refs", &verdict.refs)?;
    for reference in &verdict.refs {
        if !finding_ids.contains(reference.as_str()) && !run_ids.contains(reference.as_str()) {
            return invalid(format!(
                "readiness has unknown finding or run ref {reference}"
            ));
        }
    }

    let required_live_not_run = document.runs.iter().any(|run| {
        run.required && run.kind == RunKind::LiveProvider && run.status == RunStatus::NotRun
    });
    if required_live_not_run && verdict.status != ReadinessStatus::Unverified {
        return invalid("required live-provider run not-run forces readiness to unverified");
    }
    if verdict.status == ReadinessStatus::Ready {
        validate_ready(document)?;
    }
    Ok(())
}

fn validate_ready(document: &FindingsDocument) -> Result<(), FindingsError> {
    if document.findings.iter().any(|finding| {
        finding.status == FindingStatus::Open
            && matches!(
                finding.classification,
                FindingClassification::SeamBlocker | FindingClassification::ReleaseBlocker
            )
    }) {
        return invalid("ready verdict has an open blocker");
    }
    if document.api_checklist.iter().any(|item| {
        !matches!(
            item.status,
            ChecklistStatus::Exercised | ChecklistStatus::NotApplicable
        )
    }) {
        return invalid(
            "ready verdict requires every checklist item to be exercised or not-applicable",
        );
    }
    Ok(())
}

pub(super) fn validate_final_triage(document: &FindingsDocument) -> Result<(), FindingsError> {
    for finding in &document.findings {
        if finding.classification == FindingClassification::Untriaged {
            return invalid(format!(
                "finding {} requires a final classification",
                finding.id
            ));
        }
        validate_owned_open_blocker(finding)?;
        validate_owned_deferral(finding)?;
    }
    Ok(())
}

fn validate_owned_open_blocker(finding: &Finding) -> Result<(), FindingsError> {
    let open_blocker = finding.status == FindingStatus::Open
        && matches!(
            finding.classification,
            FindingClassification::SeamBlocker | FindingClassification::ReleaseBlocker
        );
    if !open_blocker {
        return Ok(());
    }
    let Some(action) = &finding.action else {
        return invalid(format!(
            "open blocker {} requires action owner, summary, and issueRef",
            finding.id
        ));
    };
    if action.issue_ref.is_none() {
        return invalid(format!(
            "open blocker {} requires action issueRef",
            finding.id
        ));
    }
    Ok(())
}

fn validate_owned_deferral(finding: &Finding) -> Result<(), FindingsError> {
    if finding.status != FindingStatus::Deferred {
        return Ok(());
    }
    let Some(action) = &finding.action else {
        return invalid(format!(
            "deferred finding {} requires an owned decision and issueRef",
            finding.id
        ));
    };
    if action.issue_ref.is_none() {
        return invalid(format!(
            "deferred finding {} requires an owned decision issueRef",
            finding.id
        ));
    }
    Ok(())
}
