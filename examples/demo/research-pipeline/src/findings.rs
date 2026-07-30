//! Typed v1 contract and read-only validation for Research Pipeline evidence.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FINDINGS_KIND: &str = "orchest.research-pipeline.findings";
pub const SCHEMA_VERSION: u32 = 1;

const MAX_TEXT_LENGTH: usize = 4_000;
const MAX_DIAGNOSTIC_LENGTH: usize = 1_000;

#[derive(Debug, Error)]
pub enum FindingsError {
    #[error("invalid findings JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid findings contract: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FindingsDocument {
    pub kind: String,
    pub schema_version: u32,
    pub iteration: String,
    pub subject: String,
    pub executive_summary: String,
    pub readiness_verdict: ReadinessVerdict,
    pub api_checklist: Vec<ChecklistItem>,
    pub runs: Vec<RunEvidence>,
    pub evidence: Vec<Evidence>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReadinessVerdict {
    pub status: ReadinessStatus,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReadinessStatus {
    Ready,
    Conditional,
    Blocked,
    Unverified,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChecklistItem {
    pub id: String,
    pub api_surface: String,
    pub public_path: String,
    pub requirement: String,
    pub status: ChecklistStatus,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub finding_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ChecklistStatus {
    Planned,
    Exercised,
    Failed,
    Blocked,
    NotApplicable,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RunEvidence {
    pub id: String,
    pub kind: RunKind,
    pub status: RunStatus,
    pub required: bool,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub summary: String,
    #[serde(default)]
    pub diagnostic_excerpt: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RunKind {
    Fixture,
    Test,
    Smoke,
    LiveProvider,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    Passed,
    Failed,
    NotRun,
    Partial,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Evidence {
    pub id: String,
    pub kind: EvidenceKind,
    pub summary: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub run_ref: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    Source,
    Test,
    SmokeRun,
    LiveRun,
    Documentation,
    RuntimeOutput,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub title: String,
    pub api_surface: String,
    pub description: String,
    pub observed_consequence: String,
    pub workaround: String,
    pub classification: FindingClassification,
    pub status: FindingStatus,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub action: Option<Action>,
    pub verification: Verification,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FindingClassification {
    Untriaged,
    SeamBlocker,
    ReleaseBlocker,
    #[serde(rename = "post-1.0-backlog")]
    Post10Backlog,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FindingStatus {
    Open,
    Implemented,
    Verified,
    Deferred,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Action {
    pub owner: String,
    pub summary: String,
    #[serde(default)]
    pub issue_ref: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Verification {
    pub status: VerificationStatus,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    NotRun,
    Passed,
    Failed,
    NotApplicable,
}

pub fn validate_json(input: &str) -> Result<FindingsDocument, FindingsError> {
    let document = serde_json::from_str(input)?;
    validate_document(&document)?;
    Ok(document)
}

pub fn validate_document(document: &FindingsDocument) -> Result<(), FindingsError> {
    if document.kind != FINDINGS_KIND {
        return invalid("kind must be orchest.research-pipeline.findings");
    }
    if document.schema_version != SCHEMA_VERSION {
        return invalid("schemaVersion must be 1; unknown versions fail closed");
    }

    required("iteration", &document.iteration)?;
    required("subject", &document.subject)?;
    required("executiveSummary", &document.executive_summary)?;
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

    for evidence in &document.evidence {
        required("evidence id", &evidence.id)?;
        required("evidence summary", &evidence.summary)?;
        if let Some(path) = &evidence.path {
            validate_repository_path(path)?;
        }
        if evidence.kind == EvidenceKind::Source {
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
        }
        if evidence.path.is_none() && evidence.symbol.is_none() && evidence.run_ref.is_none() {
            return invalid(format!("evidence {} needs a stable locator", evidence.id));
        }
        if let Some(run_ref) = &evidence.run_ref {
            ensure_ref("evidence", run_ref, &run_ids, "run")?;
        }
        if evidence.command.is_some() {
            let Some(run_ref) = evidence.run_ref.as_deref() else {
                return invalid(format!(
                    "evidence {} command requires an executed run reference",
                    evidence.id
                ));
            };
            let executed = document
                .runs
                .iter()
                .any(|run| run.id == run_ref && run.status != RunStatus::NotRun);
            if !executed {
                return invalid(format!(
                    "evidence {} command requires an executed run",
                    evidence.id
                ));
            }
        }
    }

    validate_runs(&document.runs, &evidence_ids)?;

    for item in &document.api_checklist {
        required("checklist id", &item.id)?;
        required("apiSurface", &item.api_surface)?;
        required("publicPath", &item.public_path)?;
        required("requirement", &item.requirement)?;
        validate_evidence_refs(
            &format!("checklist {}", item.id),
            &item.evidence_refs,
            &evidence_ids,
        )?;
        validate_finding_refs(
            &format!("checklist {}", item.id),
            &item.finding_refs,
            &finding_ids,
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
    }

    for finding in &document.findings {
        required("finding id", &finding.id)?;
        required("finding title", &finding.title)?;
        required("finding apiSurface", &finding.api_surface)?;
        required("finding description", &finding.description)?;
        required("finding observedConsequence", &finding.observed_consequence)?;
        required("finding workaround", &finding.workaround)?;
        required("verification summary", &finding.verification.summary)?;
        validate_evidence_refs(
            &format!("finding {}", finding.id),
            &finding.evidence_refs,
            &evidence_ids,
        )?;
        validate_evidence_refs(
            &format!("finding {} verification", finding.id),
            &finding.verification.evidence_refs,
            &evidence_ids,
        )?;
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
        if let Some(action) = &finding.action {
            required("action owner", &action.owner)?;
            required("action summary", &action.summary)?;
        }
    }

    validate_readiness(document, &finding_ids, &run_ids)
}

fn validate_runs(runs: &[RunEvidence], evidence_ids: &HashSet<&str>) -> Result<(), FindingsError> {
    for run in runs {
        required("run id", &run.id)?;
        required("run summary", &run.summary)?;
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
        }
        if run.kind != RunKind::LiveProvider && (run.provider.is_some() || run.model.is_some()) {
            return invalid(format!(
                "non-live run {} must omit provider and model",
                run.id
            ));
        }
        if run.status != RunStatus::NotRun {
            let Some(command) = run.command.as_deref() else {
                return invalid(format!("executed run {} requires command", run.id));
            };
            required("executed run command", command)?;
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
        if let Some(diagnostic) = &run.diagnostic_excerpt {
            bounded("diagnosticExcerpt", diagnostic, MAX_DIAGNOSTIC_LENGTH)?;
        }
        validate_evidence_refs(&format!("run {}", run.id), &run.evidence_refs, evidence_ids)?;
    }
    Ok(())
}

pub fn render_markdown(document: &FindingsDocument) -> Result<String, FindingsError> {
    validate_document(document)?;
    let mut checklist = document.api_checklist.iter().collect::<Vec<_>>();
    checklist.sort_by_key(|item| &item.id);
    let mut findings = document.findings.iter().collect::<Vec<_>>();
    findings.sort_by_key(|finding| &finding.id);

    let mut output = format!(
        "# {} seam evidence\n\n## Executive summary\n\n{}\n\n## Readiness\n\n**{}**{}\n\n## Checklist\n\n| ID | Public path | Status |\n| --- | --- | --- |\n",
        document.subject,
        document.executive_summary,
        readiness_name(document.readiness_verdict.status),
        document
            .readiness_verdict
            .reason
            .as_deref()
            .map(|reason| format!(": {reason}"))
            .unwrap_or_default()
    );
    for item in checklist {
        output.push_str(&format!(
            "| {} | `{}` | {:?} |\n",
            item.id, item.public_path, item.status
        ));
    }
    output.push_str(
        "\n## Findings\n\n| ID | Title | Classification | Status |\n| --- | --- | --- | --- |\n",
    );
    for finding in &findings {
        output.push_str(&format!(
            "| {} | {} | {:?} | {:?} |\n",
            finding.id, finding.title, finding.classification, finding.status
        ));
    }
    for finding in findings {
        output.push_str(&format!(
            "\n### {} — {}\n\n{}\n\nConsequence: {}\n\nWorkaround: {}\n",
            finding.id,
            finding.title,
            finding.description,
            finding.observed_consequence,
            finding.workaround
        ));
    }
    Ok(output)
}

pub fn is_safe_repository_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains(':')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn validate_readiness(
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
        if verdict.refs.is_empty() {
            return invalid("non-ready readiness verdict requires reason and refs");
        }
    }
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
    }
    Ok(())
}

fn validate_repository_path(path: &str) -> Result<(), FindingsError> {
    if is_safe_repository_path(path) {
        Ok(())
    } else {
        invalid(format!("unsafe repository path {path:?}"))
    }
}

fn evidence_has_executed_behavior_run(evidence: &Evidence, runs: &[RunEvidence]) -> bool {
    evidence_has_behavior_run(evidence, runs, |status| status != RunStatus::NotRun)
}

fn evidence_has_passed_behavior_run(evidence: &Evidence, runs: &[RunEvidence]) -> bool {
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

fn is_stable_symbol(symbol: &str) -> bool {
    let symbol = symbol.trim();
    !symbol.is_empty() && !is_line_locator(symbol)
}

fn is_line_locator(symbol: &str) -> bool {
    let lower = symbol.trim().to_ascii_lowercase();
    is_decimal(&lower)
        || lower.strip_prefix("line ").is_some_and(is_decimal)
        || lower.strip_prefix('l').is_some_and(is_decimal)
        || lower
            .rsplit_once([':', '#'])
            .is_some_and(|(_, suffix)| suffix_is_line_number(suffix))
}

fn suffix_is_line_number(suffix: &str) -> bool {
    is_decimal(suffix) || suffix.strip_prefix('l').is_some_and(is_decimal)
}

fn is_decimal(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
}

fn validate_unique<'a>(
    kind: &str,
    ids: impl Iterator<Item = &'a String>,
) -> Result<(), FindingsError> {
    let mut seen = HashSet::new();
    for id in ids {
        required(&format!("{kind} id"), id)?;
        if !seen.insert(id.as_str()) {
            return invalid(format!("duplicate {kind} id {id}"));
        }
    }
    Ok(())
}

fn validate_evidence_refs(
    owner: &str,
    refs: &[String],
    evidence_ids: &HashSet<&str>,
) -> Result<(), FindingsError> {
    for reference in refs {
        ensure_ref(owner, reference, evidence_ids, "evidence")?;
    }
    Ok(())
}

fn validate_finding_refs(
    owner: &str,
    refs: &[String],
    finding_ids: &HashSet<&str>,
) -> Result<(), FindingsError> {
    for reference in refs {
        ensure_ref(owner, reference, finding_ids, "finding")?;
    }
    Ok(())
}

fn ensure_ref(
    owner: &str,
    reference: &str,
    known_ids: &HashSet<&str>,
    target: &str,
) -> Result<(), FindingsError> {
    if known_ids.contains(reference) {
        Ok(())
    } else {
        invalid(format!("{owner} has unknown {target} ref {reference}"))
    }
}

fn required(name: &str, value: &str) -> Result<(), FindingsError> {
    bounded(name, value, MAX_TEXT_LENGTH)?;
    if value.trim().is_empty() {
        invalid(format!("{name} is required"))
    } else {
        Ok(())
    }
}

fn bounded(name: &str, value: &str, maximum: usize) -> Result<(), FindingsError> {
    if value.len() > maximum {
        invalid(format!("{name} exceeds {maximum} characters"))
    } else {
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> Result<(), FindingsError> {
    Err(FindingsError::Validation(message.into()))
}

fn readiness_name(status: ReadinessStatus) -> &'static str {
    match status {
        ReadinessStatus::Ready => "ready",
        ReadinessStatus::Conditional => "conditional",
        ReadinessStatus::Blocked => "blocked",
        ReadinessStatus::Unverified => "unverified",
    }
}
