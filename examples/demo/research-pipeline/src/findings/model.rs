use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FINDINGS_KIND: &str = "orchest.research-pipeline.findings";
pub const SCHEMA_VERSION: u32 = 1;

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
