//! Skill manifest, dependency, and capability data types.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tool::{JsonSchema, ToolMetadata, ToolSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    /// The skill manifest file the scanner actually parsed (`SKILL.md` or
    /// lowercase `skill.md`), resolved against the canonicalized skill
    /// directory. Telemetry registration uses this instead of assuming an
    /// uppercase filename.
    #[serde(default)]
    pub skill_md_path: PathBuf,
    /// Tools the skill declares it may use (official `allowed-tools` /
    /// `allowed_tools`, both spellings accepted). **Declared-reserved: parsed
    /// but not enforced** — the runtime does not restrict the session's tool
    /// set based on this field. Whether to enforce it is a post-1.0 decision.
    pub allowed_tools: Option<Vec<String>>,
    pub bundled_tools: Vec<BundledToolDef>,
    #[serde(default)]
    pub dependencies: SkillDependencies,
    /// Resource declarations. Only `env` is enforced (the script executor
    /// clears the process environment and injects only declared variables).
    /// `network` / `filesystem_read` / `filesystem_write` / `max_memory_mb`
    /// are **declared-reserved: parsed but not enforced** — there is no
    /// sandbox yet, so they must not be relied on for containment.
    #[serde(default)]
    pub capabilities: Option<SkillCapabilities>,
    pub raw_frontmatter: Value,
}

impl SkillManifest {
    pub fn build_tool_metadata(&self, _tool: &BundledToolDef) -> ToolMetadata {
        ToolMetadata {
            side_effect: false,
            approval: crate::tool::Approval::Never,
            source: ToolSource::Skill {
                skill_name: self.name.clone(),
            },
            ..ToolMetadata::default()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDependencies {
    #[serde(default)]
    pub python: Vec<String>,
    #[serde(default)]
    pub node: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillCapabilities {
    #[serde(default)]
    pub network: bool,
    #[serde(default, rename = "filesystem_read")]
    pub filesystem_read: Vec<PathBuf>,
    #[serde(default, rename = "filesystem_write")]
    pub filesystem_write: Vec<PathBuf>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledToolDef {
    pub name: String,
    pub description: String,
    pub executable: String,
    pub script: PathBuf,
    pub input_schema: JsonSchema,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("failed to read directory: {0}")]
    Io(#[from] std::io::Error),
}

/// Result of a skill directory scan: successfully parsed manifests plus a
/// structured warning for every problem found. Warnings cover both skills
/// that failed to load (unreadable or invalid SKILL.md) and loaded skills
/// that violate the Agent Skills spec (name/description rules) — see
/// [`skill_name_violations`]. Spec-violating skills still load, so a warning
/// does not imply the skill is absent from `manifests`.
#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub manifests: Vec<SkillManifest>,
    pub warnings: Vec<ScanWarning>,
}

/// A single skill scan problem: the SKILL.md it concerns and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanWarning {
    pub path: PathBuf,
    pub reason: String,
}

/// Validates a skill `name` against the naming rules of the Agent Skills
/// specification and returns every violation found (empty when the name is
/// valid):
///
/// - 1–64 characters
/// - lowercase ASCII letters, digits, and hyphens only (kebab-case)
/// - must not start or end with a hyphen
/// - must not contain consecutive hyphens
///
/// The spec additionally requires the name to match the skill's directory
/// name; that check needs filesystem context and lives in the scanner.
pub fn skill_name_violations(name: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let len = name.chars().count();
    if !(1..=64).contains(&len) {
        violations.push(format!("name must be 1-64 characters (got {len})"));
    }
    if !name.is_empty() {
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            violations.push(
                "name must contain only lowercase letters, digits, and hyphens (kebab-case)"
                    .to_string(),
            );
        }
        if name.starts_with('-') || name.ends_with('-') {
            violations.push("name must not start or end with a hyphen".to_string());
        }
        if name.contains("--") {
            violations.push("name must not contain consecutive hyphens".to_string());
        }
    }
    violations
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct EnvError {
    pub message: String,
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_name_violations_accepts_spec_examples() {
        for name in [
            "pdf-processing",
            "data-analysis",
            "code-review",
            "a",
            "1st-pass",
        ] {
            assert!(
                skill_name_violations(name).is_empty(),
                "'{name}' should be valid"
            );
        }
        // Exactly 64 characters is still valid.
        assert!(skill_name_violations(&"a".repeat(64)).is_empty());
    }

    #[test]
    fn skill_name_violations_flags_spec_counterexamples() {
        // From the spec's invalid examples.
        assert!(skill_name_violations("PDF-Processing")
            .iter()
            .any(|v| v.contains("kebab-case")));
        assert!(skill_name_violations("-pdf")
            .iter()
            .any(|v| v.contains("start or end")));
        assert!(skill_name_violations("pdf-")
            .iter()
            .any(|v| v.contains("start or end")));
        assert!(skill_name_violations("pdf--processing")
            .iter()
            .any(|v| v.contains("consecutive hyphens")));
    }

    #[test]
    fn skill_name_violations_flags_length_and_charset() {
        assert!(skill_name_violations("")
            .iter()
            .any(|v| v.contains("1-64 characters")));
        assert!(skill_name_violations(&"a".repeat(65))
            .iter()
            .any(|v| v.contains("1-64 characters")));
        assert!(skill_name_violations("pdf_processing")
            .iter()
            .any(|v| v.contains("kebab-case")));
        // Path separators are charset violations — this is what keeps
        // `../x` and `a/b` out of the env cache path.
        assert!(skill_name_violations("../x")
            .iter()
            .any(|v| v.contains("kebab-case")));
        assert!(skill_name_violations("a/b")
            .iter()
            .any(|v| v.contains("kebab-case")));
    }
}
