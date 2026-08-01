use std::collections::{HashMap, HashSet};

use super::*;

mod lifecycle;
mod refs;
mod safety;

pub use safety::is_safe_repository_path;

pub fn validate_json(input: &str) -> Result<FindingsDocument, FindingsError> {
    let document = serde_json::from_str(input)?;
    validate_document(&document)?;
    Ok(document)
}

pub fn validate_document(document: &FindingsDocument) -> Result<(), FindingsError> {
    use lifecycle::{validate_final_triage, validate_findings, validate_readiness};
    use refs::{validate_checklist, validate_evidence, validate_runs};
    use safety::{
        invalid, required, validate_private_text, validate_rendered_strings, validate_unique,
    };

    if document.kind != FINDINGS_KIND {
        return invalid("kind must be orchest.research-pipeline.findings");
    }
    if document.schema_version != SCHEMA_VERSION {
        return invalid("schemaVersion must be 1; unknown versions fail closed");
    }
    validate_rendered_strings(document)?;

    required("iteration", &document.iteration)?;
    required("subject", &document.subject)?;
    required("executiveSummary", &document.executive_summary)?;
    validate_private_text("executiveSummary", &document.executive_summary)?;
    validate_unique(
        "checklist",
        document.api_checklist.iter().map(|item| &item.id),
    )?;
    validate_unique("run", document.runs.iter().map(|run| &run.id))?;
    validate_unique(
        "evidence",
        document.evidence.iter().map(|evidence| &evidence.id),
    )?;
    validate_unique(
        "finding",
        document.findings.iter().map(|finding| &finding.id),
    )?;

    let evidence_ids = document
        .evidence
        .iter()
        .map(|evidence| evidence.id.as_str())
        .collect::<HashSet<_>>();
    let finding_ids = document
        .findings
        .iter()
        .map(|finding| finding.id.as_str())
        .collect::<HashSet<_>>();
    let run_ids = document
        .runs
        .iter()
        .map(|run| run.id.as_str())
        .collect::<HashSet<_>>();
    let runs_by_id = document
        .runs
        .iter()
        .map(|run| (run.id.as_str(), run))
        .collect::<HashMap<_, _>>();
    let evidence_by_id = document
        .evidence
        .iter()
        .map(|evidence| (evidence.id.as_str(), evidence))
        .collect::<HashMap<_, _>>();

    validate_evidence(document, &run_ids, &runs_by_id)?;
    validate_runs(&document.runs, &evidence_ids, &evidence_by_id)?;
    validate_checklist(document, &evidence_ids, &finding_ids)?;
    validate_findings(document, &evidence_ids, &evidence_by_id, &runs_by_id)?;
    validate_readiness(document, &finding_ids, &run_ids)?;
    validate_final_triage(document)
}
